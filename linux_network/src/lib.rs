#[macro_use] extern crate error_chain;
#[cfg(feature = "async")] extern crate futures as ext_futures;
extern crate interfaces;
#[macro_use] extern crate log;
#[cfg(feature = "async")] extern crate mio;
extern crate nix;
extern crate pnet_packet;
#[cfg(feature = "seccomp")] extern crate seccomp;
#[cfg(feature = "async")] extern crate tokio_core;

#[macro_use] extern crate boolean_enums;
#[macro_use] extern crate numeric_enums;

#[macro_use] mod util;
#[macro_use] pub mod bpf;
pub mod epoll;
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

use nix::libc;

pub use self::bpf::*;
pub use self::epoll::*;
pub use self::constants::*;
pub use self::functions::*;
pub use self::socket::*;
pub use self::structs::*;
use self::util::check_for_eagain;

pub use self::errors::ErrorKind::{Again, Interrupted};

pub use numeric_enums::*;

pub use pnet_packet::ipv6::Ipv6;
