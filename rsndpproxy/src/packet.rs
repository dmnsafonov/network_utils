use ::std::net::Ipv6Addr;

use ::pnet_packet::*;
use ::pnet_packet::ipv6::*;
use ::pnet_packet::icmpv6;
use ::pnet_packet::icmpv6::ndp::*;

use ::linux_network::MacAddr;

use ::util::is_solicited_node_multicast;

pub struct Solicitation {
    pub src: Ipv6Addr,
    pub dst: Ipv6Addr,
    pub target: Ipv6Addr,
    pub ll_addr_opt: Option<MacAddr>
}

// WRONG: parse whole IPv6 packet
pub fn parse_solicitation(data: impl AsRef<[u8]>) -> Option<Solicitation> {
    let packet_opt = Ipv6Packet::new(data.as_ref());
    let (icmp_data, src, dst) =
        if packet_opt.is_some() {
            let packet = packet_opt.as_ref().unwrap();
            if packet.get_hop_limit() != 255 {
                 return None;
            }
            (packet.payload(), packet.get_source(), packet.get_destination())
        } else {
            return None;
        };

    let solicit = match NeighborSolicitPacket::new(icmp_data.as_ref()) {
        Some(packet) => packet.from_packet(),
        None => return None
    };

    let checksum = {
        let packet = icmpv6::Icmpv6Packet::new(icmp_data.as_ref())
            .expect("a valid icmpv6 packet");
        icmpv6::checksum(&packet, &src, &dst)
    };

    if solicit.icmpv6_type != icmpv6::Icmpv6Types::NeighborSolicit
            || solicit.icmpv6_code != Icmpv6Codes::NoCode
            || solicit.checksum != checksum
            || solicit.payload.len() < 24
            || solicit.target_addr.is_multicast()
            || (!src.is_unspecified() || is_solicited_node_multicast(&dst)) {
        return None;
    }

    let mut ll_addr_opt = None;
    for i in solicit.options {
        if i.option_type == NdpOptionType::new(2) {
            if ll_addr_opt.is_some()
                    || i.length != 1
                    || i.data.len() != 6 {
                return None;
            }
            ll_addr_opt = Some(MacAddr::from_bytes(i.data).unwrap())
        }
    }

    if src.is_unspecified() && ll_addr_opt.is_some() {
        return None;
    }

    Some(Solicitation {
        src,
        dst,
        target: solicit.target_addr,
        ll_addr_opt
    })
}
