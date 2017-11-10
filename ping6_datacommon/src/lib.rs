extern crate capabilities;
extern crate crc16;
#[macro_use] extern crate error_chain;
extern crate libc;
#[macro_use] extern crate log;
extern crate nix;

extern crate linux_network;

error_chain!(
    errors {
        Priv {
            description("privilege operation error (is cap_net_raw+p not set \
                on the executable?)")
        }
    }

    foreign_links {
        IoError(std::io::Error);
        NixError(nix::Error);
    }

    links {
        LinuxNetwork (
            linux_network::errors::Error,
            linux_network::errors::ErrorKind
        );
    }
);

use std::net::*;
use std::sync::atomic::*;

use capabilities::*;
use libc::{c_int, IPPROTO_ICMPV6};
use nix::sys::signal::*;

use linux_network::*;

pub fn make_socket_addr<T>(addr_str: T, resolve: bool) -> Result<SocketAddrV6>
        where T: AsRef<str> {
    let sockaddr_in = make_sockaddr_in6_v6_dgram(
        addr_str,
        None,
        IPPROTO_ICMPV6,
        match resolve {
            true => AddrInfoFlagSet::new(),
            false => AddrInfoFlags::NumericHost.into()
        }
    )?;

    Ok(SocketAddrV6::new(
        sockaddr_in.sin6_addr.s6_addr.into(),
        0,
        0,
        sockaddr_in.sin6_scope_id
    ))
}

pub fn option_map_result<T,F,R,E>(x: Option<T>, f: F)
        -> ::std::result::Result<Option<R>, E> where
        F: FnOnce(T) -> ::std::result::Result<R,E> {
    match x {
        Some(y) => f(y).map(Some),
        None => Ok(None)
    }
}

pub fn gain_net_raw() -> Result<()> {
    let err = || ErrorKind::Priv;
    let mut caps = Capabilities::from_current_proc()
        .chain_err(&err)?;
    if !caps.update(&[Capability::CAP_NET_RAW], Flag::Effective, true) {
        bail!(err());
    }
    caps.apply().chain_err(err)?;
    debug!("gained CAP_NET_RAW");
    Ok(())
}

pub fn drop_caps() -> Result<()> {
    Capabilities::new()?
        .apply()
        .chain_err(|| ErrorKind::Priv)?;
    debug!("dropped all capabilities");
    Ok(())
}

pub fn ping6_data_checksum<T>(payload: T) -> u16 where T: AsRef<[u8]> {
    let b = payload.as_ref();
    let len = b.len();

    let mut crc_st = crc16::State::<crc16::CCITT_FALSE>::new();
    crc_st.update(&[
        ((len & 0xff00) >> 8) as u8,
        (len & 0xff) as u8
    ]);
    crc_st.update(b);

    crc_st.get()
}

const SIGNAL_FLAG: AtomicBool = ATOMIC_BOOL_INIT;

pub fn setup_signal_handler() -> Result<()> {
    let sigact = SigAction::new(
        SigHandler::Handler(signal_handler),
        SaFlags::empty(),
        SigSet::empty()
    );

    unsafe {
        sigaction(Signal::SIGINT, &sigact)?;
        sigaction(Signal::SIGQUIT, &sigact)?;
        sigaction(Signal::SIGTERM, &sigact)?;
    }

    debug!("set up signal handlers");
    Ok(())
}

extern "C" fn signal_handler(_: c_int) {
    SIGNAL_FLAG.store(true, Ordering::Relaxed);
}

pub fn signal_received() -> bool {
    SIGNAL_FLAG.swap(false, Ordering::Relaxed)
}
