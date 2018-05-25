use ::std::net::Ipv6Addr;

use ::bytes::{*, BigEndian as BE};
use ::pnet_packet::icmpv6::*;
use ::pnet_packet::icmpv6::ndp::Icmpv6Codes;
use ::pnet_packet::*;

use ::ping6_datacommon::*;

pub struct StreamClientPacket<'a> {
    pub flags: StreamPacketFlags,
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
    let flags = StreamPacketFlags::from_bits(payload[3]).unwrap();

    StreamClientPacket {
        flags: flags,
        seqno: BE::read_u16(&payload[4..=5]),
        payload: &payload[6..]
    }
}

pub fn make_stream_server_icmpv6_packet(
    packet_buff: &mut BytesMut,
    src: Ipv6Addr,
    dst: Ipv6Addr,
    seqno_start: u16,
    seqno_end: u16,
    flags: StreamPacketFlags,
    payload: &[u8]
) -> Bytes {
    let targlen = STREAM_SERVER_FULL_HEADER_SIZE + payload.len();
    let buflen = packet_buff.len();
    if buflen < targlen {
        packet_buff.reserve(targlen - buflen);
        unsafe { packet_buff.advance_mut(targlen - buflen); }
    }

    {
        let mut packet = MutableIcmpv6Packet::new(packet_buff)
            .expect("buffer big enough for the payload");
        debug_assert!(packet.payload().len()
            >= STREAM_SERVER_HEADER_SIZE + payload.len());

        packet.set_icmpv6_type(Icmpv6Types::EchoRequest);
        packet.set_icmpv6_code(Icmpv6Codes::NoCode);

        {
            let payload_buff = packet.payload_mut();
            let mut buf = [0;2];

            payload_buff[2] = !0;
            payload_buff[3] = flags.bits();

            BE::write_u16(&mut buf, seqno_start);
            payload_buff[4..=5].copy_from_slice(&buf);

            BE::write_u16(&mut buf, seqno_end);
            payload_buff[6..=7].copy_from_slice(&buf);

            payload_buff[8..].copy_from_slice(payload);

            let checksum = ping6_data_checksum(&payload_buff[2..]);
            BE::write_u16(&mut buf, checksum);
            payload_buff[0..=1].copy_from_slice(&buf);
        }

        let cm = icmpv6::checksum(&packet.to_immutable(), &src, &dst);
        packet.set_checksum(cm);
    }

    packet_buff.split_to(targlen).freeze()
}
