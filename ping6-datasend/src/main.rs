#[macro_use] extern crate clap;
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
        LogInit(log::SetLoggerError);
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
use pnet_packet::Packet;

use linux_network::*;
use ping6_datacommon::*;

// TODO: add correct signal handling
quick_main!(the_main);
fn the_main() -> Result<()> {
    env_logger::init()?;

    let matches = get_args();

    let src_addr = Ipv6Addr::from_str(matches.value_of("source").unwrap())?;
    let src = make_socket_addr(src_addr);

    let dst_addr = Ipv6Addr::from_str(matches.value_of("destination")
        .unwrap())?;
    let dst = make_socket_addr(dst_addr);
    info!("resolved destination address: {}", dst);

    gain_net_raw()?;
    let mut sock = IpV6RawSocket::new(
        libc::IPPROTO_ICMPV6,
        SockFlag::empty()
    )?;
    debug!("raw socket created");

    drop_caps()?;
    set_no_new_privs()?;
    debug!("PR_SET_NO_NEW_PRIVS set");

    sock.bind(src)?;
    debug!("bound to address {}", src);

    for i in matches.values_of("messages").unwrap() {
        let b = i.as_bytes();

        let mut packet_descr = Icmpv6 {
            icmpv6_type: Icmpv6Types::EchoRequest,
            icmpv6_code: Icmpv6Codes::NoCode,
            checksum: 0,
            payload: vec![]
        };

        packet_descr.payload = match matches.is_present("raw") {
            true => b.into(),
            false => checked_payload(b)
        };

        let packet = make_packet(&packet_descr, src_addr, dst_addr);
        sock.sendto(packet.packet(), dst, SendFlagSet::new())?;
        info!("message \"{}\" sent", i);
    }

    Ok(())
}

fn get_args<'a>() -> ArgMatches<'a> {
    App::new(crate_name!())
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
        ).get_matches()
}

fn checked_payload<T>(payload: T) -> Vec<u8> where T: AsRef<[u8]> {
    let b = payload.as_ref();
    let len = b.len();
    assert!(len <= 65535); // max mtu on linux

    let crc = ping6_data_checksum(b);

    let mut ret = Vec::with_capacity(len + 4);
    ret.push(((len & 0xff00) >> 8) as u8);
    ret.push((len & 0xff) as u8);
    ret.push(((crc & 0xff00) >> 8) as u8);
    ret.push((crc & 0xff) as u8);
    ret.extend_from_slice(b);

    ret
}

fn make_packet(descr: &Icmpv6, src: Ipv6Addr, dst: Ipv6Addr) -> Icmpv6Packet {
    let buf = vec![0; Icmpv6Packet::packet_size(&descr)];
    let mut packet = MutableIcmpv6Packet::owned(buf).unwrap();
    packet.populate(&descr);

    let cm = icmpv6::checksum(
        &packet.to_immutable(),
        src,
        dst
    );
    packet.set_checksum(cm);

    packet.consume_to_immutable()
}
