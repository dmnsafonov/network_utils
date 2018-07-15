#![allow(bare_trait_objects)] // triggered by failure_derive

pub type Result<T> = ::std::result::Result<T, ::failure::Error>;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "ping6-data error")]
    CommonLibError(#[cause] ::ping6_datacommon::errors::Error),

    #[fail(display = "io error")]
    LinuxNetworkError(#[cause] ::linux_network::errors::Error),

    #[fail(display = "packet payload size {} is too big", size)]
    PayloadTooBig {
        size: usize
    },

    #[fail(display = "failed to spawn task on the thread pool")]
    SpawnError,

    #[fail(display = "operation timed out")]
    TimedOut,

    #[fail(display = "timer operation failed")]
    TimerError(#[cause] ::tokio_timer::Error),

    #[fail(display = "message of length {} expected, {} bytes read", exp, len)]
    WrongLengthMessage {
        len: usize,
        exp: usize
    }
}

impl From<::ping6_datacommon::errors::Error> for Error {
    fn from(err: ::ping6_datacommon::errors::Error) -> Error {
        Error::CommonLibError(err)
    }
}

impl From<::linux_network::errors::Error> for Error {
    fn from(err: ::linux_network::errors::Error) -> Error {
        Error::LinuxNetworkError(err)
    }
}

impl From<::tokio_timer::Error> for Error {
    fn from(err: ::tokio_timer::Error) -> Error {
        Error::TimerError(err)
    }
}
