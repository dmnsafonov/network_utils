#![allow(bare_trait_objects)] // triggered by failure_derive
#![allow(clippy::pub_enum_variant_names)]

use ::std::io;

use ::failure::Fail;

pub type Result<T> = ::std::result::Result<T, ::failure::Error>;

pub fn error_to_errno(x: &dyn Fail) -> Option<i32> {
    x.downcast_ref::<io::Error>().and_then(|e| e.raw_os_error())
}

#[cfg_attr(feature = "async", derive(EnumKind))]
#[cfg_attr(feature = "async", enum_kind(ErrorKind))]
#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "system call would block")]
    Again(#[cause] io::Error),

    #[fail(display = "error getting address {} info: {}", addr, explanation)]
    AddrError {
        addr: String,
        explanation: String
    },

    #[fail(display = "provided buffer of length {} is too small", len)]
    BufferTooSmall {
        len: usize
    },

    #[fail(display = "cannot get the network interface info")]
    GetInterfaceError {
        name: String,
        #[cause] cause: ::interfaces::InterfacesError
    },

    #[fail(display = "interface name \"{}\" is too long", _0)]
    IfNameTooLong {
        if_name: String
    },

    #[fail(display = "system call was interrupted")]
    Interrupted(#[cause] io::Error),

    #[fail(display = "io error")]
    IoError(#[cause] io::Error),

    #[fail(display = "no \"{}\" network interface", name)]
    NoInterface {
        name: String
    },

    #[fail(display = "io error ocurred on a socket")]
    SocketError(#[cause] ::failure::Compat<::failure::Error>),

    #[fail(display = "io error ocurred in tokio")]
    TokioError(#[cause] io::Error),

    #[fail(display = "wrong buffer length")]
    WrongSize
}
