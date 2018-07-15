#![allow(bare_trait_objects)] // triggered by failure_derive

use ::std::io;

pub type Result<T> = ::std::result::Result<T, ::failure::Error>;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "error polling timer")]
    TimerError(#[cause] ::tokio_timer::Error),

    #[fail(display = "privilege operation error (is cap_net_raw+p not set \
        on the executable?)")]
    Priv(#[cause] io::Error)
}
