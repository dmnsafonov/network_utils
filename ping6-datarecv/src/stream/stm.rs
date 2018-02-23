use ::std::cell::RefCell;
use ::std::net::SocketAddrV6;
use ::std::num::Wrapping;
use ::std::time::Duration;

use ::futures::prelude::*;
use ::pnet_packet::Packet;
use ::state_machine_future::RentToOwn;
use ::tokio_timer::*;

use ::linux_network::*;
use ::ping6_datacommon::*;

use ::config::Config;
use ::errors::{Error, ErrorKind};
use ::stdout_iterator::StdoutBytesWriter;
use ::stream::packet::*;

#[derive(StateMachineFuture)]
pub enum StreamMachine<'s> {
    #[state_machine_future(start, transitions(WaitForFirstSyn))]
    InitState {
        common: Box<StreamState<'s>>
    },

    #[state_machine_future(transitions(SendSynAck))]
    WaitForFirstSyn {
        common: Box<StreamState<'s>>
    },

    #[state_machine_future(transitions(WaitForAck))]
    SendSynAck {
        common: Box<StreamState<'s>>
    },

    #[state_machine_future(transitions(WaitForPackets))]
    WaitForAck {
        common: Box<StreamState<'s>>
    },

    #[state_machine_future(transitions(SendFinAck, SendFin))]
    WaitForPackets {
        common: Box<StreamState<'s>>
    },

    #[state_machine_future(transitions(WaitForLastAck))]
    SendFinAck {
        common: Box<StreamState<'s>>
    },

    #[state_machine_future(transitions(SendFinAck, ConnectionTerminated))]
    WaitForLastAck {
        common: Box<StreamState<'s>>
    },

    #[state_machine_future(transitions(WaitForFinAck))]
    SendFin {
        common: Box<StreamState<'s>>
    },

    #[state_machine_future(transitions(SendFin, SendLastAck))]
    WaitForFinAck {
        common: Box<StreamState<'s>>
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    SendLastAck {
        common: Box<StreamState<'s>>
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
        unimplemented!()
    }

    fn poll_wait_for_first_syn<'a>(
        state: &'a mut RentToOwn<'a, WaitForFirstSyn<'s>>
    ) -> Poll<AfterWaitForFirstSyn<'s>, Error> {
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

pub struct StreamState<'a> {
    pub config: &'a Config,
    pub sock: futures::IpV6RawSocketAdapter,
    pub mtu: u16,
    pub data_out: StdoutBytesWriter<'a>,
    pub timer: Timer,
    pub send_buf: RefCell<Vec<u8>>,
    pub recv_buf: RefCell<Vec<u8>>,
    pub next_seqno: Wrapping<u16>
}
