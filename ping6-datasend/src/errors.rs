use ::futures::MapErr;
use ::tokio_timer::TimeoutError;

use ::linux_network::futures::IpV6RawSocketRecvfromFuture;

error_chain!(
    errors {
        PayloadTooBig(size: usize) {
            description("packet payload is too big")
            display("packet payload size {} is too big", size)
        }

        TimedOut {
            description("operation timed out")
        }

        TimerError {
            description("timer operation failed")
        }

        WrongLength(len: usize, exp: usize) {
            description("message is smaller than the length specified")
            display("message of length {} expected, {} bytes read", exp, len)
        }
    }

    foreign_links {
        AddrParseError(::std::net::AddrParseError);
        IoError(::std::io::Error);
        LogInit(::log::SetLoggerError);
        Seccomp(::seccomp::SeccompError);
    }

    links {
        LinuxNetwork (
            ::linux_network::errors::Error,
            ::linux_network::errors::ErrorKind
        );
        Ping6DataCommon (
            ::ping6_datacommon::Error,
            ::ping6_datacommon::ErrorKind
        );
    }
);

impl<F> From<TimeoutError<MapErr<IpV6RawSocketRecvfromFuture, F>>>
        for Error where F: Fn(::linux_network::errors::Error) -> Error {
    fn from(x: TimeoutError<MapErr<IpV6RawSocketRecvfromFuture, F>>)
            -> Error {
        match x {
            TimeoutError::Timer(_, _) => ErrorKind::TimerError.into(),
            TimeoutError::TimedOut(_) => ErrorKind::TimedOut.into()
        }
    }
}
