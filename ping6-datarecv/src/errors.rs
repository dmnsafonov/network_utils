use ::std::io;

pub type Result<T> = ::std::result::Result<T, ::failure::Error>;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "io error")]
    IoError(#[cause] io::Error),

    #[fail(display = "io error")]
    LinuxNetworkError(#[cause] ::linux_network::errors::Error),

    #[fail(display = "receive buffer space depleted before completing \
        handshake")]
    RecvBufferOverrunOnStart,

    #[fail(display = "Received packet larger than the assumed MTU ({}).  \
                Consider specifying the interface to listen on.  \
                Default MTU is the safe guess of 1280.", packet_size)]
    MtuLessThanReal {
        packet_size: u16
    },

    #[fail(display = "failed to spawn task on the thread pool")]
    SpawnError,

    #[fail(display = "operation timed out")]
    TimedOut,

    #[fail(display = "timer operation failed")]
    TimerError(#[cause] ::tokio_timer::Error)
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
