error_chain!(
    errors {
        AlreadyRunning(filename: String) {
            description("another instance already running")
            display("another instance already running: failed locking {}",
                filename)
        }

        PrivDrop {
            description("privilege dropping error")
        }

        FileIo(name: String) {
            description("file io error")
            display("error accessing file: {}", name)
        }

        NoInterface(name: String) {
            description("cannot find specified network interface")
            display("cannot find network interface {}", name)
        }

        NoMac(name: String) {
            description("cannot get the mac address")
            display("cannot get the mac address of the interface {}", name)
        }
    }

    foreign_links {
        ConfigParseError(::toml::de::Error);
        ConfigSerializeError(::toml::ser::Error);
        NixError(::nix::Error);
        InvalidIpv6Prefix(::ipnetwork::IpNetworkError);
        LogInit(::log::SetLoggerError);
        SyslogInit(::syslog::SyslogError);
    }

    links {
        LinuxNetwork (
            ::linux_network::errors::Error,
            ::linux_network::errors::ErrorKind
        );
    }
);
