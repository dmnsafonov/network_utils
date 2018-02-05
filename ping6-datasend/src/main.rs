#[macro_use] extern crate clap;
extern crate env_logger;
#[macro_use] extern crate enum_extract;
#[macro_use] extern crate enum_kinds_macros;
extern crate enum_kinds_traits;
#[macro_use] extern crate error_chain;
extern crate futures;
extern crate libc;
#[macro_use] extern crate log;
extern crate owning_ref;
extern crate pnet_packet;
extern crate seccomp;
extern crate tokio_core;

extern crate linux_network;
extern crate ping6_datacommon;

mod config;
mod errors;
mod stdin_iterator;

use std::net::*;
use std::os::unix::prelude::*;

use enum_kinds_traits::ToKind;
use pnet_packet::icmpv6;
use pnet_packet::icmpv6::*;
use pnet_packet::icmpv6::ndp::Icmpv6Codes;
use pnet_packet::Packet;

use linux_network::*;
use ping6_datacommon::*;

use config::*;
use errors::{ErrorKind, Result};
use stdin_iterator::*;

type InitState = (Config, SocketAddrV6, SocketAddrV6, IpV6RawSocket);

quick_main!(the_main);
fn the_main() -> Result<()> {
    let state = init()?;

    match state.0.mode.kind() {
        ModeConfigKind::Datagram => datagram_mode(state),
        ModeConfigKind::Stream => stream_mode(state)
    }
}

fn init() -> Result<InitState> {
    let config = get_config();

    env_logger::init()?;

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

    let src = make_socket_addr(&config.source, false)?;

    let dst = make_socket_addr(&config.destination, true)?;
    info!("resolved destination address: {}", dst);

    setup_signal_handler()?;

    let use_stdin = if let ModeConfig::Datagram(ref datagram_conf) = config.mode {
        datagram_conf.inline_messages.len() == 0
    } else {
        false
    };
    setup_seccomp(&sock, use_stdin)?;

    Ok((config, src, dst, sock))
}

fn setup_seccomp<T>(sock: &T, use_stdin: bool)
        -> Result<()> where T: SocketCommon {
    let mut ctx = allow_defaults()?;
    allow_console_out(&mut ctx, StdoutUse::No)?;
    if use_stdin {
        allow_console_in(&mut ctx)?;
    }
    sock.allow_sending(&mut ctx)?;
    ctx.load()?;
    Ok(())
}

fn datagram_mode((config, src, dst, mut sock): InitState) -> Result<()> {
    let datagram_conf = extract!(ModeConfig::Datagram(_), config.mode)
        .unwrap();

    let mut process_message = |i: &[u8]| -> Result<bool> {
        if signal_received() {
            info!("interrupted");
            return Ok(false);
        }

        let mut packet_descr = Icmpv6 {
            icmpv6_type: Icmpv6Types::EchoRequest,
            icmpv6_code: Icmpv6Codes::NoCode,
            checksum: 0,
            payload: vec![]
        };

        packet_descr.payload = match datagram_conf.raw {
            true => i.into(),
            false => form_checked_payload(i)?
        };

        let packet = make_packet(&packet_descr, *src.ip(), *dst.ip());
        match sock.sendto(packet.packet(), dst, SendFlagSet::new()) {
            Ok(_) => (),
            Err(e) => {
                if let Interrupted = *e.kind() {
                    info!("system call interrupted");
                    return Ok(true);
                } else {
                    return Err(e.into());
                }
            }
        }
        info!("message \"{}\" sent", String::from_utf8_lossy(i));

        Ok(true)
    };

    if datagram_conf.inline_messages.len() > 0 {
        for i in &datagram_conf.inline_messages {
            if !process_message(i.as_bytes())? {
                break;
            }
        }
    } else {
        for i in StdinBytesIterator::new() {
            if !process_message(&(*i?))? {
                break;
            }
        }
    }

    Ok(())
}

fn stream_mode((config, src, dst, mut sock): InitState) -> Result<()> {
    let _stream_conf = extract!(ModeConfig::Stream(_), config.mode).unwrap();

    unimplemented!()
}

fn form_checked_payload<T>(payload: T)
        -> Result<Vec<u8>> where T: AsRef<[u8]> {
    let b = payload.as_ref();
    let len = b.len();
    if len > std::u16::MAX as usize {
        bail!(ErrorKind::PayloadTooBig(len));
    }

    let checksum = ping6_data_checksum(b);

    let mut ret = Vec::with_capacity(len + 4);
    ret.extend_from_slice(&u16_to_bytes_be(checksum));
    ret.extend_from_slice(&u16_to_bytes_be(len as u16));
    ret.extend_from_slice(b);

    Ok(ret)
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
