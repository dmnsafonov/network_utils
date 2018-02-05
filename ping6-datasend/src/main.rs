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

extern crate linux_network;
extern crate ping6_datacommon;

error_chain!(
    errors {
        PayloadTooBig(size: usize) {
            description("packet payload is too big")
            display("packet payload size {} is too big", size)
        }

        WrongLength(len: usize, exp: usize) {
            description("message is smaller than the length specified")
            display("message of length {} expected, {} bytes read", exp, len)
        }
    }

    foreign_links {
        AddrParseError(std::net::AddrParseError);
        IoError(std::io::Error);
        LogInit(log::SetLoggerError);
        Seccomp(seccomp::SeccompError);
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

use std::ffi::*;
use std::io;
use std::io::prelude::*;
use std::net::*;
use std::os::unix::prelude::*;

use clap::*;
use enum_kinds_traits::ToKind;
use futures::prelude::*;
use owning_ref::*;
use pnet_packet::icmpv6;
use pnet_packet::icmpv6::*;
use pnet_packet::icmpv6::ndp::Icmpv6Codes;
use pnet_packet::Packet;

use linux_network::*;
use ping6_datacommon::*;

struct Config {
    source: String,
    destination: String,
    bind_interface: Option<String>,
    mode: ModeConfig
}

#[derive(EnumKind)]
#[enum_kind_name(ModeConfigKind)]
enum ModeConfig {
    Datagram(DatagramConfig),
    Stream(StreamConfig)
}

struct DatagramConfig {
    raw: bool,
    inline_messages: Vec<OsString>
}

struct StreamConfig;

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

fn get_config() -> Config {
    let matches = get_args();

    let messages = match matches.values_of_os("messages") {
        Some(messages) => messages.map(OsStr::to_os_string).collect(),
        None => Vec::new()
    };

    Config {
        source: matches.value_of("source").unwrap().to_string(),
        destination: matches.value_of("destination").unwrap().to_string(),
        bind_interface: matches.value_of("bind-to-interface")
            .map(str::to_string),
        mode: if matches.is_present("stream") {
                ModeConfig::Stream(StreamConfig)
            } else {
                ModeConfig::Datagram(DatagramConfig {
                    raw: matches.is_present("raw"),
                    inline_messages: messages
                })
            }
    }
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
            .conflicts_with("stream")
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
            .conflicts_with("use-stdin")
            .value_name("MESSAGES")
            .multiple(true)
            .index(3)
            .help("The messages to send, one argument for a packet")
        ).arg(Arg::with_name("bind-to-interface")
            .short("I")
            .long("bind-to-interface")
            .takes_value(true)
            .value_name("INTERFACE")
            .help("Binds to an interface")
        ).arg(Arg::with_name("use-stdin")
            .required(true)
            .conflicts_with("messages")
            .long("use-stdin")
            .short("c")
            .help("Instead of messages on the command-line, read from stdin \
                (prepend each message with 16-bit BE length)")
        ).arg(Arg::with_name("stream")
            .long("stream")
            .short("s")
            .help("Sets stream mode on: messages are to be read as \
                a continuous stream from stdin")
            .requires("use-stdin")
        ).get_matches()
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

struct StdinBytesIterator<'a> {
    tin: io::StdinLock<'a>,
    tin_glue: Box<io::Stdin>
}

impl<'a> StdinBytesIterator<'a> {
    fn new() -> StdinBytesIterator<'a> {
        let glue = Box::new(io::stdin());
        StdinBytesIterator {
            tin: unsafe { (glue.as_ref() as *const io::Stdin).as_ref().unwrap().lock() },
            tin_glue: glue,
        }
    }
}

impl<'a> Iterator for StdinBytesIterator<'a> {
    type Item = Result<OwningRef<Vec<u8>, [u8]>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut len_buf = [0; 2];
        match self.tin.read(&mut len_buf) {
            Ok(0) => return None,
            Err(e) => return Some(Err(e.into())),
            _ => ()
        };
        let len = ((len_buf[0] as usize) << 8) | (len_buf[1] as usize);

        let mut buf = vec![0; std::u16::MAX as usize];
        match self.tin.read(&mut buf[..len]) {
            Ok(x) if x == len => (),
            Ok(x) => return Some(Err(ErrorKind::WrongLength(x, len).into())),
            Err(e) => return Some(Err(e.into()))
        };

        let ret = VecRef::new(buf).map(|v| &v[..len]);
        Some(Ok(ret))
    }
}

impl<'a> AsRawFd for StdinBytesIterator<'a> {
    fn as_raw_fd(&self) -> RawFd {
        io::stdin().as_raw_fd()
    }
}

struct StdinBytesFuture<'a> {
    iter: &'a mut StdinBytesIterator<'a>,
    pending: bool,
    drop_nonblock: bool
}

impl<'a> StdinBytesFuture<'a> {
    fn new(iter: &'a mut StdinBytesIterator<'a>)
            -> Result<StdinBytesFuture<'a>> {
        let old = set_fd_nonblock(iter, true)?;
        Ok(StdinBytesFuture {
            iter: iter,
            pending: true,
            drop_nonblock: !old
        })
    }
}

impl<'a> Drop for StdinBytesFuture<'a> {
    fn drop(&mut self) {
        if self.drop_nonblock {
            set_fd_nonblock(self.iter, false).unwrap();
        }
    }
}

impl<'a> Future for StdinBytesFuture<'a> {
    type Item = OwningRef<Vec<u8>, [u8]>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        assert!(self.pending);
        let res = self.iter.next().unwrap_or(Ok(OwningRef::new(Vec::new())));
        match res {
            Err(Error(ErrorKind::IoError(e), magic)) => {
                if let io::ErrorKind::WouldBlock = e.kind() {
                    Ok(Async::NotReady)
                } else {
                    bail!(Error(ErrorKind::IoError(e), magic))
                }
            },
            Err(e) => Err(e),
            Ok(x) => Ok(Async::Ready(x))
        }
    }
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
