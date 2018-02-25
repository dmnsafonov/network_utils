use ::std::cell::RefCell;
use ::std::net::*;
use ::std::num::Wrapping;
use ::std::time::Duration;

use ::futures::prelude::*;
use ::futures::stream::unfold;
use ::pnet_packet::Packet;
use ::state_machine_future::RentToOwn;
use ::tokio_timer::*;

use ::linux_network::*;
use ::ping6_datacommon::*;
use ::sliceable_rcref::SRcRef;

use ::config::Config;
use ::errors::{Error, ErrorKind};
use ::stdout_iterator::StdoutBytesWriter;
use ::stream::packet::*;

type FutureE<T> = ::futures::Future<Item = T, Error = Error>;
type StreamE<T> = ::futures::stream::Stream<Item = T, Error = Error>;

#[derive(StateMachineFuture)]
pub enum StreamMachine<'s> {
    #[state_machine_future(start, transitions(WaitForFirstSyn))]
    InitState {
        common: StreamCommonState<'s>
    },

    #[state_machine_future(transitions(SendSynAck))]
    WaitForFirstSyn {
        common: StreamCommonState<'s>,
        recv_first_syn: Box<FutureE<(futures::U8Slice, SocketAddrV6)>>
    },

    #[state_machine_future(transitions(WaitForAck))]
    SendSynAck {
        common: StreamCommonState<'s>,
        dst: SocketAddrV6,
        next_seqno: Wrapping<u16>,
        send_syn_ack: futures::IpV6RawSocketSendtoFuture,
        next_action: Option<Box<StreamE<
            TimedResult<(futures::U8Slice, SocketAddrV6)>
        >>>
    },

    #[state_machine_future(transitions(SendSynAck, WaitForPackets))]
    WaitForAck {
        common: StreamCommonState<'s>,
        dst: SocketAddrV6,
        next_seqno: Wrapping<u16>,
        recv_stream: Box<StreamE<
            TimedResult<(futures::U8Slice, SocketAddrV6)>
        >>
    },

    #[state_machine_future(transitions(SendFinAck, SendFin))]
    WaitForPackets {
        common: StreamCommonState<'s>,
        dst: SocketAddrV6
    },

    #[state_machine_future(transitions(WaitForLastAck))]
    SendFinAck {
        common: StreamCommonState<'s>,
        dst: SocketAddrV6
    },

    #[state_machine_future(transitions(SendFinAck, ConnectionTerminated))]
    WaitForLastAck {
        common: StreamCommonState<'s>,
        dst: SocketAddrV6
    },

    #[state_machine_future(transitions(WaitForFinAck))]
    SendFin {
        common: StreamCommonState<'s>,
        dst: SocketAddrV6
    },

    #[state_machine_future(transitions(SendFin, SendLastAck))]
    WaitForFinAck {
        common: StreamCommonState<'s>,
        dst: SocketAddrV6
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    SendLastAck {
        common: StreamCommonState<'s>,
        dst: SocketAddrV6
    },

    #[state_machine_future(ready)]
    ConnectionTerminated(TerminationReason),

    #[state_machine_future(error)]
    ErrorState(Error)
}

pub enum TerminationReason {
    DataReceived,
    Interrupted
}

impl<'s> PollStreamMachine<'s> for StreamMachine<'s> {
    fn poll_init_state<'a>(
        state: &'a mut RentToOwn<'a, InitState<'s>>
    ) -> Poll<AfterInitState<'s>, Error> {
        let mut common = state.take().common;

        let recv_future = make_recv_packets_stream(&mut common)
            .filter(|&(ref x, src)| {
                let data_ref = x.borrow();
                let packet = parse_stream_client_packet(&data_ref);

                packet.flags == StreamPacketFlags::Syn.into()
            })
            .into_future()
            .map(|(x,_)| x.unwrap())
            .map_err(|(e,_)| e);

        transition!(WaitForFirstSyn {
            common: common,
            recv_first_syn: Box::new(recv_future)
        })
    }

    fn poll_wait_for_first_syn<'a>(
        state: &'a mut RentToOwn<'a, WaitForFirstSyn<'s>>
    ) -> Poll<AfterWaitForFirstSyn<'s>, Error> {
        let (data_ref, dst) = try_ready!(state.recv_first_syn.poll());

        let mut common = state.take().common;

        let data = data_ref.borrow();
        let packet = parse_stream_client_packet(&data);

        let send_future = make_syn_ack_future(
            &mut common,
            dst,
            packet.seqno,
            packet.seqno
        );

        transition!(SendSynAck {
            common: common,
            dst: dst,
            next_seqno: Wrapping(packet.seqno) + Wrapping(1),
            send_syn_ack: send_future,
            next_action: None
        })
    }

    fn poll_send_syn_ack<'a>(
        state: &'a mut RentToOwn<'a, SendSynAck<'s>>
    ) -> Poll<AfterSendSynAck<'s>, Error> {
        let size = try_ready!(state.send_syn_ack.poll());
        debug_assert!(size == STREAM_SERVER_FULL_HEADER_SIZE as usize);

        let SendSynAck { mut common, dst, next_seqno, next_action, .. }
            = state.take();

        let timed_packets = next_action.unwrap_or_else(|| {
            let seqno = next_seqno;
            let packets = make_recv_packets_stream(&mut common)
                .filter(move |&(ref x, _)| {
                    let data_ref = x.borrow();
                    let packet = parse_stream_client_packet(&data_ref);

                    packet.flags == StreamPacketFlags::Ack.into()
                        && packet.seqno == seqno.0
                });
            let timed = TimeoutResultStream::new(
                &common.timer,
                packets,
                Duration::from_millis(PACKET_LOSS_TIMEOUT as u64)
            );
            Box::new(timed.take(RETRANSMISSIONS_NUMBER as u64))
        });

        transition!(WaitForAck {
            common: common,
            dst: dst,
            next_seqno: next_seqno,
            recv_stream: timed_packets
        })
    }

    fn poll_wait_for_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForAck<'s>>
    ) -> Poll<AfterWaitForAck<'s>, Error> {
        let (data_ref, dst) = match state.recv_stream.poll() {
            Err(e) => bail!(e),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(Some(TimedResult::InTime(x)))) => x,
            Ok(Async::Ready(Some(TimedResult::TimedOut))) => {
                let mut st = state.take();
                let seqno = st.next_seqno - Wrapping(1);
                let send_future = make_syn_ack_future(
                    &mut st.common,
                    st.dst,
                    seqno.0,
                    seqno.0
                );
                transition!(SendSynAck {
                    common: st.common,
                    dst: st.dst,
                    next_seqno: st.next_seqno,
                    send_syn_ack: send_future,
                    next_action: Some(st.recv_stream)
                })
            }
            Ok(Async::Ready(None)) => bail!(ErrorKind::TimedOut)
        };

        unimplemented!()
    }

    fn poll_wait_for_packets<'a>(
        state: &'a mut RentToOwn<'a, WaitForPackets<'s>>
    ) -> Poll<AfterWaitForPackets<'s>, Error> {
        unimplemented!()
    }

    fn poll_send_fin_ack<'a>(
        state: &'a mut RentToOwn<'a, SendFinAck<'s>>
    ) -> Poll<AfterSendFinAck<'s>, Error> {
        unimplemented!()
    }

    fn poll_wait_for_last_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForLastAck<'s>>
    ) -> Poll<AfterWaitForLastAck<'s>, Error> {
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

fn make_recv_packets_stream<'a>(
    common: &mut StreamCommonState<'a>
) -> Box<StreamE<(futures::U8Slice, SocketAddrV6)>> {
    let csrc = common.src;

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
        validate_stream_packet(
            &x.borrow(),
            Some((*src.ip(), *csrc.ip()))
        )
    }))
}

fn make_syn_ack_future<'a>(
    common: &mut StreamCommonState<'a>,
    dst: SocketAddrV6,
    seqno_start: u16,
    seqno_end: u16
) -> futures::IpV6RawSocketSendtoFuture {
    let send_buf_ref = common.send_buf
        .range(0 .. STREAM_SERVER_FULL_HEADER_SIZE as usize);

    make_stream_server_icmpv6_packet(
        &mut send_buf_ref.borrow_mut(),
        *common.src.ip(),
        *dst.ip(),
        seqno_start,
        seqno_end,
        StreamPacketFlags::Syn.into(),
        &[]
    );

    common.sock.sendto(
        send_buf_ref,
        dst,
        SendFlagSet::new()
    )
}

pub struct StreamCommonState<'a> {
    pub config: &'a Config,
    pub src: SocketAddrV6,
    pub sock: futures::IpV6RawSocketAdapter,
    pub mtu: u16,
    pub data_out: StdoutBytesWriter<'a>,
    pub timer: Timer,
    pub send_buf: SRcRef<Vec<u8>>,
    pub recv_buf: SRcRef<Vec<u8>>
}
