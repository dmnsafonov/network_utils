#![allow(non_upper_case_globals)]

use ::pnet_packet::icmpv6::ndp::NeighborAdvertFlags::*;

pub const NEIGHBOR_ADVERT_SIZE: usize = 24;
pub const NEIGHBOR_ADVERT_LL_ADDR_OPTION_SIZE: usize = 8;

bitflags!(
    pub struct NdpAdvertFlags: u8 {
        const Router = Router;
        const Solicited = Solicited;
        const Override = Override;
    }
);
