#![warn(bare_trait_objects)]

#[macro_use] extern crate bitflags;
#[cfg(feature = "async")] extern crate bytes;
#[macro_use] extern crate enum_kinds_macros;
extern crate enum_kinds_traits;
#[macro_use] extern crate failure;
extern crate interfaces;
#[macro_use] extern crate log;
#[cfg(feature = "async")] extern crate mio;
extern crate nix;
#[cfg(feature = "async")] extern crate owning_ref;
extern crate pnet_packet;
#[cfg(feature = "seccomp")] extern crate seccomp;
#[cfg(feature = "async")] extern crate tokio;

#[macro_use] extern crate boolean_enums;
#[macro_use] extern crate numeric_enums;

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

pub use enum_kinds_traits::ToKind;

use nix::libc;

pub use self::bpf::*;
pub use self::constants::*;
pub use self::functions::*;
pub use self::socket::*;
pub use self::structs::*;
use self::util::check_for_eagain;

pub use self::errors::ErrorKind::{Again, Interrupted};

pub use numeric_enums::*;

pub use pnet_packet::ipv6::Ipv6;
