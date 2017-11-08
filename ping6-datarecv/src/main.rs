#[macro_use] extern crate clap;
extern crate env_logger;
#[macro_use] extern crate error_chain;
extern crate libc;
#[macro_use] extern crate log;

extern crate linux_network;

error_chain!(
    foreign_links {
        LogInit(::log::SetLoggerError);
    }

    links {
        LinuxNetwork (
            linux_network::errors::Error,
            linux_network::errors::ErrorKind
        );
    }
);

use clap::*;
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
            .short("b")
            .takes_value(true)
            .value_name("INTERFACE")
            .help("Bind to an interface")
        ).get_matches();

    let mut sock = IpV6RawSocket::new(
        ::libc::IPPROTO_ICMPV6,
        SockFlag::empty()
    )?;
    if let Some(ifname) = matches.value_of("bind") {
        sock.setsockopt(SockOptLevel::Socket, &SockOpt::BindToDevice(ifname))?;
        info!("bound to {} interface", ifname);
    }

    // TODO: drop privileges

    // TODO: actually, receive datagrams

    Ok(())
}
