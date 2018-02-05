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
