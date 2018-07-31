use ::std::net::Ipv6Addr;
use ::std::os::unix::prelude::*;

use ::bytes::*;
use ::pnet_packet::icmpv6;
use ::pnet_packet::icmpv6::*;
use ::pnet_packet::icmpv6::ndp::Icmpv6Codes;
use ::pnet_packet::Packet;

use ::linux_network::*;
use ::ping6_datacommon::*;

use ::config::*;
use ::errors::{Error, Result};
use ::util::*;
use ::stdin::StdinBytesIterator;

pub fn datagram_mode((config, src, dst, mut sock): InitState) -> Result<()> {
    let datagram_conf = match config.mode {
        ModeConfig::Datagram(ref conf) => conf,
        _ => unreachable!()
    };

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

        packet_descr.payload = if datagram_conf.raw {
            i.into()
        } else {
            form_checked_payload(i)?
        };

        let packet = make_packet(&packet_descr, *src.ip(), *dst.ip());
        match sock.sendto(packet.packet(), dst, SendFlags::empty()) {
            Ok(_) => (),
            Err(e) => {
                let err = e.downcast_ref::<::linux_network::errors::Error>()
                    .map(|x| x.into());
                if let Some(Interrupted) = err {
                    info!("system call interrupted");
                    return Ok(true);
                } else {
                    return Err(e);
                }
            }
        }
        info!("message \"{}\" sent", String::from_utf8_lossy(i));

        Ok(true)
    };

    if !datagram_conf.inline_messages.is_empty() {
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

fn form_checked_payload<T>(payload: T)
        -> Result<Vec<u8>> where T: AsRef<[u8]> {
    let b = payload.as_ref();
    let len = b.len();
    if len > ::std::u16::MAX as usize {
        return Err(Error::PayloadTooBig {
            size: len
        }.into());
    }

    let checksum = ping6_data_checksum(b);

    let mut ret = Vec::with_capacity(len + 4);
    ret.put_u16_be(checksum);
    ret.put_u16_be(len as u16);
    ret.put(b);

    Ok(ret)
}

pub fn make_packet(descr: &Icmpv6, src: Ipv6Addr, dst: Ipv6Addr)
        -> Icmpv6Packet {
    let buf = vec![0; Icmpv6Packet::packet_size(&descr)];
    let mut packet = MutableIcmpv6Packet::owned(buf).unwrap();
    packet.populate(&descr);

    let cm = icmpv6::checksum(
        &packet.to_immutable(),
        &src,
        &dst
    );
    packet.set_checksum(cm);

    packet.consume_to_immutable()
}
