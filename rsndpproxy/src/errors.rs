use ::std::io;

pub type Result<T> = ::std::result::Result<T, ::failure::Error>;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "another instance already running: failed locking {}", filename)]
    AlreadyRunning {
        filename: String
    },

    #[fail(display = "io error on file {}", name)]
    FileIo {
        name: String,
        #[cause] cause: io::Error
    },

    #[fail(display = "io error")]
    LinuxNetworkError(#[cause] ::linux_network::errors::Error),

    #[fail(display = "privilege dropping error")]
    PrivDrop(#[cause] io::Error),

    #[fail(display = "setting securebits failed")]
    SecurebitsError(#[cause] ::failure::Compat<::failure::Error>),

    #[fail(display = "waiting for signal failed")]
    SignalIOError(#[cause] io::Error)
}

impl From<::linux_network::errors::Error> for Error {
    fn from(err: ::linux_network::errors::Error) -> Error {
        Error::LinuxNetworkError(err)
    }
}
