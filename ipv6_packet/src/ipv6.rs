use ::std::net::Ipv6Addr;

use ::nom::*;

named!(pub ipv6_address(&[u8]) -> Ipv6Addr, do_parse!(
    a: be_u16 >>
    b: be_u16 >>
    c: be_u16 >>
    d: be_u16 >>
    e: be_u16 >>
    f: be_u16 >>
    g: be_u16 >>
    h: be_u16 >>
    (Ipv6Addr::new(a, b, c, d, e, f, g, h))
));
