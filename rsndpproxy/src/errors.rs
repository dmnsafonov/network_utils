use ::std::io;

pub type Result<T> = ::std::result::Result<T, ::failure::Error>;

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "another instance already running: failed locking {}", filename)]
    AlreadyRunning {
        filename: String
    },

    #[fail(display = "privilege dropping error: {}", _0)]
    PrivDrop(#[cause] io::Error),

    #[fail(display = "io error on file {}: {}", name, cause)]
    FileIo {
        name: String,
        #[cause] cause: io::Error
    },

    #[fail(display = "cannot find network interface {}", name)]
    NoInterface {
        name: String
    },

    #[fail(display = "cannot get the mac address of the interface {}", if_name)]
    NoMac {
        if_name: String
    }
}
