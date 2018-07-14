use ::std::io;
use ::std::io::prelude::*;
use ::std::net::Ipv6Addr;

use ::pnet_packet::icmpv6;
use ::pnet_packet::icmpv6::*;
use ::pnet_packet::icmpv6::ndp::Icmpv6Codes;
use ::pnet_packet::*;

use ::linux_network::IPv6RawSocket;

use ::config::Config;
use ::errors::Result;

pub type InitState = (Config, Option<Ipv6Addr>, IPv6RawSocket);

pub fn validate_icmpv6(
        packet: &Icmpv6Packet,
        src: Ipv6Addr,
        dst: Option<Ipv6Addr>) -> bool {
    let icmp = packet.from_packet();
    assert_eq!(icmp.icmpv6_type, Icmpv6Types::EchoRequest);

    if let Some(dest_addr) = dst {
        let cm = icmpv6::checksum(&packet, &src, &dest_addr);
        if icmp.checksum != cm {
            info!("wrong icmp checksum {}, correct is {}",
                icmp.checksum,
                cm
            );
            return false;
        }
    }

    if icmp.icmpv6_code != Icmpv6Codes::NoCode {
        info!("nonzero code {} in echo request",
            icmp.icmpv6_code.to_primitive_values().0
        );
        return false;
    }

    true
}

pub fn write_binary(out: &mut io::StdoutLock, len: &[u8], payload: &[u8])
        -> Result<()> {
    out.write_all(len)?;
    out.write_all(payload)?;
    Ok(())
}
