#[macro_use] extern crate clap;
extern crate env_logger;
#[macro_use] extern crate error_chain;
extern crate libc;
#[macro_use] extern crate log;
extern crate pnet_packet;

extern crate linux_network;

error_chain!(
    foreign_links {
        AddrParseError(std::net::AddrParseError);
        IoError(std::io::Error);
        LogInit(log::SetLoggerError);
    }

    links {
        LinuxNetwork (
            linux_network::errors::Error,
            linux_network::errors::ErrorKind
        );
    }
);

use std::net::*;
use std::ops::Add;
use std::str::FromStr;

use clap::*;
use pnet_packet::icmpv6;
use pnet_packet::icmpv6::*;
use pnet_packet::icmpv6::ndp::Icmpv6Codes;
use pnet_packet::Packet;

use linux_network::*;

quick_main!(the_main);
fn the_main() -> Result<()> {
    env_logger::init()?;

    let matches = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(Arg::with_name("raw")
            .long("raw")
            .short("r")
            .help("Forms raw packets without payload identification")
        ).arg(Arg::with_name("source")
            .required(true)
            .value_name("SOURCE_ADDRESS")
            .index(1)
            .help("Source address to use")
        ).arg(Arg::with_name("destination")
            .required(true)
            .value_name("DESTINATION")
            .index(2)
            .help("Messages destination")
        ).arg(Arg::with_name("messages")
            .required(true)
            .value_name("MESSAGES")
            .multiple(true)
            .index(3)
            .help("The messages to send, one argument for a packet")
        ).get_matches();

    let use_raw = matches.is_present("raw");

    // TODO: support link-local addresses
    let src_addr = Ipv6Addr::from_str(matches.value_of("source").unwrap())?;
    let src = SocketAddrV6::new(src_addr, 0, 0, 0);

    // TODO: support link-local addresses
    let dest_addr = matches
        .value_of("destination")
        .unwrap()
        .to_string()
        .add(":0")
        .to_socket_addrs()?
        .filter(SocketAddr::is_ipv6)
        .map(|x| match x {
            SocketAddr::V6(x) => x.ip().clone(),
            _ => unreachable!()
        }).next()
        .ok_or(ErrorKind::Msg("".to_string()))?;
    let dest = SocketAddrV6::new(dest_addr, 0, 0, 0);

    info!("resolved destination address: {}", dest);

    let mut sock = IpV6RawSocket::new(
        libc::IPPROTO_ICMPV6,
        SockFlag::empty()
    )?;
    sock.bind(&src)?;
    debug!("bound to address {}", src);

    // TODO: drop privileges

    for i in matches.values_of("messages").unwrap() {
        if use_raw {
            let packet_descr = Icmpv6 {
                icmpv6_type: Icmpv6Types::EchoRequest,
                icmpv6_code: Icmpv6Codes::NoCode,
                checksum: 0,
                payload: i.as_bytes().into()
            };

            let buf = vec![0; Icmpv6Packet::packet_size(&packet_descr)];
            let mut packet = MutableIcmpv6Packet::owned(buf).unwrap();
            packet.populate(&packet_descr);

            let cm = icmpv6::checksum(
                &packet.to_immutable(),
                src_addr,
                dest_addr
            );
            packet.set_checksum(cm);

            sock.sendto(packet.packet(), dest, SendFlagSet::new())?;
        } else {
            // TODO: make identified (with length + checksum) packet mode
            unimplemented!();
        }

        info!("message \"{}\" sent", i);
    }

    Ok(())
}
