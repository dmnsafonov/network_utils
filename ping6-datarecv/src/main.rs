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
        IoError(std::io::Error);
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

use std::io::prelude::*;
use std::io::stdout;
use std::net::*;

use clap::*;
use pnet_packet::icmpv6;
use pnet_packet::icmpv6::*;
use pnet_packet::icmpv6::ndp::Icmpv6Codes;
use pnet_packet::{FromPacket, Packet, PrimitiveValues};

use linux_network::*;
use ping6_datacommon::*;

quick_main!(the_main);
fn the_main() -> Result<()> {
    env_logger::init()?;

    let matches = get_args();
    let raw = matches.is_present("raw");
    let binary = matches.is_present("binary");

    gain_net_raw()?;
    let mut sock = IpV6RawSocket::new(
        ::libc::IPPROTO_ICMPV6,
        SockFlag::empty()
    )?;
    debug!("raw socket created");

    drop_caps()?;
    set_no_new_privs()?;
    debug!("PR_SET_NO_NEW_PRIVS set");

    let bound_addr_str = matches.value_of("bind");
    let bound_sockaddr = option_map_result(bound_addr_str,
        |x| make_socket_addr(x, false))?;
    let bound_addr = bound_sockaddr.map(|x| *x.ip());

    if let Some(addr) = bound_sockaddr {
        sock.bind(addr)?;
        info!("bound to {} address", bound_addr_str.unwrap());
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

    setup_signal_handler()?;

    loop {
        if signal_received() {
            info!("interrupted");
            break;
        }

        // ipv6 payload length is 2-byte
        let mut buf = vec![0; std::u16::MAX as usize];

        use linux_network::errors::ErrorKind::Interrupted;
        let (buf, sockaddr) =
            match sock.recvfrom(&mut buf, RecvFlagSet::new()) {
                x@Ok(_) => x,
                Err(e) => {
                    if let Interrupted = *e.kind() {
                        info!("interrupted");
                        break;
                    } else {
                        Err(e)
                    }
                }
            }?;
        let src = *sockaddr.ip();
        let packet = Icmpv6Packet::new(&buf).unwrap();
        let payload = packet.payload();

        debug!("received packet, payload size = {} from {}",
            payload.len(), src);

        if !validate_icmpv6(&packet, src, bound_addr) {
            continue;
        }

        let payload_for_print;
        if raw {
            if binary {
                write_binary(
                    &u16_to_bytes_be(payload.len() as u16),
                    payload
                )?;
            }

            payload_for_print = Some(payload);
        } else if validate_payload(payload) {
            if binary {
                write_binary(&payload[0..2], &payload[4..])?;
            }

            payload_for_print = Some(&payload[4..]);
        } else {
            payload_for_print = None;
        }

        if let Some(payload_for_print) = payload_for_print {
            let str_payload = String::from_utf8_lossy(payload_for_print);
            if binary {
                stdout().flush()?;
                info!("received message from {}: {}", src, str_payload);
            } else {
                println!("received message from {}: {}", src, str_payload);
            }
        }
    }

    Ok(())
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
            .help("Binds to an address")
        ).arg(Arg::with_name("bind-to-interface")
            .long("bind-to-interface")
            .short("I")
            .takes_value(true)
            .value_name("INTERFACE")
            .help("Binds to an interface")
        ).arg(Arg::with_name("raw")
            .long("raw")
            .short("r")
            .help("Shows all received packets' payload")
        ).arg(Arg::with_name("binary")
            .long("binary")
            .short("B")
            .help("Outputs only the messages' contents, preceded by \
                2-byte-BE length; otherwise messages are converted to \
                unicode, filtering any non-unicode data")
        ).get_matches()
}

pub fn option_map_result<T,F,R,E>(x: Option<T>, f: F)
        -> ::std::result::Result<Option<R>, E> where
        F: FnOnce(T) -> ::std::result::Result<R,E> {
    match x {
        Some(y) => f(y).map(Some),
        None => Ok(None)
    }
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

fn write_binary(len: &[u8], payload: &[u8]) -> Result<()> {
    let mut out = stdout();
    out.write(len)?;
    out.write(payload)?;
    Ok(())
}
