use ::std::net::Ipv6Addr;

use ::pnet_packet::icmpv6::*;
use ::pnet_packet::icmpv6::ndp::Icmpv6Codes;
use ::pnet_packet::*;

use ::ping6_datacommon::*;

use ::stream::constants::*;

pub struct StreamServerPacket<'a> {
    pub flags: StreamPacketFlagSet,
    pub seqno_start: u16,
    pub seqno_end: u16,
    pub payload: &'a [u8]
}

pub fn make_stream_client_icmpv6_packet<'a>(
    packet_buff: &'a mut [u8],
    src: Ipv6Addr,
    dst: Ipv6Addr,
    seqno: u16,
    flags: StreamPacketFlagSet,
    payload: &[u8]
) -> Icmpv6Packet<'a> {
    let mut packet = MutableIcmpv6Packet::new(packet_buff)
        .expect("buffer big enough for the payload");
    debug_assert!(packet.payload().len()
        == STREAM_CLIENT_HEADER_SIZE as usize + payload.len());

    packet.set_icmpv6_type(Icmpv6Types::EchoRequest);
    packet.set_icmpv6_code(Icmpv6Codes::NoCode);

    {
        let payload_buff = packet.payload_mut();
        payload_buff[2] = flags.get();
        payload_buff[3] = 0;
        payload_buff[4..6].copy_from_slice(&u16_to_bytes_be(seqno));
        payload_buff[6..].copy_from_slice(payload);
        let checksum = ping6_data_checksum(&payload_buff[2..]);
        payload_buff[0..2].copy_from_slice(&u16_to_bytes_be(checksum));
    }

    let cm = icmpv6::checksum(&packet.to_immutable(), src, dst);
    packet.set_checksum(cm);

    packet.consume_to_immutable()
}

pub fn parse_stream_packet<'a>(
    packet_buff: &'a [u8],
    addrs: Option<(Ipv6Addr,Ipv6Addr)>
) -> Option<StreamServerPacket<'a>> {
    let packet = Icmpv6Packet::new(packet_buff)
        .expect("a valid length icmpv6 packet");

    if packet.get_icmpv6_type() != Icmpv6Types::EchoReply
            || packet.get_icmpv6_code() != Icmpv6Codes::NoCode {
        return None;
    }

    if let Some((src,dst)) = addrs {
        if packet.get_checksum()
            != icmpv6::checksum(&packet, src, dst) {
            return None;
        }
    }

    let payload = packet.payload();
    let checksum = u16_from_bytes_be(&payload[0..2]);

    if checksum != ping6_data_checksum(&payload[2..]) {
        return None;
    }

    let flags = unsafe {
        let x = payload[3];
        if x & ALL_STREAM_PACKET_FLAGS != 0 {
            return None;
        }
        StreamPacketFlagSet::from_num(x)
    };

    if payload[2] != !0 {
        return None;
    }

    // satisfying the borrow checker
    let payload_ind = (&payload[8..9]).as_ptr() as usize
        - packet_buff.as_ptr() as usize;

    Some(StreamServerPacket {
        flags: flags,
        seqno_start: u16_from_bytes_be(&payload[4..6]),
        seqno_end: u16_from_bytes_be(&payload[6..8]),
        payload: &packet_buff[payload_ind..]
    })
}
