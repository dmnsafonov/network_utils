use ::std::io;
use ::std::io::prelude::*;
use ::std::os::unix::prelude::*;

use ::futures::prelude::*;
use ::owning_ref::*;

use ::ping6_datacommon::*;
use ::linux_network::*;

use ::errors::{Error, ErrorKind, Result};

pub struct StdinBytesIterator<'a> {
    tin: MovableIoLock<'a, io::Stdin>
}

impl<'a> StdinBytesIterator<'a> {
    pub fn new() -> StdinBytesIterator<'a> {
        StdinBytesIterator {
            tin: movable_io_lock(io::stdin())
        }
    }
}

impl<'a> Iterator for StdinBytesIterator<'a> {
    type Item = Result<OwningRef<Vec<u8>, [u8]>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut len_buf = [0; 2];
        match self.tin.read(&mut len_buf) {
            Ok(0) => return None,
            Err(e) => return Some(Err(e.into())),
            _ => ()
        };
        let len = ((len_buf[0] as usize) << 8) | (len_buf[1] as usize);

        let mut buf = vec![0; ::std::u16::MAX as usize];
        match self.tin.read(&mut buf[..len]) {
            Ok(x) if x == len => (),
            Ok(x) => return Some(Err(ErrorKind::WrongLength(x, len).into())),
            Err(e) => return Some(Err(e.into()))
        };

        let ret = VecRef::new(buf).map(|v| &v[..len]);
        Some(Ok(ret))
    }
}

impl<'a> AsRawFd for StdinBytesIterator<'a> {
    fn as_raw_fd(&self) -> RawFd {
        io::stdin().as_raw_fd()
    }
}

pub struct StdinBytesFuture<'a> {
    iter: &'a mut StdinBytesIterator<'a>,
    pending: bool,
    drop_nonblock: bool
}

impl<'a> StdinBytesFuture<'a> {
    pub fn new(iter: &'a mut StdinBytesIterator<'a>)
            -> Result<StdinBytesFuture<'a>> {
        let old = set_fd_nonblock(iter, true)?;
        Ok(StdinBytesFuture {
            iter: iter,
            pending: true,
            drop_nonblock: !old
        })
    }
}

impl<'a> Drop for StdinBytesFuture<'a> {
    fn drop(&mut self) {
        if self.drop_nonblock {
            set_fd_nonblock(self.iter, false).unwrap();
        }
    }
}

impl<'a> Future for StdinBytesFuture<'a> {
    type Item = OwningRef<Vec<u8>, [u8]>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        assert!(self.pending);
        let res = self.iter.next().unwrap_or(Ok(OwningRef::new(Vec::new())));
        match res {
            Err(Error(ErrorKind::IoError(e), magic)) => {
                if let io::ErrorKind::WouldBlock = e.kind() {
                    Ok(Async::NotReady)
                } else {
                    bail!(Error(ErrorKind::IoError(e), magic))
                }
            },
            Err(e) => Err(e),
            Ok(x) => Ok(Async::Ready(x))
        }
    }
}
