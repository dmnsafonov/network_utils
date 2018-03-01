use ::std::net::Ipv6Addr;

use ::pnet_packet::icmpv6::*;
use ::pnet_packet::icmpv6::ndp::Icmpv6Codes;
use ::pnet_packet::*;

use ::ping6_datacommon::*;

pub struct StreamClientPacket<'a> {
    pub flags: StreamPacketFlagSet,
    pub seqno: u16,
    pub payload: &'a [u8]
}

pub fn parse_stream_client_packet<'a>(
    packet_buff: &'a [u8]
) -> StreamClientPacket<'a> {
    debug_assert!(validate_stream_packet(packet_buff, None));

    let packet = Icmpv6Packet::new(packet_buff)
        .expect("a valid length icmpv6 packet");

    let StreamClientPacket { flags, seqno, payload } =
        parse_stream_client_packet_payload(packet.payload());

    // satisfying the borrow checker
    let payload_ind = payload.as_ptr() as usize
        - packet_buff.as_ptr() as usize;
    StreamClientPacket {
        flags: flags,
        seqno: seqno,
        payload: &packet_buff[payload_ind..]
    }
}

pub fn parse_stream_client_packet_payload<'a>(
    payload: &'a [u8]
) -> StreamClientPacket<'a> {
    let flags = unsafe {
        StreamPacketFlagSet::from_num(payload[2])
    };

    StreamClientPacket {
        flags: flags,
        seqno: u16_from_bytes_be(&payload[4..6]),
        payload: &payload[6..]
    }
}

pub fn make_stream_server_icmpv6_packet<'a>(
    packet_buff: &'a mut [u8],
    src: Ipv6Addr,
    dst: Ipv6Addr,
    seqno_start: u16,
    seqno_end: u16,
    flags: StreamPacketFlagSet,
    payload: &[u8]
) -> Icmpv6Packet<'a> {
    let mut packet = MutableIcmpv6Packet::new(packet_buff)
        .expect("buffer big enough for the payload");
    debug_assert!(packet.payload().len()
        == STREAM_SERVER_HEADER_SIZE as usize + payload.len());

    packet.set_icmpv6_type(Icmpv6Types::EchoRequest);
    packet.set_icmpv6_code(Icmpv6Codes::NoCode);

    {
        let payload_buff = packet.payload_mut();
        payload_buff[2] = !0;
        payload_buff[3] = flags.get();
        payload_buff[4..6].copy_from_slice(&u16_to_bytes_be(seqno_start));
        payload_buff[6..8].copy_from_slice(&u16_to_bytes_be(seqno_end));
        payload_buff[8..].copy_from_slice(payload);
        let checksum = ping6_data_checksum(&payload_buff[2..]);
        payload_buff[0..2].copy_from_slice(&u16_to_bytes_be(checksum));
    }

    let cm = icmpv6::checksum(&packet.to_immutable(), src, dst);
    packet.set_checksum(cm);

    packet.consume_to_immutable()
}
