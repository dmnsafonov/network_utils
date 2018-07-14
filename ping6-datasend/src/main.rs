#![allow(unknown_lints)]
#![warn(bare_trait_objects)]
#![warn(clippy)]

extern crate bytes;
#[macro_use] extern crate clap;
extern crate env_logger;
#[macro_use] extern crate enum_extract;
#[macro_use] extern crate enum_kinds_macros;
extern crate enum_kinds_traits;
#[macro_use] extern crate failure;
#[macro_use] extern crate futures;
extern crate libc;
#[macro_use] extern crate log;
extern crate mio;
extern crate owning_ref;
extern crate pnet_packet;
extern crate rand;
extern crate seccomp;
extern crate send_box;
#[macro_use] extern crate state_machine_future;
extern crate tokio;
extern crate tokio_timer;

#[macro_use] extern crate boolean_enums;
extern crate linux_network;
extern crate ping6_datacommon;

mod config;
mod datagrams;
mod errors;
mod stdin_iterator;
mod stream;
mod util;

use enum_kinds_traits::ToKind;

use linux_network::*;
use ping6_datacommon::*;

use config::*;
use datagrams::datagram_mode;
use errors::Result;
use stream::stream_mode;
use util::InitState;

fn main() {
    if let Err(e) = the_main() {
        let mut first = true;;
        for i in e.causes() {
            if !first {
                eprint!(": ");
            }
            eprint!("{}", i);
            first = false;
        }
        eprintln!("");
    }
}

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
    let mut sock = IPv6RawSocket::new(
        IpProto::IcmpV6.bits(),
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

    let mut filter = icmp6_filter::new();
    filter.pass(IcmpV6Type::EchoRequest);
    sock.setsockopt(&SockOpts::IcmpV6Filter::new(&filter))?;
    debug!("set icmpv6 type filter");

    sock.setsockopt(&SockOpts::V6MtuDiscover::new(&V6PmtuType::Do))?;

    let src = make_socket_addr(&config.source, Resolve::No)?;

    let dst = make_socket_addr(&config.destination, Resolve::Yes)?;
    info!("resolved destination address: {}", dst);

    setup_signal_handler()?;

    let use_stdin = if let ModeConfig::Datagram(ref datagram_conf) = config.mode {
        datagram_conf.inline_messages.is_empty()
    } else {
        false
    };
    setup_seccomp(&sock, use_stdin.into(),
        (config.mode.kind() == ModeConfigKind::Stream).into())?;

    Ok((config, src, dst, sock))
}

gen_boolean_enum!(StdinUse);

fn setup_seccomp<T>(
    sock: &T,
    use_stdin: StdinUse,
    use_stream_mode: UseStreamMode
) -> Result<()> where T: SocketCommon {
    // tokio libs syscall use is not documented
    if use_stream_mode.into() {
        return Ok(())
    }

    let mut ctx = allow_defaults()?;
    allow_console_out(&mut ctx, StdoutUse::No)?;
    if use_stdin.into() {
        allow_console_in(&mut ctx)?;
    }
    sock.allow_sending(&mut ctx)?;
    ctx.load()?;
    Ok(())
}
