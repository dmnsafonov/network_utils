use ::std::net::Ipv6Addr;
use ::libc::*;

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
                bail!(ErrorKind::Interrupted);
            } else if check_for_eagain(oserr) {
                bail!(ErrorKind::Again);
            } else {
                bail!(err);
            }
        } else {
            ret
        }
    })
}

#[cfg(feature = "futures")]
macro_rules! try_async {
    ($e:expr) => (
        match $e {
            Err(e) => {
                match *e.kind() {
                    Again => return Ok(Async::NotReady),
                    _ => return Err(e)
                }
            },
            Ok(x) => Ok(Async::Ready(x))
        }
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
        (ad[0] as u16) << 8 | (ad[1] as u16),
        (ad[2] as u16) << 8 | (ad[3] as u16),
        (ad[4] as u16) << 8 | (ad[5] as u16),
        (ad[6] as u16) << 8 | (ad[7] as u16),
        (ad[8] as u16) << 8 | (ad[9] as u16),
        (ad[10] as u16) << 8 | (ad[11] as u16),
        (ad[12] as u16) << 8 | (ad[13] as u16),
        (ad[14] as u16) << 8 | (ad[15] as u16)
   )
}

pub fn log_if_err<T,E>(x: ::std::result::Result<T,E>)
        where E: ::error_chain::ChainedError {
    if let Err(e) = x {
        error!("{}", e.display_chain());
    }
}

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
