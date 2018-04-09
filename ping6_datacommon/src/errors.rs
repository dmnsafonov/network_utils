error_chain!(
    errors {
        Priv {
            description("privilege operation error (is cap_net_raw+p not set \
                on the executable?)")
        }
    }

    foreign_links {
        IoError(::std::io::Error);
        NixError(::nix::Error);
        Seccomp(::seccomp::SeccompError);
        TimerError(::tokio_timer::Error);
    }

    links {
        LinuxNetwork (
            ::linux_network::errors::Error,
            ::linux_network::errors::ErrorKind
        );
    }
);
