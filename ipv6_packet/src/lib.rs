#[macro_use] extern crate nom;

extern crate linux_network;
#[macro_use] extern crate numeric_enums;

pub mod ipv6;
pub mod macaddr;
pub mod ndp;

pub use ipv6::*;
pub use macaddr::*;
pub use ndp::*;

pub use nom::IResult;
