use ::std::cell::*;
use ::std::net::*;
use ::std::num::Wrapping;
use ::std::ops::*;
use ::std::sync::{atomic::{fence, Ordering}, *};
use ::std::time::*;

use ::futures::future::*;
use ::futures::prelude::*;
use ::futures::stream::unfold;
use ::futures::task::*;
use ::owning_ref::*;
use ::pnet_packet::Packet;
use ::state_machine_future::RentToOwn;
use ::tokio::io::*;
use ::tokio::prelude::*;
use ::tokio::timer::*;

use ::linux_network::*;
use ::linux_network::futures::U8Slice;
use ::ping6_datacommon::*;
use ::send_box::SendBox;
use ::sliceable_rcref::*;

use ::config::Config;
use ::errors::{Error, ErrorKind};
use ::stdout_iterator::StdoutBytesWriter;
use ::stream::buffers::*;
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
        recv_first_syn: SendBox<FutureE<(U8Slice, SocketAddrV6)>>
    },

    #[state_machine_future(transitions(WaitForAck))]
    SendSynAck {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState,
        send_syn_ack: futures::IpV6RawSocketSendtoFuture,
        next_action: Option<SendBox<
            StreamE<TimedResult<(U8Slice,SocketAddrV6)>>
        >>
    },

    #[state_machine_future(transitions(SendSynAck, ReceivePackets))]
    WaitForAck {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState,
        recv_stream: SendBox<StreamE<
            TimedResult<(U8Slice, SocketAddrV6)>
        >>
    },

    #[state_machine_future(transitions(SendFinAck, SendFin))]
    ReceivePackets {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState,
        task: Arc<Mutex<Cell<Option<Task>>>>,
        recv_stream: SendBox<StreamE<(U8Slice, SocketAddrV6)>>,
        write_future: Option<WriteBorrowing<StdoutBytesWriter<'s>>>,
        timeout: Delay
    },

    #[state_machine_future(transitions(WaitForLastAck))]
    SendFinAck {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState
    },

    #[state_machine_future(transitions(SendFinAck, ConnectionTerminated))]
    WaitForLastAck {
        common: StreamCommonState<'s>,
        active: ActiveStreamCommonState
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
    ErrorState(Error)
}

pub enum TerminationReason {
    DataReceived,
    Interrupted
}

pub struct WriteBorrowing<T>(WriteAll<T, TrimmingBufferSlice>);

impl<T> WriteBorrowing<T> where T: AsyncWrite {
    fn new(write: T, buf: TrimmingBufferSlice) -> WriteBorrowing<T> {
        WriteBorrowing(
            write_all(write, buf)
        )
    }
}

impl<T> Future for WriteBorrowing<T> where T: AsyncWrite {
    type Item = ();
    type Error = Error;
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0.poll().map(|res| {
            match res {
                Async::Ready(_) => Async::Ready(()),
                Async::NotReady => Async::NotReady
            }
        }).map_err(|e| e.into())
    }
}

impl<'s> PollStreamMachine<'s> for StreamMachine<'s> {
    fn poll_init_state<'a>(
        state: &'a mut RentToOwn<'a, InitState<'s>>
    ) -> Poll<AfterInitState<'s>, Error> {
        let mut common = state.take().common;

        let recv_future = make_recv_packets_stream(&mut common)
            .filter(|&(ref x, _)| {
                let data_ref = x.lock();
                let packet = parse_stream_client_packet(&data_ref);

                packet.flags == StreamPacketFlags::Syn.into()
            })
            .into_future()
            .map(|(x,_)| x.unwrap())
            .map_err(|(e,_)| e);

        transition!(WaitForFirstSyn {
            common: common,
            recv_first_syn: unsafe { SendBox::new(Box::new(recv_future)) }
        })
    }

    fn poll_wait_for_first_syn<'a>(
        state: &'a mut RentToOwn<'a, WaitForFirstSyn<'s>>
    ) -> Poll<AfterWaitForFirstSyn<'s>, Error> {
        let (data_ref, dst) = try_ready!(state.recv_first_syn.poll());

        let mut common = state.take().common;

        let data = data_ref.lock();
        let packet = parse_stream_client_packet(&data);

        let next_seqno = Wrapping(packet.seqno) + Wrapping(1);
        let seqno_tracker = Arc::new(Mutex::new(
            SeqnoTracker::new(next_seqno)
        ));

        let seqno_tracker_clone = seqno_tracker.clone();
        let active = ActiveStreamCommonState {
            dst: dst,
            next_seqno: next_seqno,
            order: Arc::new(Mutex::new(
                DataOrderer::new(common.window_size as usize)
            )),
            seqno_tracker: seqno_tracker,
            ack_gen: Some(TimedAckSeqnoGenerator::new(
                seqno_tracker_clone,
                Duration::from_millis(PACKET_LOSS_TIMEOUT as u64)
            ))
        };

        let send_future = make_syn_ack_future(
            &mut common,
            dst,
            packet.seqno,
            packet.seqno
        );

        transition!(SendSynAck {
            common: common,
            active: active,
            send_syn_ack: send_future,
            next_action: None
        })
    }

    fn poll_send_syn_ack<'a>(
        state: &'a mut RentToOwn<'a, SendSynAck<'s>>
    ) -> Poll<AfterSendSynAck<'s>, Error> {
        let size = try_ready!(state.send_syn_ack.poll());
        debug_assert!(size == STREAM_SERVER_FULL_HEADER_SIZE as usize);

        let SendSynAck { mut common, active, next_action, .. }
            = state.take();

        let timed_packets = next_action.unwrap_or_else(|| {
            let seqno = active.next_seqno;
            let seqno_tracker_ref = active.seqno_tracker.clone();
            let order_ref = active.order.clone();
            let packets = make_recv_packets_stream(&mut common)
                .and_then(move |(data_ref, dst)| {
                    let pass = {
                        let data = data_ref.lock();
                        let packet = parse_stream_client_packet(&data);

                        seqno_tracker_ref.lock().unwrap()
                            .add(Wrapping(packet.seqno));

                        if packet.flags == StreamPacketFlags::Ack.into()
                                && packet.seqno == seqno.0 {
                            true
                        } else {
                            let order = order_ref.lock().unwrap();
                            if order.get_space_left() < data.len() {
                                bail!(ErrorKind::RecvBufferOverrunOnStart);
                            }
                            order_ref.lock().unwrap().add(&data);
                            false
                        }
                    };

                    match pass {
                        true => Ok(Some((data_ref, dst))),
                        false => Ok(None)
                    }
                }).filter_map(|x| x);
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

        transition!(WaitForAck {
            common: common,
            active: active,
            recv_stream: timed_packets
        })
    }

    fn poll_wait_for_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForAck<'s>>
    ) -> Poll<AfterWaitForAck<'s>, Error> {
        let (_,_) = match state.recv_stream.poll() {
            Err(e) => bail!(e),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(Some(TimedResult::InTime(x)))) => x,
            Ok(Async::Ready(Some(TimedResult::TimedOut))) => {
                let mut st = state.take();
                let seqno = st.active.next_seqno - Wrapping(1);
                let send_future = make_syn_ack_future(
                    &mut st.common,
                    st.active.dst,
                    seqno.0,
                    seqno.0
                );
                transition!(SendSynAck {
                    common: st.common,
                    active: st.active,
                    send_syn_ack: send_future,
                    next_action: Some(st.recv_stream)
                })
            }
            Ok(Async::Ready(None)) => bail!(ErrorKind::TimedOut)
        };

        let WaitForAck { mut common, mut active, .. } = state.take();

        let task = Arc::new(Mutex::new(Cell::new(None)));

        // stuff to move into spawned closure
        let mut ack_gen = active.ack_gen.take().expect("an ack range generator");
        ack_gen.start();
        let send_src = *common.src.ip();
        let send_dst = active.dst;
        let send_buf_full_ref = common.send_buf.clone();
        let send_sock = common.sock.clone();
        let task_clone = task.clone();
        let main_task = ::futures::task::current();

        // spawn the ack packet sending task
        common.handle.spawn(
            lazy(move || {
                // a hack to get the task handle out of spawn()
                task_clone.lock().unwrap()
                    .set(Some(::futures::task::current()));
                main_task.notify();
                ok(())
            }).and_then(move |_| {
                ack_gen.map_err(|_| ()).for_each(move |ranges| {
                    // can not move out of the outer closure
                    let send_buf_full_ref = send_buf_full_ref.clone();
                    let mut send_sock = send_sock.clone();
                    join_all(ranges.into_iter().map(move |IRange(l,r)| {
                        let send_buf_ref = send_buf_full_ref
                            .range(0 .. STREAM_SERVER_FULL_HEADER_SIZE as usize);

                        make_stream_server_icmpv6_packet(
                            &mut send_buf_ref.borrow_mut(),
                            send_src,
                            *send_dst.ip(),
                            l.0,
                            r.0,
                            StreamPacketFlags::Ack.into(),
                            &[]
                        );

                        send_sock.sendto(
                            send_buf_ref,
                            send_dst,
                            SendFlagSet::new()
                        ).map_err(|_| ())
                    })).map(|_| ())
                })
            })
        );

        let seqno_tracker_ref = active.seqno_tracker.clone();
        let window_size = common.window_size;
        let recv_stream = Box::new(make_recv_packets_stream(&mut common).filter(
            move |&(ref x,_)| {
                let data = x.lock();
                let packet = parse_stream_client_packet(&data);

                let seqno = seqno_tracker_ref.lock().unwrap()
                    .to_sequential(Wrapping(packet.seqno));

                seqno < window_size as usize
                    && !packet.flags.test(StreamPacketFlags::Syn.into())
                    && !packet.flags.test(StreamPacketFlags::Ack.into())
            }
        ));

        let timeout = make_connection_timeout(&common);

        transition!(
            ReceivePackets {
                common: common,
                active: active,
                task: task,
                recv_stream: unsafe { SendBox::new(Box::new(recv_stream)) },
                write_future: None,
                timeout: timeout
            }
        )
    }

    fn poll_receive_packets<'a>(
        state: &'a mut RentToOwn<'a, ReceivePackets<'s>>
    ) -> Poll<AfterReceivePackets<'s>, Error> {
        let ack_sending_task = {
            let task_guard = state.task.lock().unwrap();
            let task_opt = task_guard.take();
            if task_opt.is_none() {
                return Ok(Async::NotReady);
            }
            task_opt.unwrap()
        };

        let mut activity = true;
        while activity {
            activity = false;

            if state.active.order.lock().unwrap().get_space_left()
                    >= state.common.mtu as usize {
                if let Async::Ready(Some((data_ref,_)))
                        = state.recv_stream.poll()? {
                    state.timeout = make_connection_timeout(&state.common);

                    let data = data_ref.lock();
                    let packet = parse_stream_client_packet(&data);

                    if data.len() > state.common.mtu as usize {
                        bail!(ErrorKind::MtuLessThanReal(data.len() as u16));
                    }

                    if packet.flags.test(StreamPacketFlags::Fin) {
                        unimplemented!()
                    }

                    state.active.seqno_tracker.lock().unwrap()
                        .add(Wrapping(packet.seqno));
                    state.active.order.lock().unwrap().add(&data);

                    ack_sending_task.notify();

                    activity = true;
                }
            }

            let mut write_future = state.write_future.take();
            if write_future.is_some() {
                if let Async::Ready(_)
                        = write_future.as_mut().unwrap().poll()? {
                    activity = true;
                    write_future = None;
                }
            }

            if write_future.is_none() {
                let peeked_seqno = state.active.order.lock().unwrap()
                    .peek_seqno();
                if peeked_seqno == Some(state.active.next_seqno.0) {
                    state.active.next_seqno += Wrapping(1);
                    write_future = Some(WriteBorrowing::new(
                        state.common.data_out.clone(),
                        state.active.order.lock().unwrap().take().unwrap()
                    ));
                }
            }

            state.write_future = write_future;
        }

        if let Async::Ready(_) = state.timeout.poll()? {
            bail!(ErrorKind::TimedOut);
        }

        Ok(Async::NotReady)
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
) -> Box<StreamE<(U8Slice, SocketAddrV6)>> {
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
            &x.lock(),
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
        StreamPacketFlags::Syn | StreamPacketFlags::Ack,
        &[]
    );

    common.sock.sendto(
        send_buf_ref,
        dst,
        SendFlagSet::new()
    )
}

fn make_connection_timeout<'a>(common: &StreamCommonState<'a>)
        -> Delay {
    Delay::new(Instant::now() + Duration::from_millis(
        PACKET_LOSS_TIMEOUT as u64 * RETRANSMISSIONS_NUMBER as u64
    ))
}

pub struct StreamCommonState<'a> {
    pub config: &'a Config,
    pub src: SocketAddrV6,
    pub window_size: u32,
    pub sock: futures::IpV6RawSocketAdapter,
    pub mtu: u16,
    pub data_out: StdoutBytesWriter<'a>,
    pub send_buf: SArcRef<Vec<u8>>,
    pub recv_buf: SArcRef<Vec<u8>>,
    pub handle: ::tokio::runtime::TaskExecutor
}

pub struct ActiveStreamCommonState {
    dst: SocketAddrV6,
    next_seqno: Wrapping<u16>,
    order: Arc<Mutex<DataOrderer>>,
    seqno_tracker: Arc<Mutex<SeqnoTracker>>,
    ack_gen: Option<TimedAckSeqnoGenerator>
}
