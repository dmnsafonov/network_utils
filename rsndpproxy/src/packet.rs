use ::std::net::Ipv6Addr;

use ::bytes::*;
use ::pnet_packet::{*, ipv6::*};
use ::pnet_packet::icmpv6::{*, ndp::*};

use ::linux_network::MacAddr;

use ::config::*;
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

impl Advertisement {
    pub fn solicited_to_packet(
        &self,
        override_flag: Override,
        router_flag: Router
    ) -> Bytes {
        let size = NEIGHBOR_ADVERT_SIZE + NEIGHBOR_ADVERT_LL_ADDR_OPTION_SIZE;
        let mut icmp_bytes = BytesMut::with_capacity(size);

        {
            let mut buff = unsafe { icmp_bytes.bytes_mut() };

            {
                let mut icmp = MutableNeighborAdvertPacket::new(buff).unwrap();

                let mut flags = NdpAdvertFlags::Solicited;
                if let Override::Yes = override_flag {
                    flags |= NdpAdvertFlags::Override;
                }
                if let Router::Yes = router_flag {
                    flags |= NdpAdvertFlags::Router;
                }

                icmp.set_icmpv6_type(Icmpv6Types::NeighborAdvert);
                icmp.set_icmpv6_code(Icmpv6Codes::NoCode);
                icmp.set_flags(flags.bits());
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

            let mut icmp = MutableIcmpv6Packet::new(&mut buff).unwrap();
            let checksum = icmpv6::checksum(
                &icmp.to_immutable(),
                &self.src,
                &self.dst
            );
            icmp.set_checksum(checksum);
        }
        unsafe { icmp_bytes.advance_mut(size); }
        
        icmp_bytes.freeze()
    }
}

impl Solicitation {
    pub fn parse(packet: &Ipv6) -> Option<Self> {
        // validates only the points required
        // by https://tools.ietf.org/html/rfc4861#section-6.1.1

        let icmp_data = &packet.payload;
        let src = packet.source;
        let dst = packet.destination;

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
                || icmp_data.len() < 24
                || solicit.target_addr.is_multicast()
                || (src.is_unspecified()
                    || !is_solicited_node_multicast(&dst)) {
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

        Some(Self {
            src,
            dst,
            target: solicit.target_addr,
            ll_addr_opt
        })
    }
}
