use ::std::cell::RefCell;
use ::std::net::SocketAddrV6;
use ::std::rc::Rc;

use ::futures::prelude::*;
use ::pnet_packet::Packet;
use ::state_machine_future::RentToOwn;
use ::tokio_timer::*;

use ::linux_network::*;

use ::config::Config;
use ::errors::Error;
use ::stdin_iterator::StdinBytesReader;
use ::stream::constants::*;
use ::stream::packet::*;

#[derive(StateMachineFuture)]
pub enum StreamMachine<'s> {
    #[state_machine_future(start, transitions(SendFirstSyn))]
    InitState {
        common: StreamState<'s>
    },

    #[state_machine_future(transitions(WaitForSynAck))]
    SendFirstSyn {
        common: StreamState<'s>,
        send: futures::IpV6RawSocketSendtoFuture<'s>,
        try_number: u32
    },

    #[state_machine_future(transitions(SendFirstSyn, SendAck))]
    WaitForSynAck {
        common: StreamState<'s>,
        try_number: u32
    },

    #[state_machine_future(transitions(SendData))]
    SendAck {
        common: StreamState<'s>
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

macro_rules! reset_lifetime {
    ($v:expr; $t:ty) => (($v as *const $t).as_ref().unwrap());
    ($v:expr; mut $t:ty) => (($v as *mut $t).as_mut().unwrap())
}

macro_rules! get_sock {
    (mut $c:ident) => (
        reset_lifetime!(&mut *$c.sock; mut futures::IpV6RawSocketAdapter)
    )
}

macro_rules! get_buf {
    ($c:ident, $size:expr) => (
        reset_lifetime!(&$c.buf.borrow()[0 .. $size as usize]; [u8])
    );
    (mut $c:ident, $size:expr) => (
        reset_lifetime!(&mut $c.buf.borrow_mut()[0 .. $size as usize]; mut [u8])
    )
}

impl<'s> PollStreamMachine<'s> for StreamMachine<'s> {
    fn poll_init_state<'a>(
        state: &'a mut RentToOwn<'a, InitState<'s>>
    ) -> Poll<AfterInitState<'s>, Error> {
        let mut common = state.take().common;

        let dst = common.dst;
        let packet = make_stream_packet(
            unsafe {
                get_buf!(mut common, FULL_HEADER_SIZE)
            },
            *common.src.ip(),
            *dst.ip(),
            common.next_seqno,
            StreamPacketFlags::Syn.into(),
            &[]
        );

        let mut send_future = unsafe {
            get_sock!(mut common).sendto(
                get_buf!(common, FULL_HEADER_SIZE),
                dst,
                SendFlagSet::new()
            )
        };

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
        debug_assert!(size == FULL_HEADER_SIZE as usize);
        unimplemented!()
    }

    fn poll_wait_for_syn_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForSynAck<'s>>
    ) -> Poll<AfterWaitForSynAck<'s>, Error> {
        unimplemented!()
    }

    fn poll_send_ack<'a>(
        state: &'a mut RentToOwn<'a, SendAck<'s>>
    ) -> Poll<AfterSendAck<'s>, Error> {
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

pub struct StreamState<'a> {
    pub config: &'a Config,
    pub src: SocketAddrV6,
    pub dst: SocketAddrV6,
    pub sock: Box<futures::IpV6RawSocketAdapter>,
    pub data_source: StdinBytesReader<'a>,
    pub timer: Timer,
    pub buf: RefCell<Vec<u8>>,
    pub next_seqno: u16
}
