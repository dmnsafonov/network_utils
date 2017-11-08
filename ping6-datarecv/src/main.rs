extern crate capabilities;
#[macro_use] extern crate clap;
extern crate crc16;
extern crate env_logger;
#[macro_use] extern crate error_chain;
extern crate libc;
#[macro_use] extern crate log;
extern crate pnet_packet;

extern crate linux_network;

error_chain!(
    errors {
        Priv {
            description("privilege operation error")
        }
    }

    foreign_links {
        AddrParseError(std::net::AddrParseError);
        IoError(std::io::Error);
        LogInit(::log::SetLoggerError);
    }

    links {
        LinuxNetwork (
            linux_network::errors::Error,
            linux_network::errors::ErrorKind
        );
    }
);

use std::net::*;
use std::str::FromStr;

use capabilities::*;
use clap::*;
use pnet_packet::icmpv6;
use pnet_packet::icmpv6::*;
use pnet_packet::icmpv6::ndp::Icmpv6Codes;
use pnet_packet::{FromPacket, Packet, PrimitiveValues};

use linux_network::*;

quick_main!(the_main);
fn the_main() -> Result<()> {
    env_logger::init()?;

    let matches = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(Arg::with_name("bind")
            .long("bind")
            .short("-b")
            .takes_value(true)
            .value_name("ADDRESS")
            .help("Bind to an address")
        ).arg(Arg::with_name("bind-to-interface")
            .long("bind-to-interface")
            .short("I")
            .takes_value(true)
            .value_name("INTERFACE")
            .help("Bind to an interface")
        ).arg(Arg::with_name("raw")
            .long("raw")
            .short("r")
            .help("Show all received packets' payload")
        ).get_matches();

    let use_raw = matches.is_present("raw");

    let err = || ErrorKind::Priv;
    let mut caps = Capabilities::from_current_proc()
        .chain_err(&err)?;
    if !caps.update(&[Capability::CAP_NET_RAW], Flag::Effective, true) {
        bail!(err());
    }
    caps.apply().chain_err(&err)?;
    debug!("gained CAP_NET_RAW");

    let mut sock = IpV6RawSocket::new(
        ::libc::IPPROTO_ICMPV6,
        SockFlag::empty()
    )?;
    debug!("raw socket created");

    caps.reset_all();
    caps.apply().chain_err(err)?;
    debug!("dropped all capabilities");

    set_no_new_privs()?;
    debug!("PR_SET_NO_NEW_PRIVS set");

    let bound_addr = match matches.value_of("bind") {
        Some(x) => Some(Ipv6Addr::from_str(x)?),
        None => None
    };

    if let Some(addr) = bound_addr {
        // TODO: support link-local addresses
        sock.bind(SocketAddrV6::new(
            addr, 0, 0, 0)
        )?;
        info!("bound to {} address", addr);
    }
    if let Some(ifname) = matches.value_of("bind-to-interface") {
        sock.setsockopt(
            SockOptLevel::Socket,
            &SockOpt::BindToDevice(ifname)
        )?;
        info!("bound to {} interface", ifname);
    }

    let mut filter = icmp6_filter::new();
    filter.pass(IcmpV6Type::EchoRequest);
    sock.setsockopt(SockOptLevel::IcmpV6, &SockOpt::IcmpV6Filter(&filter))?;

    loop {
        let mut buf = [0; 65535]; // mtu unlikely to be higher

        let (buf, sockaddr) = sock.recvfrom(&mut buf, RecvFlagSet::new())?;
        let addr = sockaddr.ip();
        let packet = Icmpv6Packet::new(&buf).unwrap();
        let payload = packet.payload();

        debug!("received packet, length = {} from {}", payload.len(), addr);

        let icmp = packet.from_packet();
        assert_eq!(icmp.icmpv6_type, Icmpv6Types::EchoRequest);

        if let Some(dest_addr) = bound_addr {
            let cm = icmpv6::checksum(&packet, *addr, dest_addr);
            if icmp.checksum != cm {
                info!("wrong icmp checksum {}, correct is {}, dropping",
                    icmp.checksum,
                    cm
                );
                continue;
            }
        }

        if icmp.icmpv6_code != Icmpv6Codes::NoCode {
            info!("nonzero code {} in echo request, dropping",
                icmp.icmpv6_code.to_primitive_values().0
            );
            continue;
        }

        if use_raw {
            println!("received message from {}: {}",
                addr,
                String::from_utf8_lossy(packet.payload()));
        } else {
            let len = ((payload[0] as u16) << 8) | (payload[1] as u16);
            let packet_crc = ((payload[2] as u16) << 8) | (payload[3] as u16);

            if len != (payload.len() - 4) as u16 {
                debug!("wrong encapsulated packet length: {}, dropping", len);
                continue;
            }

            let mut crc_st = crc16::State::<crc16::CCITT_FALSE>::new();
            crc_st.update(&payload[0..2]);
            crc_st.update(&payload[4..]);
            let crc = crc_st.get();

            if packet_crc != crc {
                debug!("wrong crc, dropping");
                continue;
            }

            println!("received message from {}: {}",
                addr,
                String::from_utf8_lossy(&payload[4..])
            );
        }
    }
}
