use ::std::net::Ipv6Addr;

use ::pnet_packet::ip::IpNextHeaderProtocols;
use ::pnet_packet::{*, ipv6::*, icmpv6};
use ::pnet_packet::icmpv6::{*, ndp::*};

use ::linux_network::MacAddr;

use ::constants::*;
use ::util::is_solicited_node_multicast;

#[derive(Debug)]
pub struct Advertisement {
    pub src: Ipv6Addr,
    pub dst: Ipv6Addr,
    pub target: Ipv6Addr,
    pub ll_addr_opt: Option<MacAddr>
}

#[derive(Debug)]
pub struct Solicitation {
    pub src: Ipv6Addr,
    pub dst: Ipv6Addr,
    pub target: Ipv6Addr,
    pub ll_addr_opt: Option<MacAddr>
}

gen_boolean_enum!(pub Override);

impl Advertisement {
    pub fn solicited_to_ipv6(&self, override_flag: Override) -> Ipv6 {
        let mut icmp_buff = vec![0; NEIGHBOR_ADVERT_SIZE
            + NEIGHBOR_ADVERT_LL_ADDR_OPTION_SIZE];
        {
            let mut icmp = MutableNeighborAdvertPacket::new(&mut icmp_buff)
                .unwrap();

            icmp.set_icmpv6_type(Icmpv6Types::NeighborAdvert);
            icmp.set_icmpv6_code(Icmpv6Codes::NoCode);
            icmp.set_flags(match override_flag {
                Override::Yes => NdpAdvertFlags::Override,
                Override::No => NdpAdvertFlags::empty()
            }.bits());
            icmp.set_target_addr(self.target);
            match self.ll_addr_opt {
                Some(ref mac) => icmp.set_options(&[NdpOption {
                    option_type: NdpOptionTypes::TargetLLAddr,
                    length: 1,
                    data: mac.as_bytes().to_vec()
                }]),
                None => icmp.set_options(&[])
            }
            icmp.set_payload(&[]);
        }
        {
            let mut icmp = MutableIcmpv6Packet::new(&mut icmp_buff).unwrap();
            let checksum = icmpv6::checksum(
                &icmp.to_immutable(),
                &self.src,
                &self.dst
            );
            icmp.set_checksum(checksum);
        }

        Ipv6 {
            version: 6,
            traffic_class: 0,
            flow_label: 0,
            payload_length: (IPV6_HEADER_SIZE + NEIGHBOR_ADVERT_SIZE
                + NEIGHBOR_ADVERT_LL_ADDR_OPTION_SIZE) as u16,
            next_header: IpNextHeaderProtocols::Icmpv6,
            hop_limit: 255,
            source: self.src,
            destination: self.dst,
            payload: icmp_buff
        }
    }
}

impl Solicitation {
    pub fn parse(data: impl AsRef<[u8]>) -> Option<Solicitation> {
        let packet_opt = Ipv6Packet::new(data.as_ref());
        let (icmp_data, src, dst) =
            if packet_opt.is_some() {
                let packet = packet_opt.as_ref().unwrap();
                if packet.get_hop_limit() != 255 {
                     return None;
                }
                (
                    packet.payload(),
                    packet.get_source(),
                    packet.get_destination()
                )
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
                || (!src.is_unspecified()
                    || is_solicited_node_multicast(&dst)) {
            return None;
        }

        let mut ll_addr_opt = None;
        for i in solicit.options {
            if i.option_type == NdpOptionTypes::SourceLLAddr {
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
}
