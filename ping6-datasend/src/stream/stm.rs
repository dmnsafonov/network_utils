use ::std::cell::*;
use ::std::net::SocketAddrV6;
use ::std::num::Wrapping;
use ::std::time::Duration;

use ::futures::future;
use ::futures::prelude::*;
use ::futures::Stream;
use ::futures::stream::*;
use ::pnet_packet::Packet;
use ::state_machine_future::RentToOwn;
use ::tokio::prelude::*;

use ::linux_network::*;
use ::ping6_datacommon::*;
use ::sliceable_rcref::SArcRef;

use ::config::Config;
use ::errors::{Error, ErrorKind};
use ::send_box::SendBox;
use ::stdin_iterator::StdinBytesReader;
use ::stream::buffers::AckWaitlist;
use ::stream::packet::*;

type FutureE<T> = ::futures::Future<Item = T, Error = Error>;
type StreamE<T> = ::futures::stream::Stream<Item = T, Error = Error>;

// TODO: tune or make configurable
const TMP_BUFFER_SIZE: usize = 64 * 1024;

#[derive(StateMachineFuture)]
pub enum StreamMachine<'s> {
    #[state_machine_future(start, transitions(SendFirstSyn))]
    InitState {
        common: StreamCommonState<'s>
    },

    #[state_machine_future(transitions(WaitForSynAck))]
    SendFirstSyn {
        common: StreamCommonState<'s>,
        send: futures::IpV6RawSocketSendtoFuture,
        next_action: Option<SendBox<StreamE<
            TimedResult<(futures::U8Slice, SocketAddrV6)>
        >>>
    },

    #[state_machine_future(transitions(SendFirstSyn, SendAck))]
    WaitForSynAck {
        common: StreamCommonState<'s>,
        recv_stream: SendBox<StreamE<
            TimedResult<(futures::U8Slice, SocketAddrV6)>
        >>
    },

    #[state_machine_future(transitions(SendData))]
    SendAck {
        common: StreamCommonState<'s>,
        send_ack: futures::IpV6RawSocketSendtoFuture
    },

    #[state_machine_future(transitions(ReceivedServerFin, SendFin, WaitForAck))]
    SendData {
        common: StreamCommonState<'s>,
        tmp_buf: RefCell<Vec<u8>>,
        next_data: Cell<Option<TrimmingBufferSlice>>
    },

    #[state_machine_future(transitions(ReceivedServerFin, SendData, SendFin))]
    WaitForAck {
        common: StreamCommonState<'s>,
    },

    #[state_machine_future(transitions(SendFinAck))]
    ReceivedServerFin {
        common: StreamCommonState<'s>,
    },

    #[state_machine_future(transitions(ReceivedServerFin, WaitForLastAck))]
    SendFinAck {
        common: StreamCommonState<'s>
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    WaitForLastAck {
        common: StreamCommonState<'s>
    },

    #[state_machine_future(transitions(WaitForFinAck))]
    SendFin {
        common: StreamCommonState<'s>
    },

    #[state_machine_future(transitions(SendFin, SendLastAck))]
    WaitForFinAck {
        common: StreamCommonState<'s>
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    SendLastAck {
        common: StreamCommonState<'s>
    },

    #[state_machine_future(ready)]
    ConnectionTerminated(TerminationReason),

    #[state_machine_future(error)]
    ErrorState(Error)
}

pub enum TerminationReason {
    DataSent,
    ServerFin
}

impl<'s> PollStreamMachine<'s> for StreamMachine<'s> {
    fn poll_init_state<'a>(
        state: &'a mut RentToOwn<'a, InitState<'s>>
    ) -> Poll<AfterInitState<'s>, Error> {
        let mut common = state.take().common;

        let send_future = make_first_syn_future(&mut common);
        transition!(SendFirstSyn {
            common: common,
            send: send_future,
            next_action: None
        })
    }

    fn poll_send_first_syn<'a>(
        state: &'a mut RentToOwn<'a, SendFirstSyn<'s>>
    ) -> Poll<AfterSendFirstSyn<'s>, Error> {
        let size = try_ready!(state.send.poll());
        debug_assert!(size == STREAM_CLIENT_FULL_HEADER_SIZE as usize);

        let state = state.take();
        let mut common = state.common;

        let timed_packets = state.next_action.unwrap_or_else(|| {
            let seqno = common.next_seqno;
            let packets = make_recv_packets_stream(&mut common)
                .filter(move |&(ref x, _)| {
                    let data_ref = x.lock();
                    let packet = parse_stream_server_packet(&data_ref);

                    packet.flags.test(StreamPacketFlags::Syn)
                            && packet.flags.test(StreamPacketFlags::Ack)
                            && !packet.flags.test(StreamPacketFlags::Fin)
                        && packet.seqno_start == packet.seqno_end
                        && packet.seqno_start == seqno.0
                });
            let timed = TimeoutResultStream::new(
                packets,
                Duration::from_millis(PACKET_LOSS_TIMEOUT as u64)
            );
            unsafe {
                SendBox::new(Box::new(
                    timed.take(RETRANSMISSIONS_NUMBER as u64)
                ))
            }
        });

        transition!(WaitForSynAck {
            common: common,
            recv_stream: timed_packets
        })
    }

    fn poll_wait_for_syn_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForSynAck<'s>>
    ) -> Poll<AfterWaitForSynAck<'s>, Error> {
        let (data_ref, dst) = match state.recv_stream.poll() {
            Err(e) => bail!(e),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(Some(TimedResult::InTime(x)))) => x,
            Ok(Async::Ready(Some(TimedResult::TimedOut))) => {
                let mut st = state.take();
                let send_future =
                    make_first_syn_future(&mut st.common);
                transition!(SendFirstSyn {
                    common: st.common,
                    send: send_future,
                    next_action: Some(st.recv_stream)
                })
            }
            Ok(Async::Ready(None)) => bail!(ErrorKind::TimedOut)
        };

        let state = state.take();
        let mut common = state.common;
        common.next_seqno += Wrapping(1);
        debug_assert!(dst == common.dst);

        let src = *common.src.ip();

        let data = data_ref.lock();
        let packet = parse_stream_server_packet(&data);

        if packet.seqno_start != packet.seqno_end
                || packet.seqno_start != (common.next_seqno - Wrapping(1)).0 {
            return Ok(Async::NotReady);
        }

        if packet.flags != (StreamPacketFlags::Syn | StreamPacketFlags::Ack) {
            return Ok(Async::NotReady);
        }

        // TODO: output the server message

        let send_buf_ref = common.send_buf
            .range(0 .. STREAM_CLIENT_FULL_HEADER_SIZE as usize);
        let mut send_buf = send_buf_ref.borrow_mut();

        make_stream_client_icmpv6_packet(
            &mut send_buf,
            src,
            *dst.ip(),
            common.next_seqno.0,
            StreamPacketFlags::Ack.into(),
            &[]
        );
        let send_ack_future = common.sock.sendto(
            common.send_buf
                .range(0 .. STREAM_CLIENT_FULL_HEADER_SIZE as usize),
            dst,
            SendFlagSet::new()
        );
        common.next_seqno += Wrapping(1);

        transition!(SendAck {
            common: common,
            send_ack: send_ack_future
        })
    }

    fn poll_send_ack<'a>(
        state: &'a mut RentToOwn<'a, SendAck<'s>>
    ) -> Poll<AfterSendAck<'s>, Error> {
        let size = try_ready!(state.send_ack.poll());
        debug_assert!(size == STREAM_CLIENT_FULL_HEADER_SIZE as usize);
        transition!(SendData {
            common: state.take().common,
            tmp_buf: RefCell::new(vec![0; TMP_BUFFER_SIZE]),
            next_data: Cell::new(None)
        })
    }

    fn poll_send_data<'a>(
        state: &'a mut RentToOwn<'a, SendData<'s>>
    ) -> Poll<AfterSendData<'s>, Error> {
        let common = &state.common;

        let mut data_source = common.data_source.clone();

        let tmp_buf_ref = state.tmp_buf.clone();
        let mut tmp_buf = tmp_buf_ref.borrow_mut();

        let mut read_buf = common.read_buf.clone();

        let mut activity = true;
        while activity {
            activity = false;

            let buffer_space = {
                let sp = read_buf.get_space_left();
                if sp < common.mtu as usize {
                    read_buf.cleanup();
                    read_buf.get_space_left()
                } else {
                    sp
                }
            };
            let to_read = ::std::cmp::min(buffer_space, tmp_buf.len());

            if to_read != 0 {
                if let Async::Ready(size) =
                        data_source.poll_read(&mut tmp_buf[0 .. to_read])? {
                    read_buf.add(&tmp_buf[0..size]);
                    activity = true;
                }
            }

            unimplemented!()
        }

        unimplemented!()
    }

    fn poll_wait_for_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForAck<'s>>
    ) -> Poll<AfterWaitForAck<'s>, Error> {
        unimplemented!()
    }

    fn poll_received_server_fin<'a>(
        state: &'a mut RentToOwn<'a, ReceivedServerFin<'s>>
    ) -> Poll<AfterReceivedServerFin<'s>, Error> {
        unimplemented!()
    }

    fn poll_send_fin_ack<'a>(
        state: &'a mut RentToOwn<'a, SendFinAck<'s>>
    ) -> Poll<AfterSendFinAck<'s>, Error> {
        unimplemented!()
    }

    fn poll_wait_for_last_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForLastAck<'s>>
    ) -> Poll<AfterWaitForLastAck, Error> {
        unimplemented!()
    }

    fn poll_send_fin<'a>(
        state: &'a mut RentToOwn<'a, SendFin<'s>>
    ) -> Poll<AfterSendFin<'s>, Error> {
        unimplemented!()
    }

    fn poll_wait_for_fin_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForFinAck<'s>>
    ) -> Poll<AfterWaitForFinAck<'s>, Error> {
        unimplemented!()
    }

    fn poll_send_last_ack<'a>(
        state: &'a mut RentToOwn<'a, SendLastAck<'s>>
    ) -> Poll<AfterSendLastAck, Error> {
        unimplemented!()
    }
}

fn make_first_syn_future<'a>(common: &mut StreamCommonState<'a>)
        -> futures::IpV6RawSocketSendtoFuture {
    let dst = common.dst;
    let send_buf_ref = common.send_buf
        .range(0 .. STREAM_CLIENT_FULL_HEADER_SIZE as usize);

    make_stream_client_icmpv6_packet(
        &mut send_buf_ref.borrow_mut(),
        *common.src.ip(),
        *dst.ip(),
        common.next_seqno.0,
        StreamPacketFlags::Syn.into(),
        &[]
    );

    common.sock.sendto(
        send_buf_ref,
        dst,
        SendFlagSet::new()
    )
}

fn make_recv_packets_stream<'a>(common: &mut StreamCommonState<'a>)
        -> Box<StreamE<(futures::U8Slice, SocketAddrV6)>> {
    let cdst = common.dst;

    Box::new(unfold((
            common.sock.clone(),
            common.recv_buf.range(0 .. common.mtu as usize),
            common.mtu
        ),
        move |(mut sock, recv_buf, mtu)| {
            Some(sock.recvfrom(recv_buf.clone(), RecvFlagSet::new())
                .map_err(|e| e.into())
                .map(move |x| (x, (sock, recv_buf, mtu)))
            )
        }
    ).filter(move |&(ref x, src)| {
        src == cdst
            && validate_stream_packet(
                &x.lock(),
                Some((*cdst.ip(), *src.ip()))
            )
    }))
}

pub struct StreamCommonState<'a> {
    pub config: &'a Config,
    pub src: SocketAddrV6,
    pub dst: SocketAddrV6,
    pub sock: futures::IpV6RawSocketAdapter,
    pub mtu: u16,
    pub data_source: StdinBytesReader<'a>,
    pub send_buf: SArcRef<Vec<u8>>,
    pub recv_buf: SArcRef<Vec<u8>>,
    pub next_seqno: Wrapping<u16>,
    pub read_buf: TrimmingBuffer,
    pub ack_wait: AckWaitlist
}
