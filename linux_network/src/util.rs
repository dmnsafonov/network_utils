use ::std::net::Ipv6Addr;
use ::nlibc::*;

#[inline]
pub fn check_for_eagain(x: c_int) -> bool {
    x == EAGAIN || x == EWOULDBLOCK
}

macro_rules! n1try {
    ( $e:expr ) => ({
        let ret = $e;
        if ret == -1 {
            let err = ::std::io::Error::last_os_error();
            let oserr =  err.raw_os_error().unwrap() as c_int;
            if oserr == EINTR {
                return Err(Error::Interrupted(err).into());
            } else if check_for_eagain(oserr) {
                return Err(Error::Again(err).into());
            } else {
                return Err(Error::IoError(err).into());
            }
        } else {
            ret
        }
    })
}

#[cfg(feature = "async")]
macro_rules! try_async_val {
    ($e:expr) => (
        match $e {
            Err(e) => {
                match (&e).into() {
                    Again => return Ok(Async::NotReady),
                    _ => return Err(e)
                }
            },
            Ok(x) => x
        }
    )
}

#[cfg(feature = "async")]
macro_rules! try_async {
    ($e:expr) => (
        Ok(Async::Ready(try_async_val!($e)))
    )
}

pub unsafe fn ref_to_cvoid<T: ?Sized>(x: &T) -> *const c_void {
    x as *const T as *const c_void
}

pub unsafe fn ref_to_mut_cvoid<T: ?Sized>(x: &mut T) -> *mut c_void {
    x as *mut T as *mut c_void
}

pub fn addr_from_segments(ad: &[u8; 16]) -> Ipv6Addr {
    Ipv6Addr::new(
        (u16::from(ad[0]) << 8) | u16::from(ad[1]),
        (u16::from(ad[2]) << 8) | u16::from(ad[3]),
        (u16::from(ad[4]) << 8) | u16::from(ad[5]),
        (u16::from(ad[6]) << 8) | u16::from(ad[7]),
        (u16::from(ad[8]) << 8) | u16::from(ad[9]),
        (u16::from(ad[10]) << 8) | u16::from(ad[11]),
        (u16::from(ad[12]) << 8) | u16::from(ad[13]),
        (u16::from(ad[14]) << 8) | u16::from(ad[15])
   )
}

pub fn log_if_err<T>(x: ::std::result::Result<T, ::failure::Error>) {
    if let Err(e) = x {
        let mut out = String::new();

        let mut first = true;;
        for i in e.iter_chain() {
            if !first {
                out += ": ";
            }
            out += &format!("{}", i);
            first = false;
        }

        error!("{}", out);
    }
}

#[cfg(feature = "async")]
#[macro_export]
macro_rules! gen_evented_eventedfd {
    ($name:ident) => (
        impl Evented for $name {
            fn register(
                &self,
                poll: &mio::Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt
            ) -> io::Result<()> {
                EventedFd(&self.as_raw_fd())
                    .register(poll, token, interest, opts)
            }

            fn reregister(
                &self,
                poll: &mio::Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt
            ) -> io::Result<()> {
                EventedFd(&self.as_raw_fd())
                    .reregister(poll, token, interest, opts)
            }

            fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
                EventedFd(&self.as_raw_fd())
                    .deregister(poll)
            }
        }
    )
}

#[macro_export]
macro_rules! gen_evented_eventedfd_lifetimed {
    ($name:ty) => (
        impl<'gen_lifetime> Evented
                for $name {
            fn register(
                &self,
                poll: &mio::Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt
            ) -> io::Result<()> {
                EventedFd(&self.as_raw_fd())
                    .register(poll, token, interest, opts)
            }

            fn reregister(
                &self,
                poll: &mio::Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt
            ) -> io::Result<()> {
                EventedFd(&self.as_raw_fd())
                    .reregister(poll, token, interest, opts)
            }

            fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
                EventedFd(&self.as_raw_fd())
                    .deregister(poll)
            }
        }
    )
}

#[cfg(feature = "async")]
#[macro_export]
macro_rules! try_nb {
    ($e:expr) => (match $e {
        Ok(t) => t,
        Err(ref e) if e.kind() == ::std::io::ErrorKind::WouldBlock => {
            return Ok(::futures::Async::NotReady)
        }
        Err(e) => return Err(e.into()),
    })
}
