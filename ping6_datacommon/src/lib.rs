extern crate capabilities;
extern crate crc16;
#[macro_use] extern crate error_chain;
#[macro_use] extern crate log;

error_chain!(
    errors {
        Priv {
            description("privilege operation error")
        }
    }

    foreign_links {
        IoError(std::io::Error);
    }
);

use std::net::*;

use capabilities::*;

// TODO: support link-local addresses
pub fn make_socket_addr(addr: Ipv6Addr) -> SocketAddrV6 {
    SocketAddrV6::new(addr, 0, 0, 0)
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
