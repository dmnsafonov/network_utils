use std::net::SocketAddrV6;

use ::linux_network::IPv6RawSocket;

use ::config::Config;

pub type InitState = (Config, SocketAddrV6, SocketAddrV6, IPv6RawSocket);
