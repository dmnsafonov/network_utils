#[macro_use] extern crate clap;
extern crate env_logger;
#[macro_use] extern crate enum_extract;
#[macro_use] extern crate enum_kinds_macros;
extern crate enum_kinds_traits;
#[macro_use] extern crate error_chain;
extern crate libc;
#[macro_use] extern crate log;
extern crate pnet_packet;
extern crate seccomp;

extern crate linux_network;
extern crate ping6_datacommon;

mod config;
mod errors;

use std::io::prelude::*;
use std::io::stdout;
use std::net::*;

use enum_kinds_traits::ToKind;
use pnet_packet::icmpv6;
use pnet_packet::icmpv6::*;
use pnet_packet::icmpv6::ndp::Icmpv6Codes;
use pnet_packet::{FromPacket, Packet, PrimitiveValues};

use linux_network::*;
use ping6_datacommon::*;

use config::*;
use errors::Result;

type InitState = (Config, Option<Ipv6Addr>, IpV6RawSocket);

quick_main!(the_main);
fn the_main() -> Result<()> {
    let state = init()?;

    match state.0.mode.kind() {
        ModeConfigKind::Datagram => datagram_mode(state),
        ModeConfigKind::Stream => stream_mode(state)
    }
}

fn init() -> Result<InitState> {
    env_logger::init()?;

    let config = get_config();

    gain_net_raw()?;
    let mut sock = IpV6RawSocket::new(
        IpProto::IcmpV6.to_num(),
        SockFlag::empty()
    )?;
    debug!("raw socket created");

    if let Some(ref ifname) = config.bind_interface {
        sock.setsockopt(&SockOpts::BindToDevice::new(&ifname))?;
        info!("bound to {} interface", ifname);
    }

    drop_caps()?;
    set_no_new_privs()?;
    debug!("PR_SET_NO_NEW_PRIVS set");

    let bound_addr = if let Some(ref addr) = config.bind_address {
        let bound_sockaddr = make_socket_addr(addr, Resolve::Yes)?;
        sock.bind(bound_sockaddr)?;
        info!("bound to {} address", addr);

        Some(*bound_sockaddr.ip())
    } else {
        None
    };

    let mut filter = icmp6_filter::new();
    filter.pass(IcmpV6Type::EchoRequest);
    sock.setsockopt(&SockOpts::IcmpV6Filter::new(&filter))?;
    debug!("set icmpv6 type filter");

    setup_signal_handler()?;

    setup_seccomp(&sock, StdoutUse::Yes)?;

    Ok((config, bound_addr, sock))
}

fn setup_seccomp<T>(sock: &T, use_stdout: StdoutUse)
        -> Result<()> where T: SocketCommon {
    let mut ctx = allow_defaults()?;
    allow_console_out(&mut ctx, use_stdout)?;
    sock.allow_receiving(&mut ctx)?;
    ctx.load()?;
    Ok(())
}

fn datagram_mode((config, bound_addr, mut sock): InitState) -> Result<()> {
    let datagram_conf = extract!(ModeConfig::Datagram(_), config.mode)
        .unwrap();

    // ipv6 payload length is 2-byte
    let mut raw_buf = vec![0; std::u16::MAX as usize];
    loop {
        if signal_received() {
            info!("interrupted");
            break;
        }

        let (buf, sockaddr) =
            match sock.recvfrom(&mut raw_buf, RecvFlagSet::new()) {
                x@Ok(_) => x,
                Err(e) => {
                    if let Interrupted = *e.kind() {
                        debug!("system call interrupted");
                        continue;
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
            info!("invalid icmpv6 packet, dropping");
            continue;
        }

        if datagram_conf.binary {
            binary_print(payload, src, datagram_conf.raw)?;
        } else {
            regular_print(payload, src, datagram_conf.raw)?;
        }
    }

    Ok(())
}

fn stream_mode((config, bound_addr, mut sock): InitState) -> Result<()> {
    let stream_conf = extract!(ModeConfig::Stream(_), config.mode).unwrap();

    unimplemented!()
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
            info!("wrong icmp checksum {}, correct is {}",
                icmp.checksum,
                cm
            );
            return false;
        }
    }

    if icmp.icmpv6_code != Icmpv6Codes::NoCode {
        info!("nonzero code {} in echo request",
            icmp.icmpv6_code.to_primitive_values().0
        );
        return false;
    }

    return true;
}

fn validate_payload<T>(payload_arg: T) -> bool where T: AsRef<[u8]> {
    let payload = payload_arg.as_ref();

    let packet_checksum = ((payload[0] as u16) << 8) | (payload[1] as u16);
    let len = ((payload[2] as u16) << 8) | (payload[3] as u16);

    if len != (payload.len() - 4) as u16 {
        debug!("wrong encapsulated packet length: {}, dropping", len);
        return false;
    }

    let checksum = ping6_data_checksum(&payload[4..]);

    if packet_checksum != checksum {
        debug!("wrong checksum, dropping");
        return false;
    }

    return true;
}

fn binary_print(payload: &[u8], src: Ipv6Addr, raw: bool) -> Result<()> {
    let payload_for_print;
    if raw {
        write_binary(
            &u16_to_bytes_be(payload.len() as u16),
            payload
        )?;
        payload_for_print = Some(payload);
    } else if validate_payload(payload) {
        let real_payload = &payload[4..];
        write_binary(&payload[0..2], real_payload)?;
        payload_for_print = Some(real_payload);
    } else {
        payload_for_print = None;
    }

    if let Some(payload_for_print) = payload_for_print {
        let str_payload = String::from_utf8_lossy(payload_for_print);
        info!("received message from {}: {}", src, str_payload);
        stdout().flush()?;
    }

    Ok(())
}

fn write_binary(len: &[u8], payload: &[u8]) -> Result<()> {
    let mut out = stdout();
    out.write(len)?;
    out.write(payload)?;
    Ok(())
}

fn regular_print(payload: &[u8], src: Ipv6Addr, raw: bool) -> Result<()> {
    let payload_for_print = match raw {
        true => Some(payload),
        false => {
            match validate_payload(payload) {
                true => Some(&payload[4..]),
                false => None
            }
        }
    };
    if let Some(payload_for_print) = payload_for_print {
        let str_payload = String::from_utf8_lossy(payload_for_print);
        println!("received message from {}: {}", src, str_payload);
    }
    Ok(())
}
