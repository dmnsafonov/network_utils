error_chain!(
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
