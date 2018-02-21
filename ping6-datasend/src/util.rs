use std::net::SocketAddrV6;

use ::linux_network::IpV6RawSocket;

use ::config::Config;

pub type InitState = (Config, SocketAddrV6, SocketAddrV6, IpV6RawSocket);
