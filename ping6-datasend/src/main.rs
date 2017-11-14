#[macro_use] extern crate clap;
extern crate env_logger;
#[macro_use] extern crate error_chain;
extern crate libc;
#[macro_use] extern crate log;
extern crate pnet_packet;

extern crate linux_network;
extern crate ping6_datacommon;

error_chain!(
    errors {
        PayloadTooBig(size: usize) {
            description("packet payload is too big")
            display("packet payload size {} is too big", size)
        }
    }

    foreign_links {
        AddrParseError(std::net::AddrParseError);
        IoError(std::io::Error);
        LogInit(log::SetLoggerError);
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

use std::net::*;
use std::os::unix::prelude::*;

use clap::*;
use pnet_packet::icmpv6;
use pnet_packet::icmpv6::*;
use pnet_packet::icmpv6::ndp::Icmpv6Codes;
use pnet_packet::Packet;

use linux_network::*;
use ping6_datacommon::*;

quick_main!(the_main);
fn the_main() -> Result<()> {
    env_logger::init()?;

    let matches = get_args();
    let raw = matches.is_present("raw");

    let src = make_socket_addr(matches.value_of("source").unwrap(), false)?;
    let src_addr = *src.ip();

    let dst = make_socket_addr(
        matches.value_of("destination").unwrap(),
        true
    )?;
    let dst_addr = *dst.ip();
    info!("resolved destination address: {}", dst);

    gain_net_raw()?;
    let mut sock = IpV6RawSocket::new(
        libc::IPPROTO_ICMPV6,
        SockFlag::empty()
    )?;
    debug!("raw socket created");

    drop_caps()?;
    set_no_new_privs()?;
    debug!("PR_SET_NO_NEW_PRIVS set");

    sock.bind(src)?;
    debug!("bound to address {}", src);

    setup_signal_handler()?;

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

        packet_descr.payload = match raw {
            true => i.into(),
            false => checked_payload(i)?
        };

        let packet = make_packet(&packet_descr, src_addr, dst_addr);
        match sock.sendto(packet.packet(), dst, SendFlagSet::new()) {
            Ok(_) => (),
            Err(e) => {
                if let Interrupted = *e.kind() {
                    info!("interrupted");
                    return Ok(false);
                } else {
                    return Err(e.into());
                }
            }
        }
        info!("message \"{}\" sent", String::from_utf8_lossy(i));

        Ok(true)
    };

    if matches.is_present("use-stdin") {
        for i in StdinBytesIterator::new() {
            if !process_message(i?)? {
                break;
            }
        }
    } else {
        for i in matches.values_of_os("messages").unwrap() {
            if !process_message(i.as_bytes())? {
                break;
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
        .arg(Arg::with_name("raw")
            .long("raw")
            .short("r")
            .help("Forms raw packets without payload identification")
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
        ).arg(Arg::with_name("use-stdin")
            .required(true)
            .conflicts_with("messages")
            .long("use-stdin")
            .short("c")
            .help("Instead of messages on the command-line, read from stdin \
                (each successful read is a message)")
        ).get_matches()
}

struct StdinBytesIterator<'a> {
    buf: Vec<u8>,
    _phantom: std::marker::PhantomData<&'a [u8]>
}

impl<'a> StdinBytesIterator<'a> {
    fn new() -> StdinBytesIterator<'a> {
        // maximum ipv6 payload length
        let size = std::u16::MAX as usize;
        let mut buf = Vec::with_capacity(size);
        buf.resize(size, 0);
        StdinBytesIterator {
            buf: buf,
            _phantom: Default::default()
        }
    }
}

impl<'a> Iterator for StdinBytesIterator<'a> {
    type Item = Result<&'a [u8]>;

    fn next(&mut self) -> Option<Self::Item> {
        use std::io::prelude::*;
        use std::io;

        let mut tin = io::stdin();

        let len = match tin.read(&mut self.buf) {
            Ok(0) => return None,
            Ok(x) => x,
            Err(e) => return Some(Err(e.into()))
        };

        let ret = unsafe { std::slice::from_raw_parts(
            self.buf.as_ptr(),
            len
        ) };
        Some(Ok(ret))
    }
}

fn checked_payload<T>(payload: T) -> Result<Vec<u8>> where T: AsRef<[u8]> {
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
