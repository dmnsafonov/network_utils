#![allow(unknown_lints)]
#![warn(bare_trait_objects)]
#![warn(clippy::pedantic)]
#![allow(clippy::stutter)]

#[macro_use] extern crate bitflags;
#[cfg(feature = "async")] extern crate bytes;
#[cfg(feature = "async")] #[macro_use] extern crate enum_kinds;
#[macro_use] extern crate failure;
extern crate interfaces;
#[macro_use] extern crate log;
#[cfg(feature = "async")] extern crate mio;
extern crate nix;
#[cfg(feature = "async")] extern crate owning_ref;
extern crate pnet_packet;
#[cfg(feature = "seccomp")] extern crate seccomp;
#[cfg(feature = "async")] extern crate spin;
#[cfg(feature = "async")] extern crate tokio;

#[macro_use] extern crate boolean_enums;
#[macro_use] extern crate enum_repr;

#[macro_use] mod util;
#[macro_use] pub mod bpf;
pub mod errors;
pub mod constants;
pub mod functions;
pub mod socket;
pub mod structs;

pub mod raw {
    pub use constants::raw::*;
    pub use functions::raw::*;
    pub use structs::raw::*;
}

#[cfg(feature = "async")]
pub mod futures {
    pub use socket::futures::*;
}

use nix::libc as nlibc;

pub use self::bpf::*;
pub use self::constants::*;
pub use self::functions::*;
pub use self::socket::*;
pub use self::structs::*;
use self::util::check_for_eagain;

#[cfg(feature = "async")]
pub use self::errors::ErrorKind::{Again, Interrupted};

pub use enum_repr::*;

pub use pnet_packet::ipv6::Ipv6;
