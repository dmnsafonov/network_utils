use std::net::*;

use pnet_packet::icmpv6;
use pnet_packet::icmpv6::*;

use ::linux_network::IpV6RawSocket;

use ::config::Config;

pub type InitState = (Config, SocketAddrV6, SocketAddrV6, IpV6RawSocket);

pub fn make_packet(descr: &Icmpv6, src: Ipv6Addr, dst: Ipv6Addr)
        -> Icmpv6Packet {
    let buf = vec![0; Icmpv6Packet::packet_size(&descr)];
    let mut packet = MutableIcmpv6Packet::owned(buf).unwrap();
    packet.populate(&descr);

    let cm = icmpv6::checksum(
        &packet.to_immutable(),
        src,
        dst
    );
    packet.set_checksum(cm);

    packet.consume_to_immutable()
}
