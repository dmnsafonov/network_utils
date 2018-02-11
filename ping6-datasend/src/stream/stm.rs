use ::std::net::SocketAddrV6;

use ::futures::prelude::*;
use ::state_machine_future::RentToOwn;

use ::linux_network::futures;

use ::config::Config;
use ::errors::Error;
use ::stdin_iterator::StdinBytesReader;
use super::packet::*;

#[derive(StateMachineFuture)]
pub enum StreamMachine<'s> {
    #[state_machine_future(start, transitions(WaitForSynAck))]
    SendFirstSyn {
        init_state: StreamInitState<'s>,
        try_number: u32
    },

    #[state_machine_future(transitions(SendFirstSyn, SendAck))]
    WaitForSynAck {
        init_state: StreamInitState<'s>,
        try_number: u32
    },

    #[state_machine_future(transitions(SendData))]
    SendAck {
        init_state: StreamInitState<'s>
    },

    #[state_machine_future(transitions(ReceivedServerFin, SendFin, WaitForAck))]
    SendData {
        init_state: StreamInitState<'s>,

    },

    #[state_machine_future(transitions(ReceivedServerFin, SendData, SendFin))]
    WaitForAck {
        init_state: StreamInitState<'s>
    },

    #[state_machine_future(transitions(SendFinAck))]
    ReceivedServerFin {
        init_state: StreamInitState<'s>
    },

    #[state_machine_future(transitions(ReceivedServerFin, WaitForLastAck))]
    SendFinAck {
        init_state: StreamInitState<'s>
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    WaitForLastAck {
        init_state: StreamInitState<'s>
    },

    #[state_machine_future(transitions(WaitForFinAck))]
    SendFin {
        init_state: StreamInitState<'s>
    },

    #[state_machine_future(transitions(SendFin, SendLastAck))]
    WaitForFinAck {
        init_state: StreamInitState<'s>
    },

    #[state_machine_future(transitions(ConnectionTerminated))]
    SendLastAck {
        init_state: StreamInitState<'s>
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
    fn poll_send_first_syn<'a>(
        state: &'a mut RentToOwn<'a, SendFirstSyn>
    ) -> Poll<AfterSendFirstSyn<'s>, Error> {
        unimplemented!()
    }

    fn poll_wait_for_syn_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForSynAck>
    ) -> Poll<AfterWaitForSynAck<'s>, Error> {
        unimplemented!()
    }

    fn poll_send_ack<'a>(
        state: &'a mut RentToOwn<'a, SendAck>
    ) -> Poll<AfterSendAck<'s>, Error> {
        unimplemented!()
    }

    fn poll_send_data<'a>(
        state: &'a mut RentToOwn<'a, SendData>
    ) -> Poll<AfterSendData<'s>, Error> {
        unimplemented!()
    }

    fn poll_wait_for_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForAck>
    ) -> Poll<AfterWaitForAck<'s>, Error> {
        unimplemented!()
    }

    fn poll_received_server_fin<'a>(
        state: &'a mut RentToOwn<'a, ReceivedServerFin>
    ) -> Poll<AfterReceivedServerFin<'s>, Error> {
        unimplemented!()
    }

    fn poll_send_fin_ack<'a>(
        state: &'a mut RentToOwn<'a, SendFinAck>
    ) -> Poll<AfterSendFinAck<'s>, Error> {
        unimplemented!()
    }

    fn poll_wait_for_last_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForLastAck>
    ) -> Poll<AfterWaitForLastAck, Error> {
        unimplemented!()
    }

    fn poll_send_fin<'a>(
        state: &'a mut RentToOwn<'a, SendFin>
    ) -> Poll<AfterSendFin<'s>, Error> {
        unimplemented!()
    }

    fn poll_wait_for_fin_ack<'a>(
        state: &'a mut RentToOwn<'a, WaitForFinAck>
    ) -> Poll<AfterWaitForFinAck<'s>, Error> {
        unimplemented!()
    }

    fn poll_send_last_ack<'a>(
        state: &'a mut RentToOwn<'a, SendLastAck>
    ) -> Poll<AfterSendLastAck, Error> {
        unimplemented!()
    }
}

pub struct StreamInitState<'a> {
    pub config: &'a Config,
    pub src: SocketAddrV6,
    pub dst: SocketAddrV6,
    pub sock: futures::IpV6RawSocketAdapter,
    pub data_source: StdinBytesReader<'a>
}
