use ::std::net::Ipv6Addr;

use ::nom::*;

use ::linux_network::*;

use ::ipv6::*;
use ::macaddr::*;

#[derive(Clone, Debug)]
pub enum NdpPacket {
    NeighAdvert(NeighborAdvert),
    NeighSolicit(NeighborSolicit)
}

#[derive(Clone, Debug)]
pub struct IcmpV6Header {
    pub typ: IcmpV6Type,
    pub code: u8,
    pub checksum: u16
}

#[derive(Clone, Debug)]
pub struct NeighborAdvert {
    pub header: IcmpV6Header,
    pub router: bool,
    pub solicited: bool,
    pub overrid: bool,
    pub target: Ipv6Addr,
    pub options: Vec<IcmpV6Option>
}

#[derive(Clone, Debug)]
pub struct NeighborSolicit {
    pub header: IcmpV6Header,
    pub target: Ipv6Addr,
    pub options: Vec<IcmpV6Option>
}

named!(pub ndp_packet(&[u8]) -> NdpPacket, complete!(alt_complete!(
    map!(neighbor_advert, NdpPacket::NeighAdvert)
    | map!(neighbor_solicit, NdpPacket::NeighSolicit)
)));

named_args!(pub icmpv6_header(typ: IcmpV6Type)<IcmpV6Header>, do_parse!(
    tag!([typ.to_num()]) >>
    code: take!(1) >>
    checksum: be_u16 >>
    (IcmpV6Header {
        typ: typ,
        code: code[0],
        checksum: checksum
    })
));

named!(pub neighbor_advert(&[u8]) -> NeighborAdvert, do_parse!(
    header: apply!(icmpv6_header, IcmpV6Type::NdNeighborAdvert) >>
    flags: be_u8 >>
    _reserved: take!(3) >>
    target: ipv6_address >>
    options: alt!(
        value!(Vec::new(), eof!())
        | many0!(icmpv6_option)
    ) >>
    eof!() >>
    (NeighborAdvert {
        header: header,
        router: (flags & 128) != 0,
        solicited: (flags & 64) != 0,
        overrid: (flags & 32) != 0,
        target: target,
        options: options
    })
));

named!(pub neighbor_solicit(&[u8]) -> NeighborSolicit, do_parse!(
    header: apply!(icmpv6_header, IcmpV6Type::NdNeighborSolicit) >>
    _reserved: take!(4) >>
    target: ipv6_address >>
    options: alt!(
        value!(Vec::new(), eof!())
        | many0!(icmpv6_option)
    ) >>
    eof!() >>
    (NeighborSolicit {
        header: header,
        target: target,
        options: options
    })
));

const SRC_LL_ADDR: u8 = 1;
const TGT_LL_ADDR: u8 = 2;

// not exhaustive
gen_enum_arg!(pub IcmpV6Option: u8;
    (SRC_LL_ADDR => SrcLLAddr(MacAddr)),
    (TGT_LL_ADDR => TgtLLAddr(MacAddr))
);

named!(pub icmpv6_option(&[u8]) -> IcmpV6Option, do_parse!(
    typ: be_u8 >>
    ret: length_value!(map!(be_u8, |x| x * 8 - 2),
        switch!(value!(typ),
            SRC_LL_ADDR => map_res!(
                mac_addr,
                |x: MacAddr| IcmpV6Option::from_num(typ, &x)
            ) |
            TGT_LL_ADDR => map_res!(
                mac_addr,
                |x: MacAddr| IcmpV6Option::from_num(typ, &x)
            )
        )
    ) >>
    (ret)
));
