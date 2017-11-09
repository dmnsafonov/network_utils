#[macro_use] extern crate clap;
extern crate crc16;
extern crate env_logger;
#[macro_use] extern crate error_chain;
extern crate libc;
#[macro_use] extern crate log;
extern crate pnet_packet;

extern crate linux_network;
extern crate ping6_datacommon;

error_chain!(
    foreign_links {
        AddrParseError(std::net::AddrParseError);
        LogInit(::log::SetLoggerError);
    }

    links {
        LinuxNetwork (
            linux_network::errors::Error,
            linux_network::errors::ErrorKind
        );
        Ping6DataCommon (
            ping6_datacommon::Error,
            ping6_datacommon::ErrorKind
        );
    }
);

use std::net::*;
use std::str::FromStr;

use clap::*;
use pnet_packet::icmpv6;
use pnet_packet::icmpv6::*;
use pnet_packet::icmpv6::ndp::Icmpv6Codes;
use pnet_packet::{FromPacket, Packet, PrimitiveValues};

use linux_network::*;
use ping6_datacommon::*;

// TODO: add correct signal handling
quick_main!(the_main);
fn the_main() -> Result<()> {
    env_logger::init()?;

    let matches = get_args();

    gain_net_raw()?;
    let mut sock = IpV6RawSocket::new(
        ::libc::IPPROTO_ICMPV6,
        SockFlag::empty()
    )?;
    debug!("raw socket created");

    drop_caps()?;
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
    debug!("set icmpv6 type filter");

    loop {
        let mut buf = [0; 65535]; // mtu unlikely to be higher

        let (buf, sockaddr) = sock.recvfrom(&mut buf, RecvFlagSet::new())?;
        let src = *sockaddr.ip();
        let packet = Icmpv6Packet::new(&buf).unwrap();
        let payload = packet.payload();

        debug!("received packet, length = {} from {}", payload.len(), src);

        if !validate_icmpv6(&packet, src, bound_addr) {
            continue;
        }

        if matches.is_present("raw") {
            println!("received message from {}: {}",
                src,
                String::from_utf8_lossy(payload));
        } else {
            if validate_payload(payload) {
                println!("received message from {}: {}",
                    src,
                    String::from_utf8_lossy(&payload[4..])
                );
            }
        }
    }
}

fn get_args<'a>() -> ArgMatches<'a> {
    App::new(crate_name!())
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
        ).get_matches()
}

fn validate_icmpv6(
        packet: &Icmpv6Packet,
        src: Ipv6Addr,
        dst: Option<Ipv6Addr>) -> bool {
    let icmp = packet.from_packet();
    assert_eq!(icmp.icmpv6_type, Icmpv6Types::EchoRequest);

    if let Some(dest_addr) = dst {
        let cm = icmpv6::checksum(&packet, src, dest_addr);
        if icmp.checksum != cm {
            info!("wrong icmp checksum {}, correct is {}, dropping",
                icmp.checksum,
                cm
            );
            return false;
        }
    }

    if icmp.icmpv6_code != Icmpv6Codes::NoCode {
        info!("nonzero code {} in echo request, dropping",
            icmp.icmpv6_code.to_primitive_values().0
        );
        return false;
    }

    return true;
}

fn validate_payload<T>(payload_arg: T) -> bool where T: AsRef<[u8]> {
    let payload = payload_arg.as_ref();

    let len = ((payload[0] as u16) << 8) | (payload[1] as u16);
    let packet_crc = ((payload[2] as u16) << 8) | (payload[3] as u16);

    if len != (payload.len() - 4) as u16 {
        debug!("wrong encapsulated packet length: {}, dropping", len);
        return false;
    }

    let crc = ping6_data_checksum(&payload[4..]);

    if packet_crc != crc {
        debug!("wrong crc, dropping");
        return false;
    }

    return true;
}
