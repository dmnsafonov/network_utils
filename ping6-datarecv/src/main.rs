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

use clap::*;
use pnet_packet::icmpv6::*;
use pnet_packet::Packet;

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

    let mut sock = IpV6RawSocket::new(
        ::libc::IPPROTO_ICMPV6,
        SockFlag::empty()
    )?;

    if let Some(addr) = matches.value_of("bind") {
        // TODO: support link-local addresses
        sock.bind(SocketAddrV6::new(
            Ipv6Addr::from_str(addr)?, 0, 0, 0)
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

    // TODO: drop privileges

    loop {
        let mut buf = [0; 1280];

        sock.recvfrom(&mut buf, RecvFlagSet::new())?;
        let packet = Icmpv6Packet::new(&buf).unwrap();

        if use_raw {
            println!("received message: {}",
                String::from_utf8_lossy(packet.payload()));
        } else {
            unimplemented!();
        }
    }
}
