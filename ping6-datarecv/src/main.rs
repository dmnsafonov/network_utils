#[macro_use] extern crate clap;
extern crate env_logger;
#[macro_use] extern crate enum_extract;
#[macro_use] extern crate enum_kinds_macros;
extern crate enum_kinds_traits;
#[macro_use] extern crate error_chain;
#[macro_use] extern crate futures;
extern crate libc;
#[macro_use] extern crate log;
extern crate mio;
extern crate owning_ref;
extern crate pnet_packet;
extern crate seccomp;
extern crate send_box;
#[macro_use] extern crate state_machine_future;
extern crate tokio;
extern crate tokio_timer;

#[macro_use] extern crate boolean_enums;
#[macro_use] extern crate linux_network;
extern crate ping6_datacommon;
extern crate sliceable_rcref;

mod config;
mod datagrams;
mod errors;
mod stdout_iterator;
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
