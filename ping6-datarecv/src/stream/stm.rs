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
        common: StreamState<'s>
    },

    #[state_machine_future(transitions(SendSynAck))]
    WaitForFirstSyn {
        common: StreamState<'s>,
        recv_future: Box<FutureE<(futures::U8Slice, SocketAddrV6)>>
    },

    #[state_machine_future(transitions(WaitForAck))]
    SendSynAck {
        common: StreamState<'s>,
        src: SocketAddrV6,
        dst: SocketAddrV6
    },

    #[state_machine_future(transitions(WaitForPackets))]
    WaitForAck {
        common: StreamState<'s>,
        src: SocketAddrV6,
        dst: SocketAddrV6
    },

    #[state_machine_future(transitions(SendFinAck, SendFin))]
    WaitForPackets {
        common: StreamState<'s>,
        src: SocketAddrV6,
        dst: SocketAddrV6
    },

    #[state_machine_future(transitions(WaitForLastAck))]
    SendFinAck {
        common: StreamState<'s>,
        src: SocketAddrV6,
        dst: SocketAddrV6
    },

    #[state_machine_future(transitions(SendFinAck, ConnectionTerminated))]
    WaitForLastAck {
        common: StreamState<'s>,
        src: SocketAddrV6,
        dst: SocketAddrV6
    },

    #[state_machine_future(transitions(WaitForFinAck))]
    SendFin {
        common: StreamState<'s>,
        src: SocketAddrV6,
        dst: SocketAddrV6
    },

    #[state_machine_future(transitions(SendFin, SendLastAck))]
    WaitForFinAck {
        common: StreamState<'s>,
        src: SocketAddrV6,
        dst: SocketAddrV6
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    SendLastAck {
        common: StreamState<'s>,
        src: SocketAddrV6,
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
            .map(|(x,s)| x.unwrap())
            .map_err(|(e,s)| e);

        transition!(WaitForFirstSyn {
            common: common,
            recv_future: Box::new(recv_future)
        })
    }

    fn poll_wait_for_first_syn<'a>(
        state: &'a mut RentToOwn<'a, WaitForFirstSyn<'s>>
    ) -> Poll<AfterWaitForFirstSyn<'s>, Error> {
        let (data_ref, dst) = try_ready!(state.recv_future.poll());

        unimplemented!()
    }

    fn poll_send_syn_ack<'a>(
        state: &'a mut RentToOwn<'a, SendSynAck<'s>>
    ) -> Poll<AfterSendSynAck<'s>, Error> {
        unimplemented!()
    }

    fn poll_wait_for_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForAck<'s>>
    ) -> Poll<AfterWaitForAck<'s>, Error> {
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
    common: &mut StreamState<'a>
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

pub struct StreamState<'a> {
    pub config: &'a Config,
    pub src: SocketAddrV6,
    pub sock: futures::IpV6RawSocketAdapter,
    pub mtu: u16,
    pub data_out: StdoutBytesWriter<'a>,
    pub timer: Timer,
    pub send_buf: SRcRef<Vec<u8>>,
    pub recv_buf: SRcRef<Vec<u8>>,
    pub next_seqno: Wrapping<u16>
}
