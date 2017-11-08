#[macro_use] extern crate clap;
extern crate env_logger;
#[macro_use] extern crate error_chain;
extern crate libc;
#[macro_use] extern crate log;

extern crate linux_network;

error_chain!(
    foreign_links {
        IoError(std::io::Error);
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

use clap::*;
use linux_network::*;

quick_main!(the_main);
fn the_main() -> Result<()> {
    env_logger::init();

    let matches = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(Arg::with_name("destination")
            .required(true)
            .value_name("DESTINATION")
            .index(1)
            .help("Messages destination")
        )
        .arg(Arg::with_name("messages")
            .required(true)
            .value_name("MESSAGES")
            .multiple(true)
            .index(2)
            .help("The messages to send, one argument for a packet")
        ).get_matches();

    let dest = matches
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

    info!("resolved destination address: {}", dest);

    let mut sock = IpV6RawSocket::new(
        libc::IPPROTO_ICMPV6,
        SockFlag::empty()
    )?;

    // TODO: drop privileges

    for i in matches.values_of("messages").unwrap() {
        // TODO: form the packet, then send it
        // make identified (with length + checksum) and bare packet modes
        info!("message \"{}\" sent", i);
    }

    Ok(())
}
