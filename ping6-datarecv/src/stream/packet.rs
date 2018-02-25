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
    let payload = packet.payload();

    let flags = unsafe {
        StreamPacketFlagSet::from_num(payload[2])
    };

    // satisfying the borrow checker
    let payload_ind = (&payload[5..6]).as_ptr() as usize + 1
        - packet_buff.as_ptr() as usize;

    StreamClientPacket {
        flags: flags,
        seqno: u16_from_bytes_be(&payload[5..7]),
        payload: &packet_buff[payload_ind..]
    }
}
