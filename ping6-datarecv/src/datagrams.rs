use ::std::io;
use ::std::io::prelude::*;
use ::std::net::Ipv6Addr;

use ::bytes::{*, BigEndian as BE};
use ::pnet_packet::icmpv6::*;
use ::pnet_packet::Packet;

use ::linux_network::*;
use ::ping6_datacommon::*;

use ::config::*;
use ::errors::Result;
use ::util::*;

pub fn datagram_mode((config, bound_addr, mut sock): InitState) -> Result<()> {
    let datagram_conf = extract!(ModeConfig::Datagram(_), config.mode)
        .unwrap();

    // ipv6 payload length is 2-byte
    let mut raw_buf = vec![0; ::std::u16::MAX as usize];
    let mut stdout_locked = match datagram_conf.binary {
        true => Some(movable_io_lock(io::stdout())),
        false => None
    };
    loop {
        if signal_received() {
            info!("interrupted");
            break;
        }

        let (buf, sockaddr) =
            match sock.recvfrom(&mut raw_buf, RecvFlags::empty()) {
                x@Ok(_) => x,
                Err(e) => {
                    let err_opt =
                        e.downcast_ref::<::linux_network::errors::Error>()
                            .map(|x| x.kind());
                    if let Some(Interrupted) = err_opt {
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
            binary_print(stdout_locked.as_mut().unwrap(), payload, src,
                datagram_conf.raw.into())?;
        } else {
            regular_print(payload, src, datagram_conf.raw.into())?;
        }
    }

    Ok(())
}

fn validate_payload<T>(payload_arg: T) -> bool where T: AsRef<[u8]> {
    let payload = payload_arg.as_ref();

    let packet_checksum = BE::read_u16(&payload[0..=1]);
    let len = BE::read_u16(&payload[2..=3]);

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

fn binary_print(
    out: &mut io::StdoutLock,
    payload: &[u8],
    src: Ipv6Addr,
    raw: Raw
) -> Result<()> {
    let payload_for_print;
    let mut buf = [0;2];
    BE::write_u16(&mut buf, payload.len() as u16);
    if raw.into() {
        write_binary(
            out,
            &buf,
            payload
        )?;
        payload_for_print = Some(payload);
    } else if validate_payload(payload) {
        let real_payload = &payload[4..];
        write_binary(out, &payload[0..=1], real_payload)?;
        payload_for_print = Some(real_payload);
    } else {
        payload_for_print = None;
    }

    if let Some(payload_for_print) = payload_for_print {
        let str_payload = String::from_utf8_lossy(payload_for_print);
        info!("received message from {}: {}", src, str_payload);
        io::stdout().flush()?;
    }

    Ok(())
}

gen_boolean_enum!(Raw);

fn regular_print(payload: &[u8], src: Ipv6Addr, raw: Raw) -> Result<()> {
    let payload_for_print = match raw {
        Raw::Yes => Some(payload),
        Raw::No => {
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
