use ::std::net::Ipv6Addr;

use ::bytes::{*, BigEndian as BE};
use ::pnet_packet::icmpv6::*;
use ::pnet_packet::icmpv6::ndp::Icmpv6Codes;
use ::pnet_packet::*;

use ::ping6_datacommon::*;

pub struct StreamServerPacket<'a> {
    pub flags: StreamPacketFlags,
    pub seqno_start: u16,
    pub seqno_end: u16,
    pub payload: &'a [u8]
}

pub fn make_stream_client_icmpv6_packet(
    packet_buff: &mut BytesMut,
    src: Ipv6Addr,
    dst: Ipv6Addr,
    seqno: u16,
    flags: StreamPacketFlags,
    payload: &[u8]
) -> Bytes {
    debug_assert!(!flags.contains(StreamPacketFlags::WS));

    let targlen = STREAM_CLIENT_FULL_HEADER_SIZE + payload.len();
    let buflen = packet_buff.len();
    if buflen < targlen {
        packet_buff.reserve(targlen - buflen);
        unsafe { packet_buff.advance_mut(targlen - buflen); }
    }

    {
        let mut packet = MutableIcmpv6Packet::new(packet_buff)
            .expect("buffer big enough for the payload");
        debug_assert!(packet.payload().len()
            >= STREAM_CLIENT_HEADER_SIZE + payload.len());

        packet.set_icmpv6_type(Icmpv6Types::EchoRequest);
        packet.set_icmpv6_code(Icmpv6Codes::NoCode);

        {
            let payload_buff = packet.payload_mut();
            let mut buf = [0;2];

            payload_buff[2] = !0;
            payload_buff[3] = flags.bits();

            BE::write_u16(&mut buf, seqno);
            payload_buff[4..=5].copy_from_slice(&buf);

            payload_buff[6..].copy_from_slice(payload);

            let checksum = ping6_data_checksum(&payload_buff[2..]);
            BE::write_u16(&mut buf, checksum);
            payload_buff[0..=1].copy_from_slice(&buf);
        }

        let cm = icmpv6::checksum(&packet.to_immutable(), &src, &dst);
        packet.set_checksum(cm);
    }

    packet_buff.split_to(targlen).freeze()
}

pub fn parse_stream_server_packet<'a>(
    packet_buff: &'a [u8]
) -> StreamServerPacket<'a> {
    debug_assert!(validate_stream_packet(packet_buff, None));

    let packet = Icmpv6Packet::new(packet_buff)
        .expect("a valid length icmpv6 packet");
    let payload = packet.payload();

    let flags = StreamPacketFlags::from_bits(payload[3]).unwrap();

    // satisfying the borrow checker
    let payload_ind = (&payload[7..=7]).as_ptr() as usize + 1
        - packet_buff.as_ptr() as usize;

    StreamServerPacket {
        flags: flags,
        seqno_start: BE::read_u16(&payload[4..=5]),
        seqno_end: BE::read_u16(&payload[6..=7]),
        payload: &packet_buff[payload_ind..]
    }
}
