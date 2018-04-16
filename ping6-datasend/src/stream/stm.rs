use ::std::cell::*;
use ::std::collections::VecDeque;
use ::std::net::SocketAddrV6;
use ::std::num::Wrapping;
use ::std::ops::*;
use ::std::time::*;

use ::futures::prelude::*;
use ::futures::Stream;
use ::futures::stream::*;
use ::state_machine_future::RentToOwn;
use ::tokio::prelude::*;
use ::tokio::timer::Delay;

use ::linux_network::*;
use ::linux_network::futures::U8Slice;
use ::ping6_datacommon::*;
use ::sliceable_rcref::SArcRef;

use ::config::*;
use ::errors::{Error, ErrorKind, Result};
use ::send_box::SendBox;
use ::stdin_iterator::StdinBytesReader;
use ::stream::buffers::*;
use ::stream::packet::*;

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
            TimedResult<(U8Slice, SocketAddrV6)>
        >>>
    },

    #[state_machine_future(transitions(SendFirstSyn, SendAck))]
    WaitForSynAck {
        common: StreamCommonState<'s>,
        recv_stream: SendBox<StreamE<
            TimedResult<(U8Slice, SocketAddrV6)>
        >>
    },

    #[state_machine_future(transitions(SendData))]
    SendAck {
        common: StreamCommonState<'s>,
        send_ack: futures::IpV6RawSocketSendtoFuture
    },

    #[state_machine_future(transitions(SendFin, SendFinAck))]
    SendData {
        common: StreamCommonState<'s>,
        read_buf: TrimmingBuffer,
        tmp_buf: RefCell<Vec<u8>>,
        next_data: RefCell<Option<NextData>>,
        retransmit_queue: RefCell<VecDeque<(Vec<u8>, u16)>>,
        send_fut: RefCell<Option<futures::IpV6RawSocketSendtoFuture>>,
        recv_stream: RefCell<SendBox<
            StreamE<(U8Slice, SocketAddrV6)>
        >>,
        ack_wait: AckWaitlist,
        ack_timer: Delay,
        reached_input_eof: bool
    },

    #[state_machine_future(transitions(WaitForLastAck))]
    SendFinAck {
        common: StreamCommonState<'s>
    },

    #[state_machine_future(transitions(ConnectionTerminated, SendFinAck))]
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

pub enum NextData {
    Input(TrimmingBufferSlice),
    Retransmission(Vec<u8>, u16)
}

impl NextData {
    fn from_tb_slice(slice: TrimmingBufferSlice) -> NextData {
        NextData::Input(slice)
    }

    fn from_retransmission(payload: Vec<u8>, seqno: u16) -> NextData {
        NextData::Retransmission(payload, seqno)
    }
}

impl Deref for NextData {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        match self {
            NextData::Input(x) => &x,
            NextData::Retransmission(x, _) => &x
        }
    }
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
        debug!("sending first SYN packet");
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
        debug!("waiting for SYN+ACK");
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
        debug!("sending first ACK");
        let size = try_ready!(state.send_ack.poll());
        debug_assert!(size == STREAM_CLIENT_FULL_HEADER_SIZE as usize);

        let (window_size, read_buffer_size) =
            if let ModeConfig::Stream(ref sc) = state.common.config.mode {
                (sc.window_size, sc.read_buffer_size)
            } else {
                unreachable!()
            };

        let recv_stream = RefCell::new(unsafe {
            SendBox::new(make_recv_ack_or_fin(&mut state.common))
        });
        let mtu = state.common.mtu;
        transition!(SendData {
            common: state.take().common,
            read_buf: TrimmingBuffer::new(read_buffer_size),
            tmp_buf: RefCell::new(vec![0; TMP_BUFFER_SIZE]),
            next_data: RefCell::new(None),
            retransmit_queue: RefCell::new(VecDeque::new()),
            send_fut: RefCell::new(None),
            recv_stream,
            ack_wait: AckWaitlist::new(window_size, mtu),
            ack_timer: make_packet_loss_delay(),
            reached_input_eof: false
        })
    }

    fn poll_send_data<'a>(
        state: &'a mut RentToOwn<'a, SendData<'s>>
    ) -> Poll<AfterSendData<'s>, Error> {
        debug!("sending data");

        let mut activity = true;
        while activity {
            activity = {
                let ret = fill_read_buf(&mut *state)?;
                ret
            };
            activity = activity || poll_send_fut(state.send_fut.borrow_mut())?;
            fill_next_data(&mut *state);
            if state.next_data.borrow().is_some() {
                make_data_send_fut(&mut *state);
            }
            activity = activity || poll_receive_packets(&mut *state)?;
            activity = activity || poll_timeout(&mut *state)?;

            // TODO: server FIN
        }

        if state.reached_input_eof && state.next_data.borrow().is_none() {
            state.ack_wait.cleanup();
            if state.ack_wait.is_empty() {
                let common = state.take().common;
                transition!(SendFin {
                    common,
                    // TODO
                });
            }
        }

        return Ok(Async::NotReady);
    }

    fn poll_send_fin_ack<'a>(
        state: &'a mut RentToOwn<'a, SendFinAck<'s>>
    ) -> Poll<AfterSendFinAck<'s>, Error> {
        debug!("sending FIN+ACK");
        unimplemented!()
    }

    fn poll_wait_for_last_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForLastAck<'s>>
    ) -> Poll<AfterWaitForLastAck<'s>, Error> {
        debug!("waiting for last ACK");
        unimplemented!()
    }

    fn poll_send_fin<'a>(
        state: &'a mut RentToOwn<'a, SendFin<'s>>
    ) -> Poll<AfterSendFin<'s>, Error> {
        debug!("sending FIN");
        unimplemented!()
    }

    fn poll_wait_for_fin_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForFinAck<'s>>
    ) -> Poll<AfterWaitForFinAck<'s>, Error> {
        debug!("waiting for FIN+ACK");
        unimplemented!()
    }

    fn poll_send_last_ack<'a>(
        state: &'a mut RentToOwn<'a, SendLastAck<'s>>
    ) -> Poll<AfterSendLastAck, Error> {
        debug!("sending last ACK");
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
        -> Box<StreamE<(U8Slice, SocketAddrV6)>> {
    let csrc = common.src;
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
                Some((*src.ip(), *csrc.ip()))
            )
    }))
}

fn make_packet_loss_delay() -> Delay {
    Delay::new(Instant::now()
        + Duration::from_millis(PACKET_LOSS_TIMEOUT as u64))
}

fn make_recv_ack_or_fin<'a>(common: &mut StreamCommonState<'a>)
        -> Box<StreamE<(U8Slice, SocketAddrV6)>> {
    Box::new(make_recv_packets_stream(common)
        .filter(|&(ref x, _)| {
            let packet_buff = x.lock();
            let packet = parse_stream_server_packet(&packet_buff);
            (packet.flags.test(StreamPacketFlags::Ack)
                    || packet.flags.test(StreamPacketFlags::Fin))
                && !packet.flags.test(StreamPacketFlags::Syn)
        }))
}

fn fill_read_buf(
    state: &mut SendData,
) -> Result<bool> {
    if state.reached_input_eof {
        return Ok(false);
    }

    let mut tmp_buf = state.tmp_buf.borrow_mut();

    let buffer_space = {
        let sp = state.read_buf.get_space_left();
        if sp < state.common.mtu as usize {
            state.read_buf.cleanup();
            state.read_buf.get_space_left()
        } else {
            sp
        }
    };
    let to_read = ::std::cmp::min(
        buffer_space,
        tmp_buf.len()
    );

    if to_read != 0 {
        if let Async::Ready(size) =
                state.common.data_source.poll_read(&mut tmp_buf[0 .. to_read])? {
            if size == 0 {
                debug!("reach EOF on the input stream");
                state.reached_input_eof = true;
                return Ok(true);
            }

            state.read_buf.add(&tmp_buf[0..size]);
            return Ok(true);
        }
    }

    Ok(false)
}

fn poll_send_fut(
    mut send_fut_opt: RefMut<Option<futures::IpV6RawSocketSendtoFuture>>
) -> Result<bool> {
    if let ref mut send_fut@Some(_) = *send_fut_opt {
        if let Async::Ready(_)
                = send_fut.as_mut().unwrap().poll()? {
            send_fut.take();
            return Ok(true);
        }
    }
    return Ok(false)
}

fn make_data_send_fut<'s>(
    state: &mut SendData<'s>,
) {
    let data = state.next_data.replace(None).unwrap();

    let seqno = match data {
        NextData::Input(_) => state.common.next_seqno.0,
        NextData::Retransmission(_, s) => s
    };

    let size = STREAM_CLIENT_FULL_HEADER_SIZE as usize + data.len();
    let send_buf_ref = state.common.send_buf
        .range(0 .. size);
    let mut send_buf = send_buf_ref.borrow_mut();
    make_stream_client_icmpv6_packet(
        &mut send_buf,
        *state.common.src.ip(),
        *state.common.dst.ip(),
        seqno,
        StreamPacketFlagSet::new(),
        &data
    );

    let buf_to_send = state.common.send_buf.range(0 .. size);
    let dst = state.common.dst;
    let fut = state.common.sock.sendto(
        buf_to_send,
        dst,
        SendFlagSet::new()
    );
    state.send_fut.replace(Some(fut));

    if let NextData::Input(slice) = data {
        let next_seqno = state.common.next_seqno;

        if state.ack_wait.is_full() {
            state.ack_wait.cleanup();
        }
        state.ack_wait.add(AckWait::new(next_seqno, slice));

        state.common.next_seqno += Wrapping(1);
    }
}

fn fill_next_data(state: &mut SendData) {
    if state.next_data.borrow().is_none() {
        let mut retransmit_queue = state.retransmit_queue.borrow_mut();
        if retransmit_queue.is_empty() {
            if let Some(slice) =
                    state.read_buf.take((state.common.mtu
                        - STREAM_CLIENT_FULL_HEADER_SIZE) as usize) {
                state.next_data.replace(Some(
                    NextData::from_tb_slice(slice)
                ));
            }
        } else {
            let (payload, seqno) =
                retransmit_queue.pop_front().unwrap();
            state.next_data.replace(Some(
                NextData::from_retransmission(payload, seqno)
            ));
        }
    }
}

fn poll_receive_packets(state: &mut SendData) -> Result<bool> {
    let recv_async = state.recv_stream.borrow_mut().poll()?;
    if let Async::Ready(Some((x, _))) = recv_async {
        let packet_buff = x.lock();
        let packet = parse_stream_server_packet(&packet_buff);
        state.ack_wait.remove(
            IRange(
                Wrapping(packet.seqno_start),
                Wrapping(packet.seqno_end)
            )
        );

        state.ack_timer = make_packet_loss_delay();

        return Ok(true);
    }

    Ok(false)
}

fn poll_timeout(state: &mut SendData) -> Result<bool> {
    if let Async::Ready(_) = state.ack_timer.poll()? {
        {
            let mut retransmit_queue =
                state.retransmit_queue.borrow_mut();
            if !retransmit_queue.is_empty() {
                bail!(ErrorKind::TimedOut);
            }
            for i in state.ack_wait.iter() {
                retransmit_queue.push_back((
                    (*i.data).into(),
                    i.seqno.0
                ));
            }
        }
        state.ack_timer = make_packet_loss_delay();

        return Ok(true);
    }

    Ok(false)
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
}
