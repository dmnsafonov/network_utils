error_chain!(
    errors {
        RecvBufferOverrunOnStart {
            description("receive buffer space depleted before completing \
                handshake")
        }

        MtuLessThanReal(packet_size: u16) {
            description("received packet greater then the assumed mtu")
            display("Received packet larger than the assumed MTU ({}).  \
                Consider specifying the interface to listen on.  \
                Default MTU is the safe guess of 1280.", packet_size)
        }

        TimedOut {
            description("operation timed out")
        }
    }

    foreign_links {
        AddrParseError(::std::net::AddrParseError);
        IoError(::std::io::Error);
        LogInit(::log::SetLoggerError);
        Seccomp(::seccomp::SeccompError);
        TimerError(::tokio_timer::TimerError);
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
