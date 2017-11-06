#[macro_use] extern crate error_chain;
extern crate interfaces;
extern crate libc;
#[macro_use] extern crate log;
extern crate nix;
extern crate pnet_packet;

#[macro_use] extern crate numeric_enums;

#[macro_use] mod util;
#[macro_use] pub mod bpf;
pub mod epoll;
pub mod errors;
pub mod constants;
pub mod functions;
pub mod socket;
pub mod structs;

pub use self::bpf::*;
pub use self::epoll::*;
pub use self::constants::*;
pub use self::functions::*;
pub use self::socket::*;
pub use self::structs::*;

pub use numeric_enums::*;

pub use pnet_packet::ipv6::Ipv6;
pub use pnet_packet::icmpv6::{
    Icmpv6,
    Icmpv6Code,
    Icmpv6Type,
};
pub use pnet_packet::icmpv6::checksum as icmpv6_checksum;
