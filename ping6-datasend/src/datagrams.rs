use ::std::net::Ipv6Addr;
use ::std::os::unix::prelude::*;

use pnet_packet::icmpv6;
use ::pnet_packet::icmpv6::*;
use ::pnet_packet::icmpv6::ndp::Icmpv6Codes;
use ::pnet_packet::Packet;

use ::linux_network::*;
use ::ping6_datacommon::*;

use ::config::*;
use ::errors::{ErrorKind, Result};
use ::util::*;
use ::stdin_iterator::*;

pub fn datagram_mode((config, src, dst, mut sock): InitState) -> Result<()> {
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

fn form_checked_payload<T>(payload: T)
        -> Result<Vec<u8>> where T: AsRef<[u8]> {
    let b = payload.as_ref();
    let len = b.len();
    if len > ::std::u16::MAX as usize {
        bail!(ErrorKind::PayloadTooBig(len));
    }

    let checksum = ping6_data_checksum(b);

    let mut ret = Vec::with_capacity(len + 4);
    ret.extend_from_slice(&u16_to_bytes_be(checksum));
    ret.extend_from_slice(&u16_to_bytes_be(len as u16));
    ret.extend_from_slice(b);

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
