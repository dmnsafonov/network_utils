use ::std::net::Ipv6Addr;

use ::pnet_packet::icmpv6::*;
use ::pnet_packet::icmpv6::ndp::Icmpv6Codes;
use ::pnet_packet::*;

use ::numeric_enums::*;
use ::ping6_datacommon::*;

use ::stream::constants::*;

pub fn make_stream_packet<'a>(
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
        == HEADER_SIZE as usize + payload.len());

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
