error_chain!(
    errors {
        PayloadTooBig(size: usize) {
            description("packet payload is too big")
            display("packet payload size {} is too big", size)
        }

        SpawnError {
            description("failed to spawn task on the thread pool")
        }

        TimedOut {
            description("operation timed out")
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
        TimerError(::tokio_timer::Error);
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
