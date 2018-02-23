use ::std::cell::RefCell;
use ::std::net::SocketAddrV6;
use ::std::num::Wrapping;
use ::std::rc::Rc;
use ::std::time::Duration;

use ::futures::future;
use ::futures::prelude::*;
use ::futures::stream::*;
use ::pnet_packet::Packet;
use ::state_machine_future::RentToOwn;
use ::tokio_timer::*;

use ::linux_network::*;
use ::ping6_datacommon::*;
use ::sliceable_rcref::SRcRef;

use ::config::Config;
use ::errors::{Error, ErrorKind};
use ::stdin_iterator::StdinBytesReader;
use ::stream::packet::*;

type Stream<T> = ::futures::stream::Stream<Item = T, Error = Error>;

#[derive(StateMachineFuture)]
pub enum StreamMachine<'s> {
    #[state_machine_future(start, transitions(SendFirstSyn))]
    InitState {
        common: StreamState<'s>
    },

    #[state_machine_future(transitions(WaitForSynAck))]
    SendFirstSyn {
        common: StreamState<'s>,
        send: futures::IpV6RawSocketSendtoFuture,
        try_number: u32
    },

    #[state_machine_future(transitions(SendFirstSyn, SendAck))]
    WaitForSynAck {
        common: StreamState<'s>,
        timed_recv: Box<
            Future<Item = (futures::U8Slice, SocketAddrV6),
            Error = Error>
        >,
        try_number: u32
    },

    #[state_machine_future(transitions(SendData))]
    SendAck {
        common: StreamState<'s>,
        send_ack: futures::IpV6RawSocketSendtoFuture
    },

    #[state_machine_future(transitions(ReceivedServerFin, SendFin, WaitForAck))]
    SendData {
        common: StreamState<'s>,

    },

    #[state_machine_future(transitions(ReceivedServerFin, SendData, SendFin))]
    WaitForAck {
        common: StreamState<'s>
    },

    #[state_machine_future(transitions(SendFinAck))]
    ReceivedServerFin {
        common: StreamState<'s>
    },

    #[state_machine_future(transitions(ReceivedServerFin, WaitForLastAck))]
    SendFinAck {
        common: StreamState<'s>
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    WaitForLastAck {
        common: StreamState<'s>
    },

    #[state_machine_future(transitions(WaitForFinAck))]
    SendFin {
        common: StreamState<'s>
    },

    #[state_machine_future(transitions(SendFin, SendLastAck))]
    WaitForFinAck {
        common: StreamState<'s>
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    SendLastAck {
        common: StreamState<'s>
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
        common.next_seqno += Wrapping(1);
        transition!(SendFirstSyn {
            common: common,
            send: send_future,
            try_number: 0
        })
    }

    fn poll_send_first_syn<'a>(
        state: &'a mut RentToOwn<'a, SendFirstSyn<'s>>
    ) -> Poll<AfterSendFirstSyn<'s>, Error> {
        let size = try_ready!(state.send.poll());
        debug_assert!(size == STREAM_CLIENT_FULL_HEADER_SIZE as usize);

        let state = state.take();
        let mut common = state.common;

        let recv_future = common.sock.recvfrom(
                common.recv_buf.range(0 .. common.mtu as usize),
                RecvFlagSet::new()
        ).map_err(Error::from);
        let timed_future = common.timer.timeout(
            recv_future,
            Duration::from_millis(PACKET_LOSS_TIMEOUT as u64)
        );
/*
        unfold(get_common!(mut common), |common| {
            Some(future::ok((
                get_sock!(mut common).recvfrom(
                    get_recv_buf!(mut common, common.mtu),
                    RecvFlagSet::new()
                ),
                common
            )))
        }).filter(|x| {

        });
*/
/*        transition!(WaitForSynAck {
            common: common,
            timed_recv: Box::new(timed_future),
            try_number: state.try_number + 1
        })*/
        unimplemented!()
    }

    fn poll_wait_for_syn_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForSynAck<'s>>
    ) -> Poll<AfterWaitForSynAck<'s>, Error> {
        let (data_ref, dst) = match state.timed_recv.poll() {
            Err(e) => {
                if let ErrorKind::TimedOut = *e.kind() {
                    if state.try_number <= RETRANSMISSIONS_NUMBER {
                        let mut st = state.take();
                        let send_future =
                            make_first_syn_future(&mut st.common);
                        return transition!(SendFirstSyn {
                            common: st.common,
                            send: send_future,
                            try_number: st.try_number
                        });
                    }
                }

                bail!(e)
            },
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Ok(Async::Ready(x)) => x
        };

        let mut state = state.take();
        let mut common = state.common;
        debug_assert!(dst == common.dst);

        let src = *common.src.ip();

        let data = data_ref.borrow();

        let packet_opt = parse_stream_packet(&data,
            Some((*dst.ip(), src)));
        let packet = match packet_opt {
            Some(x) => x,
            None => return Ok(Async::NotReady)
        };

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

        let ack_reply = make_stream_client_icmpv6_packet(
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
        });
    }

    fn poll_send_ack<'a>(
        state: &'a mut RentToOwn<'a, SendAck<'s>>
    ) -> Poll<AfterSendAck<'s>, Error> {
        let size = try_ready!(state.send_ack.poll());
        debug_assert!(size == STREAM_CLIENT_FULL_HEADER_SIZE as usize);

        unimplemented!()
    }

    fn poll_send_data<'a>(
        state: &'a mut RentToOwn<'a, SendData<'s>>
    ) -> Poll<AfterSendData<'s>, Error> {
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

fn make_first_syn_future<'a>(common: &mut StreamState<'a>)
        -> futures::IpV6RawSocketSendtoFuture {
    let dst = common.dst;
    let send_buf_ref = common.send_buf
        .range(0 .. STREAM_CLIENT_FULL_HEADER_SIZE as usize);
    let mut send_buf = send_buf_ref.borrow_mut();

    make_stream_client_icmpv6_packet(
        &mut send_buf,
        *common.src.ip(),
        *dst.ip(),
        common.next_seqno.0,
        StreamPacketFlags::Syn.into(),
        &[]
    );

    common.sock.sendto(
        common.send_buf.range(0 .. STREAM_CLIENT_FULL_HEADER_SIZE as usize),
        dst,
        SendFlagSet::new()
    )
}

pub struct StreamState<'a> {
    pub config: &'a Config,
    pub src: SocketAddrV6,
    pub dst: SocketAddrV6,
    pub sock: futures::IpV6RawSocketAdapter,
    pub mtu: u16,
    pub data_source: StdinBytesReader<'a>,
    pub timer: Timer,
    pub send_buf: SRcRef<Vec<u8>>,
    pub recv_buf: SRcRef<Vec<u8>>,
    pub next_seqno: Wrapping<u16>
}
