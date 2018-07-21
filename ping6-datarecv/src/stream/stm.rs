#![allow(large_enum_variant, type_complexity)]

use ::std::net::*;
use ::std::num::Wrapping;
use ::std::sync::*;
use ::std::time::*;

use ::bytes::*;
use ::futures::future::*;
use ::futures::prelude::*;
use ::futures::stream::unfold;
use ::futures::task::*;
use ::owning_ref::OwningRef;
use ::state_machine_future::RentToOwn;
use ::tokio::io::*;
use ::tokio::timer::*;

use ::linux_network::*;
use ::ping6_datacommon::*;
use ::send_box::SendBox;

use ::config::Config;
use ::errors::{Error, Result};
use ::stream::buffers::*;
use ::stream::packet::*;
use ::stream::stdout::StdoutBytesWriter;
use ::stream::util::make_send_fut;

type FutureE<T> = dyn(::futures::Future<Item = T, Error = ::failure::Error>);
type StreamE<T> = dyn(::futures::stream::Stream<
    Item = T,
    Error = ::failure::Error
>);

#[derive(StateMachineFuture)]
pub enum StreamMachine<'s> {
    #[state_machine_future(start, transitions(WaitForFirstSyn))]
    InitState {
        common: StreamCommonState<'s>
    },

    #[state_machine_future(transitions(SendSynAck))]
    WaitForFirstSyn {
        common: StreamCommonState<'s>,
        recv_first_syn: SendBox<FutureE<(Bytes, SocketAddrV6)>>
    },

    #[state_machine_future(transitions(WaitForAck))]
    SendSynAck {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState,
        send_syn_ack: futures::IPv6RawSocketSendtoFuture,
        next_action: Option<SendBox<
            StreamE<TimedResult<(Bytes, SocketAddrV6)>>
        >>
    },

    #[state_machine_future(transitions(WaitForFirstSyn, SendSynAck, ReceivePackets))]
    WaitForAck {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState,
        recv_stream: SendBox<StreamE<
            TimedResult<(Bytes, SocketAddrV6)>
        >>
    },

    #[state_machine_future(transitions(SendFinAck, SendFin))]
    ReceivePackets {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState,
        task: Arc<Mutex<Option<Task>>>,
        recv_stream: SendBox<StreamE<(Bytes, SocketAddrV6)>>,
        write_future: Option<WriteBorrow<StdoutBytesWriter>>,
        timeout: Delay,
        ack_sender_handle: AckGenHandle,
        fin_seqno: Option<u16>
    },

    #[state_machine_future(transitions(WaitForLastAck))]
    SendFinAck {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState,
        fin_seqno: u16,
        send_fut: futures::IPv6RawSocketSendtoFuture,
        next_action: Option<SendBox<
            StreamE<TimedResult<(Bytes, SocketAddrV6)>>
        >>
    },

    #[state_machine_future(transitions(SendFinAck, ConnectionTerminated))]
    WaitForLastAck {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState,
        fin_seqno: u16,
        recv_stream: SendBox<StreamE<
            TimedResult<(Bytes, SocketAddrV6)>
        >>
    },

    #[state_machine_future(transitions(WaitForFinAck))]
    SendFin {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState
    },

    #[state_machine_future(transitions(SendFin, SendLastAck))]
    WaitForFinAck {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    SendLastAck {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState
    },

    #[state_machine_future(ready)]
    ConnectionTerminated(TerminationReason),

    #[state_machine_future(error)]
    ErrorState(::failure::Error)
}

pub enum TerminationReason {
    DataReceived,
    Interrupted
}

pub struct WriteBorrow<T>(WriteAll<T, OwningRef<TrimmingBufferSlice, [u8]>>);

impl<T> WriteBorrow<T> where T: AsyncWrite {
    fn new(write: T, buf: TrimmingBufferSlice) -> WriteBorrow<T> {
        WriteBorrow(
            write_all(
                write,
                OwningRef::new(buf).map(|x|
                    &(*x)[STREAM_CLIENT_FULL_HEADER_SIZE..]
                )
            )
        )
    }
}

impl<T> Future for WriteBorrow<T> where T: AsyncWrite {
    type Item = ();
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0.poll().map(|res| {
            match res {
                Async::Ready(_) => Async::Ready(()),
                Async::NotReady => Async::NotReady
            }
        }).map_err(Error::IoError)
    }
}

impl<'s> PollStreamMachine<'s> for StreamMachine<'s> {
    fn poll_init_state<'a>(
        state: &'a mut RentToOwn<'a, InitState<'s>>
    ) -> Poll<AfterInitState<'s>, ::failure::Error> {
        let recv_first_syn = make_recv_first_syn(&mut state.common);
        transition!(WaitForFirstSyn {
            common: state.take().common,
            recv_first_syn
        })
    }

    fn poll_wait_for_first_syn<'a>(
        state: &'a mut RentToOwn<'a, WaitForFirstSyn<'s>>
    ) -> Poll<AfterWaitForFirstSyn<'s>, ::failure::Error> {
        debug!("waiting for first SYN");
        let (data, dst) = try_ready!(state.recv_first_syn.poll());

        let mut common = state.take().common;

        let packet = parse_stream_client_packet(&data);

        let next_seqno = Wrapping(packet.seqno) + Wrapping(1);
        let seqno_tracker = Arc::new(Mutex::new(
            SeqnoTracker::new(next_seqno + Wrapping(1))
        ));

        let seqno_tracker_clone = seqno_tracker.clone();
        let active = ActiveStreamCommonState {
            dst,
            next_seqno,
            order: Arc::new(Mutex::new(
                DataOrderer::new(common.window_size, common.mtu)
            )),
            seqno_tracker,
            ack_gen: Some(TimedAckSeqnoGenerator::new(
                seqno_tracker_clone,
                Duration::from_millis(ACK_SEND_PERIOD)
            ))
        };

        let send_future = make_syn_ack_future(
            &mut common,
            dst,
            packet.seqno,
            packet.seqno
        );

        transition!(SendSynAck {
            common,
            active,
            send_syn_ack: send_future,
            next_action: None
        })
    }

    fn poll_send_syn_ack<'a>(
        state: &'a mut RentToOwn<'a, SendSynAck<'s>>
    ) -> Poll<AfterSendSynAck<'s>, ::failure::Error> {
        debug!("sending SYN+ACK");
        let size = try_ready!(state.send_syn_ack.poll());
        debug_assert_eq!(size, STREAM_SERVER_FULL_HEADER_SIZE);

        let SendSynAck { mut common, active, next_action, .. }
            = state.take();

        let timed_packets = next_action.unwrap_or_else(|| {
            let seqno = active.next_seqno;
            let seqno_tracker_ref = active.seqno_tracker.clone();
            let order_ref = active.order.clone();
            let packets = make_recv_packets_stream(&mut common)
                .and_then(move |(data, dst)| {
                    let pass = {
                        let packet = parse_stream_client_packet(&data);

                        if packet.flags == StreamPacketFlags::Ack
                                && packet.seqno == seqno.0 {
                            true
                        } else {
                            let mut order = order_ref.lock().unwrap();
                            if order.get_space_left() < data.len() {
                                return Err(
                                    Error::RecvBufferOverrunOnStart.into()
                                );
                            }
                            seqno_tracker_ref.lock().unwrap()
                                .add(Wrapping(packet.seqno));
                            order.add(&data);
                            false
                        }
                    };

                    if pass {
                        Ok(Some((data, dst)))
                    } else {
                        Ok(None)
                    }
                }).filter_map(|x| x);
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

        transition!(WaitForAck {
            common,
            active,
            recv_stream: timed_packets
        })
    }

    fn poll_wait_for_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForAck<'s>>
    ) -> Poll<AfterWaitForAck<'s>, ::failure::Error> {
        debug!("waiting for first ACK");
        let (_,_) = match state.recv_stream.poll() {
            Err(e) => return Err(e),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(Some(TimedResult::InTime(x)))) => x,
            Ok(Async::Ready(Some(TimedResult::TimedOut))) => {
                let WaitForAck { mut common, mut active, recv_stream }
                    = state.take();

                let seqno = active.next_seqno - Wrapping(1);
                let send_future = make_syn_ack_future(
                    &mut common,
                    active.dst,
                    seqno.0,
                    seqno.0
                );

                transition!(SendSynAck {
                    common,
                    active,
                    send_syn_ack: send_future,
                    next_action: Some(recv_stream)
                });
            }
            Ok(Async::Ready(None)) => {
                info!("connection attempt timed out, \
                    waiting for new connection");
                let mut st = state.take();
                let recv_first_syn = make_recv_first_syn(&mut st.common);
                transition!(WaitForFirstSyn {
                    common: st.common,
                    recv_first_syn
                });
            }
        };

        state.active.next_seqno += Wrapping(1);

        info!("connection established");

        let WaitForAck { mut common, mut active, .. } = state.take();

        // things to move into the lambdas
        let task = Arc::new(Mutex::new(None));
        let task_clone = task.clone();
        let main_task = ::futures::task::current();
        let mut ack_gen = active.ack_gen.take().unwrap();
        let ack_sender_handle = ack_gen.handle();
        ack_gen.start();
        let ack_sender = ::stream::ack_sender::AckSender::new(
            ack_gen,
            *common.src.ip(),
            active.dst,
            common.mtu,
            common.sock.clone()
        );

        // spawn the ack packet sending task
        common.handle.spawn(
            lazy(move || {
                // a hack to get the task handle out of spawn()
                task_clone.lock().unwrap()
                    .get_or_insert(::futures::task::current());
                main_task.notify();
                Ok(())
            }).and_then(move |_| {
                ack_sender
            })
        );
        debug!("spawned ACK sending task");

        let seqno_tracker_ref = active.seqno_tracker.clone();
        let window_size = common.window_size;
        let recv_stream = Box::new(make_recv_packets_stream(&mut common).filter(
            move |&(ref data,_)| {
                let packet = parse_stream_client_packet(&data);

                let seqno = seqno_tracker_ref.lock().unwrap()
                    .pos_to_sequential(Wrapping(packet.seqno));
                debug!("packet position in the receive window: {}",
                    seqno as u32 + 1);

                seqno < window_size as usize
                    && !packet.flags.contains(StreamPacketFlags::Syn)
                    && !packet.flags.contains(StreamPacketFlags::Ack)
            }
        ));

        let timeout = Delay::new(make_connection_timeout_delay());

        transition!(
            ReceivePackets {
                common,
                active,
                task,
                recv_stream: unsafe { SendBox::new(Box::new(recv_stream)) },
                write_future: None,
                timeout,
                ack_sender_handle,
                fin_seqno: None
            }
        )
    }

    fn poll_receive_packets<'a>(
        state: &'a mut RentToOwn<'a, ReceivePackets<'s>>
    ) -> Poll<AfterReceivePackets<'s>, ::failure::Error> {
        let ack_sending_task = match *state.task.lock().unwrap() {
            Some(ref task) => task.clone(),
            None => return Ok(Async::NotReady)
        };

        debug!("receiving packets");

        let mut activity = true;
        while activity {
            let mtu = state.common.mtu;
            let space = clean_and_get_space(
                &mut state.active.order.lock().unwrap(),
                mtu
            );

            activity = poll_recv_stream(&mut *state, space)?;
            activity = poll_write_data_fut(&mut *state)? || activity;
            activity = poll_make_write_data_fut(&mut *state)? || activity;
        }

        poll_timeout(&mut *state, &ack_sending_task)?;

        if state.fin_seqno.is_some()
                && state.active.seqno_tracker.lock().unwrap().is_empty()
                && state.active.order.lock().unwrap().is_empty() {
            let ReceivePackets {
                mut common,
                active,
                mut ack_sender_handle,
                fin_seqno,
                ..
            } = state.take();

            ack_sender_handle.stop();
            ack_sending_task.notify();

            let fin_seqno = fin_seqno.unwrap();
            let send_fut = make_fin_ack_future(
                &mut common,
                active.dst,
                fin_seqno
            );
            transition!(SendFinAck {
                common,
                active,
                fin_seqno,
                send_fut,
                next_action: None
            });
        }

        Ok(Async::NotReady)
    }

    fn poll_send_fin_ack<'a>(
        state: &'a mut RentToOwn<'a, SendFinAck<'s>>
    ) -> Poll<AfterSendFinAck<'s>, ::failure::Error> {
        debug!("sending FIN+ACK");
        let size = try_ready!(state.send_fut.poll());
        debug_assert_eq!(size, STREAM_SERVER_FULL_HEADER_SIZE);

        let SendFinAck { mut common, active, fin_seqno, next_action, .. }
            = state.take();

        let timed_packets = next_action.unwrap_or_else(|| {
            let seqno = fin_seqno;
            let packets = make_recv_packets_stream(&mut common)
                .and_then(move |(data, dst)| {
                    let pass = {
                        let packet = parse_stream_client_packet(&data);

                        packet.flags == StreamPacketFlags::Ack
                            && packet.seqno == seqno
                    };

                    if pass {
                        Ok(Some((data, dst)))
                    } else {
                        Ok(None)
                    }
                }).filter_map(|x| x);
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

        transition!(WaitForLastAck {
            common,
            active,
            fin_seqno,
            recv_stream: timed_packets
        })
    }

    #[allow(needless_return)]
    fn poll_wait_for_last_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForLastAck<'s>>
    ) -> Poll<AfterWaitForLastAck<'s>, ::failure::Error> {
        debug!("waiting for last ACK");

        match state.recv_stream.poll() {
            Err(e) => return Err(e),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(Some(TimedResult::InTime(_)))) =>
                transition!(ConnectionTerminated(
                    TerminationReason::DataReceived
                )),
            Ok(Async::Ready(Some(TimedResult::TimedOut))) => {
                let WaitForLastAck { mut common, mut active, fin_seqno, recv_stream }
                    = state.take();

                let send_future = make_fin_ack_future(
                    &mut common,
                    active.dst,
                    fin_seqno
                );

                transition!(SendFinAck {
                    common,
                    active,
                    fin_seqno,
                    send_fut: send_future,
                    next_action: Some(recv_stream)
                });
            }
            Ok(Async::Ready(None)) => {
                debug!("timeout while waiting for the last ACK packet");
                return Err(Error::TimedOut.into());
            }
        }
    }

    fn poll_send_fin<'a>(
        _state: &'a mut RentToOwn<'a, SendFin<'s>>
    ) -> Poll<AfterSendFin<'s>, ::failure::Error> {
        debug!("sending FIN");
        unimplemented!()
    }

    fn poll_wait_for_fin_ack<'a>(
        _state: &'a mut RentToOwn<'a, WaitForFinAck<'s>>
    ) -> Poll<AfterWaitForFinAck<'s>, ::failure::Error> {
        debug!("waiting for FIN+ACK");
        unimplemented!()
    }

    fn poll_send_last_ack<'a>(
        _state: &'a mut RentToOwn<'a, SendLastAck<'s>>
    ) -> Poll<AfterSendLastAck, ::failure::Error> {
        debug!("sending lack ACK");
        unimplemented!()
    }
}

fn make_recv_packets_stream<'a>(
    common: &mut StreamCommonState<'a>
) -> impl Stream<Item = (Bytes, SocketAddrV6), Error = ::failure::Error> {
    let csrc = common.src;
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
            Some(sock.recvfrom(recv_buf.take(), RecvFlags::empty())
                .map_err(|e| e.into())
                .map(move |x| (x, (sock, recv_buf, mtu)))
            )
        }
    ).filter(move |&(ref x, src)| {
        let res = validate_stream_packet(
            &x,
            Some((*src.ip(), *csrc.ip()))
        );
        let flags = parse_stream_client_packet(&x).flags;
        if res && !flags.contains(StreamPacketFlags::WS) {
            debug!("valid packet received");
            true
        } else {
            debug!("invalid packet filtered out");
            false
        }
    })
}

fn make_recv_first_syn(common: &mut StreamCommonState)
        -> SendBox<FutureE<(Bytes, SocketAddrV6)>> {
    let recv_future = make_recv_packets_stream(common)
        .filter(|&(ref data, _)| {
            let packet = parse_stream_client_packet(&data);

            let res = packet.flags == StreamPacketFlags::Syn;
            if !res {
                debug!("not a SYN packet, dropping");
            }
            res
        }).into_future()
        .map(|(x,_)| x.unwrap())
        .map_err(|(e,_)| e);
    unsafe { SendBox::new(Box::new(recv_future)) }
}

fn make_syn_ack_future<'a>(
    common: &mut StreamCommonState<'a>,
    dst: SocketAddrV6,
    seqno_start: u16,
    seqno_end: u16
) -> futures::IPv6RawSocketSendtoFuture {
    make_send_fut(
        common,
        dst,
        StreamPacketFlags::Syn | StreamPacketFlags::Ack,
        seqno_start,
        seqno_end,
        &[]
    )
}

fn make_fin_ack_future<'a>(
    common: &mut StreamCommonState<'a>,
    dst: SocketAddrV6,
    seqno: u16
) -> futures::IPv6RawSocketSendtoFuture {
    make_send_fut(
        common,
        dst,
        StreamPacketFlags::Fin | StreamPacketFlags::Ack,
        seqno,
        seqno,
        &[]
    )
}

fn make_connection_timeout_delay() -> Instant {
    Instant::now() + Duration::from_millis(
        CONNECTION_LOSS_TIMEOUT
    )
}

fn clean_and_get_space(order: &mut DataOrderer, mtu: u16) -> usize {
    let space_required = mtu - STREAM_CLIENT_HEADER_SIZE_WITH_IP as u16;
    if order.get_space_left() < space_required as usize {
        order.cleanup();
    }
    order.get_space_left()
}

fn poll_recv_stream(
    state: &mut ReceivePackets,
    space: usize
) -> Result<bool> {
    if space < state.common.mtu as usize {
        return Ok(false);
    }

    if let Async::Ready(Some((data,_)))
            = state.recv_stream.poll()? {
        let packet = parse_stream_client_packet(&data);

        if data.len() > state.common.mtu as usize {
            return Err(Error::MtuLessThanReal {
                packet_size: data.len() as u16
            }.into());
        }

        if packet.flags.contains(StreamPacketFlags::Fin) {
            if let Some(seqno) = state.fin_seqno {
                if seqno != packet.seqno {
                    warn!("double FIN packet with different seqno");
                    return Ok(false);
                }
            }
            state.fin_seqno = Some(packet.seqno);
            return Ok(true);
        }

        if ::log::max_log_level() >= ::log::LogLevelFilter::Debug {
            let packet = parse_stream_client_packet(&data);
            debug!("received packet with seqno {}", packet.seqno);
        }

        let seqno_is_new = state.active.seqno_tracker.lock().unwrap()
            .add(Wrapping(packet.seqno));
        if seqno_is_new && !packet.payload.is_empty() {
            state.timeout.reset(make_connection_timeout_delay());
            state.active.order.lock().unwrap().add(&data);
        }

        return Ok(true);
    }

    Ok(false)
}

fn poll_write_data_fut(state: &mut ReceivePackets) -> Result<bool> {
    if state.write_future.is_some() {
        if let Async::Ready(_)
                = state.write_future.as_mut().unwrap().poll()? {
            state.write_future.take();
            return Ok(true);
        }
    }
    Ok(false)
}

fn poll_make_write_data_fut(state: &mut ReceivePackets) -> Result<bool> {
    if state.write_future.is_none() {
        let peeked_seqno = state.active.order.lock().unwrap()
            .peek_seqno();
        if peeked_seqno == Some(state.active.next_seqno.0) {
            state.active.next_seqno += Wrapping(1);
            let data = state.active.order.lock().unwrap()
                .take().unwrap();
            state.write_future = Some(WriteBorrow::new(
                state.common.data_out.clone(),
                data
            ));
            return Ok(true);
        }
    }
    Ok(false)
}

fn poll_timeout(
    state: &mut ReceivePackets,
    ack_sending_task: &Task
) -> Result<()> {
    if let Async::Ready(_) = state.timeout.poll()? {
        state.ack_sender_handle.stop();
        ack_sending_task.notify();
        return Err(Error::TimedOut.into());
    }
    Ok(())
}

pub struct StreamCommonState<'a> {
    pub config: &'a Config,
    pub src: SocketAddrV6,
    pub window_size: u32,
    pub sock: futures::IPv6RawSocketAdapter,
    pub mtu: u16,
    pub data_out: StdoutBytesWriter,
    pub send_buf: BytesMut,
    pub recv_buf: BytesMut,
    pub handle: ::tokio::runtime::TaskExecutor
}

pub struct ActiveStreamCommonState {
    dst: SocketAddrV6,
    next_seqno: Wrapping<u16>,
    order: Arc<Mutex<DataOrderer>>,
    seqno_tracker: Arc<Mutex<SeqnoTracker>>,
    ack_gen: Option<TimedAckSeqnoGenerator>
}
