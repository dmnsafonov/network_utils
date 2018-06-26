use ::std::cell::*;
use ::std::collections::VecDeque;
use ::std::net::SocketAddrV6;
use ::std::num::Wrapping;
use ::std::ops::*;
use ::std::time::*;

use ::bytes::*;
use ::futures::prelude::*;
use ::futures::Stream;
use ::futures::stream::*;
use ::state_machine_future::RentToOwn;
use ::tokio::prelude::*;
use ::tokio::timer::Delay;

use ::linux_network::*;
use ::ping6_datacommon::*;
use ::send_box::SendBox;

use ::config::*;
use ::errors::{Error, Result};
use ::stdin_iterator::StdinBytesReader;
use ::stream::buffers::*;
use ::stream::packet::*;

type StreamE<T> = dyn(::futures::stream::Stream<
    Item = T,
    Error = ::failure::Error
>);

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
        send: futures::IPv6RawSocketSendtoFuture,
        next_action: Option<SendBox<StreamE<
            TimedResult<(Bytes, SocketAddrV6)>
        >>>
    },

    #[state_machine_future(transitions(SendFirstSyn, SendAck))]
    WaitForSynAck {
        common: StreamCommonState<'s>,
        recv_stream: SendBox<StreamE<
            TimedResult<(Bytes, SocketAddrV6)>
        >>
    },

    #[state_machine_future(transitions(SendData))]
    SendAck {
        common: StreamCommonState<'s>,
        send_ack: futures::IPv6RawSocketSendtoFuture
    },

    #[state_machine_future(transitions(SendFin, SendFinAck))]
    SendData {
        common: StreamCommonState<'s>,
        read_buf: TrimmingBuffer,
        tmp_buf: RefCell<Vec<u8>>,
        next_data: RefCell<Option<NextData>>,
        retransmit_queue: RefCell<VecDeque<(Vec<u8>, u16)>>,
        send_fut: RefCell<Option<futures::IPv6RawSocketSendtoFuture>>,
        recv_stream: RefCell<SendBox<
            StreamE<(Bytes, SocketAddrV6)>
        >>,
        ack_wait: AckWaitlist,
        ack_timer: Delay,
        connection_timeout: Delay,
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
        common: StreamCommonState<'s>,
        send_fut: futures::IPv6RawSocketSendtoFuture,
        next_action: Option<SendBox<StreamE<
            TimedResult<(Bytes, SocketAddrV6)>
        >>>
    },

    #[state_machine_future(transitions(SendFin, SendLastAck))]
    WaitForFinAck {
        common: StreamCommonState<'s>,
        recv_stream: SendBox<StreamE<
            TimedResult<(Bytes, SocketAddrV6)>
        >>
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    SendLastAck {
        send_fut: futures::IPv6RawSocketSendtoFuture
    },

    #[state_machine_future(ready)]
    ConnectionTerminated(TerminationReason),

    #[state_machine_future(error)]
    ErrorState(::failure::Error)
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
    ) -> Poll<AfterInitState<'s>, ::failure::Error> {
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
    ) -> Poll<AfterSendFirstSyn<'s>, ::failure::Error> {
        debug!("sending first SYN packet");
        let size = try_ready!(state.send.poll());
        debug_assert_eq!(size, STREAM_CLIENT_FULL_HEADER_SIZE);

        let state = state.take();
        let mut common = state.common;

        let timed_packets = state.next_action.unwrap_or_else(|| {
            let seqno = common.next_seqno;
            let packets = make_recv_packets_stream(&mut common)
                .filter(move |&(ref data, _)| {
                    let packet = parse_stream_server_packet(&data);

                    packet.flags.contains(StreamPacketFlags::Syn)
                            && packet.flags.contains(StreamPacketFlags::Ack)
                            && !packet.flags.contains(StreamPacketFlags::Fin)
                        && packet.seqno_start == packet.seqno_end
                        && packet.seqno_start == seqno.0
                });
            let timed = TimeoutResultStream::new(
                packets,
                Duration::from_millis(PACKET_LOSS_TIMEOUT)
            );
            unsafe {
                SendBox::new(Box::new(
                    timed.take(RETRANSMISSIONS_NUMBER)
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
    ) -> Poll<AfterWaitForSynAck<'s>, ::failure::Error> {
        debug!("waiting for SYN+ACK");
        let (data, dst) = match state.recv_stream.poll() {
            Err(e) => return Err(e),
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
            Ok(Async::Ready(None)) => return Err(Error::TimedOut.into())
        };

        let state = state.take();
        let mut common = state.common;
        common.next_seqno += Wrapping(1);
        debug_assert_eq!(dst, common.dst);

        let packet = parse_stream_server_packet(&data);

        if packet.seqno_start != packet.seqno_end
                || packet.seqno_start != (common.next_seqno - Wrapping(1)).0 {
            return Ok(Async::NotReady);
        }

        if packet.flags != (StreamPacketFlags::Syn | StreamPacketFlags::Ack) {
            return Ok(Async::NotReady);
        }

        // TODO: output the server message

        let fut = make_send_fut(
            &mut common,
            StreamPacketFlags::Ack,
            &[],
            None
        );
        common.next_seqno += Wrapping(1);

        transition!(SendAck {
            common: common,
            send_ack: fut
        })
    }

    fn poll_send_ack<'a>(
        state: &'a mut RentToOwn<'a, SendAck<'s>>
    ) -> Poll<AfterSendAck<'s>, ::failure::Error> {
        debug!("sending first ACK");
        let size = try_ready!(state.send_ack.poll());
        debug_assert_eq!(size, STREAM_CLIENT_FULL_HEADER_SIZE);

        let sc = get_stream_config(&state.common.config);

        let recv_stream_box: Box<StreamE<(Bytes, SocketAddrV6)>>
            = Box::new(make_recv_ack_or_fin(&mut state.common));
        let recv_stream = RefCell::new(unsafe {
            SendBox::new(recv_stream_box)
        });
        let mtu = state.common.mtu;
        transition!(SendData {
            common: state.take().common,
            read_buf: TrimmingBuffer::new(sc.read_buffer_size),
            tmp_buf: RefCell::new(vec![0; TMP_BUFFER_SIZE]),
            next_data: RefCell::new(None),
            retransmit_queue: RefCell::new(VecDeque::new()),
            send_fut: RefCell::new(None),
            recv_stream,
            ack_wait: AckWaitlist::new(sc.window_size, mtu),
            ack_timer: Delay::new(make_packet_loss_instant()),
            connection_timeout: Delay::new(make_connection_timeout_instant()),
            reached_input_eof: false
        })
    }

    fn poll_send_data<'a>(
        state: &'a mut RentToOwn<'a, SendData<'s>>
    ) -> Poll<AfterSendData<'s>, ::failure::Error> {
        debug!("sending data");

        let mut activity = true;
        while activity {
            activity = {
                let ret = fill_read_buf(&mut *state)?;
                ret
            };
            fill_next_data(&mut *state);
            if state.next_data.borrow().is_some() {
                make_data_send_fut(&mut *state);
            }
            activity = poll_send_fut(state.send_fut.borrow_mut())? || activity;
            activity = poll_receive_packets(&mut *state)? || activity;
            activity = poll_timeout(&mut *state)? || activity;

            // TODO: server FIN
        }

        if state.reached_input_eof && state.next_data.borrow().is_none() {
            state.ack_wait.cleanup();
            if state.ack_wait.is_empty() {
                let mut common = state.take().common;
                let send_fut = make_send_fut(
                    &mut common,
                    StreamPacketFlags::Fin,
                    &[],
                    None
                );
                transition!(SendFin {
                    common,
                    send_fut: send_fut,
                    next_action: None
                });
            }
        }

        return Ok(Async::NotReady);
    }

    fn poll_send_fin_ack<'a>(
        state: &'a mut RentToOwn<'a, SendFinAck<'s>>
    ) -> Poll<AfterSendFinAck<'s>, ::failure::Error> {
        debug!("sending FIN+ACK");
        unimplemented!()
    }

    fn poll_wait_for_last_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForLastAck<'s>>
    ) -> Poll<AfterWaitForLastAck<'s>, ::failure::Error> {
        debug!("waiting for last ACK");
        unimplemented!()
    }

    fn poll_send_fin<'a>(
        state: &'a mut RentToOwn<'a, SendFin<'s>>
    ) -> Poll<AfterSendFin<'s>, ::failure::Error> {
        debug!("sending FIN");
        let size = try_ready!(state.send_fut.poll());
        debug_assert_eq!(size, STREAM_CLIENT_FULL_HEADER_SIZE);

        let state = state.take();
        let mut common = state.common;

        let cdst = common.dst;
        let timed_packets = state.next_action.unwrap_or_else(|| {
            let seqno = common.next_seqno;
            let packets = make_recv_packets_stream(&mut common)
                .filter(move |&(ref data, dst)| {
                    let packet = parse_stream_server_packet(&data);

                    !packet.flags.contains(StreamPacketFlags::Syn)
                            && packet.flags.contains(StreamPacketFlags::Ack)
                            && packet.flags.contains(StreamPacketFlags::Fin)
                        && packet.seqno_start == packet.seqno_end
                            && packet.seqno_start == seqno.0
                        && dst == cdst

                });
            let timed = TimeoutResultStream::new(
                packets,
                Duration::from_millis(PACKET_LOSS_TIMEOUT)
            );
            unsafe {
                SendBox::new(Box::new(
                    timed.take(RETRANSMISSIONS_NUMBER)
                ))
            }
        });

        transition!(WaitForFinAck {
            common,
            recv_stream: timed_packets
        })
    }

    fn poll_wait_for_fin_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForFinAck<'s>>
    ) -> Poll<AfterWaitForFinAck<'s>, ::failure::Error> {
        debug!("waiting for FIN+ACK");
        match state.recv_stream.poll() {
            Err(e) => return Err(e),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(Some(TimedResult::InTime(x)))) => x,
            Ok(Async::Ready(Some(TimedResult::TimedOut))) => {
                let mut st = state.take();
                let send_future =
                    make_first_syn_future(&mut st.common);
                transition!(SendFin {
                    common: st.common,
                    send_fut: send_future,
                    next_action: Some(st.recv_stream)
                })
            }
            Ok(Async::Ready(None)) => return Err(Error::TimedOut.into())
        };

        let mut common = state.take().common;

        let send_fut = make_send_fut(
            &mut common,
            StreamPacketFlags::Ack,
            &[],
            None
        );
        common.next_seqno += Wrapping(1);

        transition!(SendLastAck {
            send_fut
        })
    }

    fn poll_send_last_ack<'a>(
        state: &'a mut RentToOwn<'a, SendLastAck>
    ) -> Poll<AfterSendLastAck, ::failure::Error> {
        debug!("sending last ACK");
        let size = try_ready!(state.send_fut.poll());
        debug_assert_eq!(size, STREAM_CLIENT_FULL_HEADER_SIZE);

        transition!(ConnectionTerminated(TerminationReason::DataSent))
    }
}

fn get_stream_config(config: &Config) -> &StreamConfig {
    match config.mode {
        ModeConfig::Stream(ref sc) => sc,
        _ => unreachable!()
    }
}

fn make_send_fut<'a>(
    common: &mut StreamCommonState<'a>,
    flags: StreamPacketFlags,
    payload: &[u8],
    override_seqno: Option<u16>
) -> futures::IPv6RawSocketSendtoFuture {
    let dst = common.dst;
    let seqno = override_seqno.unwrap_or(common.next_seqno.0);

    let packet = make_stream_client_icmpv6_packet(
        &mut common.send_buf,
        *common.src.ip(),
        *dst.ip(),
        seqno,
        flags,
        payload
    );

    debug!("send packet with seqno {}", seqno);
    common.sock.sendto(
        packet,
        dst,
        SendFlags::empty()
    )
}

fn make_first_syn_future<'a>(common: &mut StreamCommonState<'a>)
        -> futures::IPv6RawSocketSendtoFuture {
    make_send_fut(common, StreamPacketFlags::Syn, &[], None)
}

fn make_recv_packets_stream<'a>(common: &mut StreamCommonState<'a>)
        -> impl Stream<
            Item = (Bytes, SocketAddrV6),
            Error = ::failure::Error
        > {
    let csrc = common.src;
    let cdst = common.dst;
    let mtu = common.mtu as usize;

    unfold((
            common.sock.clone(),
            common.recv_buf.split_to(mtu),
            mtu
        ),
        move |(mut sock, mut recv_buf, mtu)| {
            let len = recv_buf.len();
            if len < mtu {
                recv_buf.reserve(mtu - len);
                unsafe { recv_buf.advance_mut(mtu - len); }
            }
            Some(sock.recvfrom(recv_buf.clone(), RecvFlags::empty())
                .map_err(|e| e.into())
                .map(move |x| (x, (sock, recv_buf, mtu)))
            )
        }
    ).filter(move |&(ref x, src)| {
        src == cdst
            && validate_stream_packet(
                &x,
                Some((*src.ip(), *csrc.ip()))
            )
    })
}

fn make_packet_loss_instant() -> Instant {
    Instant::now() + Duration::from_millis(PACKET_LOSS_TIMEOUT)
}

fn make_connection_timeout_instant() -> Instant {
    Instant::now() + Duration::from_millis(CONNECTION_LOSS_TIMEOUT)
}

fn make_recv_ack_or_fin<'a>(common: &mut StreamCommonState<'a>)
        -> impl Stream<
            Item = (Bytes, SocketAddrV6),
            Error = ::failure::Error
        > {
    make_recv_packets_stream(common)
        .filter(|&(ref packet_buff, _)| {
            let packet = parse_stream_server_packet(&packet_buff);
            (packet.flags.contains(StreamPacketFlags::Ack)
                    || packet.flags.contains(StreamPacketFlags::Fin))
                && !packet.flags.contains(StreamPacketFlags::Syn)
        })
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
        let mtu = state.common.mtu as usize;
        if sp < mtu {
            state.read_buf.cleanup();
            let sp = state.read_buf.get_space_left();
            if sp < mtu {
                state.ack_wait.cleanup();
                state.read_buf.cleanup();
                state.read_buf.get_space_left()
            } else {
                sp
            }
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
    mut send_fut_opt: RefMut<Option<futures::IPv6RawSocketSendtoFuture>>
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
    if state.send_fut.borrow().is_some() {
        return;
    }

    let data = state.next_data.replace(None).unwrap();

    let seqno = match data {
        NextData::Input(_) => state.common.next_seqno.0,
        NextData::Retransmission(_, s) => s
    };

    let fut = make_send_fut(
        &mut state.common,
        StreamPacketFlags::empty(),
        &data,
        Some(seqno)
    );
    state.send_fut.replace(Some(fut));

    if let NextData::Input(slice) = data {
        if state.ack_wait.is_full() {
            state.ack_wait.cleanup();
        }
        state.ack_wait.add(AckWait::new(Wrapping(seqno), slice));

        state.common.next_seqno += Wrapping(1);
    }
}

fn fill_next_data(state: &mut SendData) {
    if state.next_data.borrow().is_none() {
        let mut retransmit_queue = state.retransmit_queue.borrow_mut();
        if retransmit_queue.is_empty() {
            // respect window size
            if let Some(window_start) = state.ack_wait.first_seqno() {
                let stream_conf = get_stream_config(state.common.config);
                let window_size = stream_conf.window_size;
                let diff = (state.common.next_seqno - window_start).0 as u32;
                debug_assert!(diff <= window_size);
                if diff == window_size {
                    return;
                }
            }

            if let Some(slice) =
                    state.read_buf.take((state.common.mtu
                        - STREAM_CLIENT_HEADER_SIZE_WITH_IP as u16) as usize) {
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
    if let Async::Ready(Some((packet_buff, _))) = recv_async {
        let sc = get_stream_config(state.common.config);

        let window_start =
            match state.ack_wait.first_seqno() {
                Some(first) => first.0 as u32,
                None => return Ok(false)
            };
        let window_end = window_start + sc.window_size - 1;

        let packet = parse_stream_server_packet(&packet_buff);

        let mut got_new_ack = false;
        if packet.flags.contains(StreamPacketFlags::WS) {
            debug!("received WS+ACK for range [{}, {}]",
                packet.seqno_start, packet.seqno_end);
            let win_range = IRange(window_start, window_end);
            if win_range.contains_point(packet.seqno_start as u32) {
                if state.ack_wait.remove(
                    IRange(
                        Wrapping(window_start as u16),
                        Wrapping(packet.seqno_start)
                    )
                ) {
                    got_new_ack = true;
                }
            }
        } else {
            debug!("received ACK for range [{}, {}]",
                packet.seqno_start, packet.seqno_end);
        }

        if state.ack_wait.remove(
            IRange(
                Wrapping(packet.seqno_start),
                Wrapping(packet.seqno_end)
            )
        ) {
            got_new_ack = true;
        }

        if got_new_ack {
            state.ack_timer.reset(make_packet_loss_instant());
            state.connection_timeout.reset(make_connection_timeout_instant());
        }

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
                return Ok(false);
            }
            for i in state.ack_wait.iter() {
                retransmit_queue.push_back((
                    (*i.data).into(),
                    i.seqno.0
                ));
            }
        }
        state.ack_timer.reset(make_packet_loss_instant());

        return Ok(true);
    }

    if let Async::Ready(_) = state.connection_timeout.poll()? {
        return Err(Error::TimedOut.into());
    }

    Ok(false)
}

pub struct StreamCommonState<'a> {
    pub config: &'a Config,
    pub src: SocketAddrV6,
    pub dst: SocketAddrV6,
    pub sock: futures::IPv6RawSocketAdapter,
    pub mtu: u16,
    pub data_source: StdinBytesReader<'a>,
    pub send_buf: BytesMut,
    pub recv_buf: BytesMut,
    pub next_seqno: Wrapping<u16>,
}
