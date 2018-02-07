use ::std::io;
use ::std::io::prelude::*;
use ::std::os::unix::prelude::*;

use ::mio;
use ::mio::*;
use ::mio::event::Evented;
use ::mio::unix::EventedFd;
use ::tokio_core::reactor::*;

use ::ping6_datacommon::*;
use ::linux_network::*;

use ::errors::{ErrorKind, Result};

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
    type Item = Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut len_buf = [0; 2];
        match self.tin.read(&mut len_buf) {
            Ok(0) => return None,
            Err(e) => return Some(Err(e.into())),
            _ => ()
        };
        let len = ((len_buf[0] as usize) << 8) | (len_buf[1] as usize);

        let mut buf = vec![0; len];
        match self.tin.read(&mut buf[..len]) {
            Ok(x) if x == len => (),
            Ok(x) => return Some(Err(ErrorKind::WrongLength(x, len).into())),
            Err(e) => return Some(Err(e.into()))
        };

        Some(Ok(buf))
    }
}

impl<'a> AsRawFd for StdinBytesIterator<'a> {
    fn as_raw_fd(&self) -> RawFd {
        // safe, because it essencially returns STDIN_FILENO without locking
        io::stdin().as_raw_fd()
    }
}

struct StdinLockWrapper<'a>(io::StdinLock<'a>);

impl<'a> Read for StdinLockWrapper<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl<'a> AsRawFd for StdinLockWrapper<'a> {
    fn as_raw_fd(&self) -> RawFd {
        io::stdin().as_raw_fd()
    }
}

gen_evented_eventedfd_lifetimed!(StdinLockWrapper<'gen_lifetime>);

pub struct StdinBytesReader<'a> {
    stdin: PollEvented<StdinLockWrapper<'a>>,
    drop_nonblock: bool
}

impl<'a> StdinBytesReader<'a> {
    pub fn new(handle: &Handle, stdin: io::StdinLock<'a>)
            -> Result<StdinBytesReader<'a>> {
        let old = set_fd_nonblock(&io::stdin(), Nonblock::Yes)?;
        Ok(StdinBytesReader {
            stdin: PollEvented::new(StdinLockWrapper(stdin), handle)?,
            drop_nonblock: !old
        })
    }
}

impl<'a> Read for StdinBytesReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stdin.get_mut().read(buf)
    }
}

impl<'a> Drop for StdinBytesReader<'a> {
    fn drop(&mut self) {
        if self.drop_nonblock {
            set_fd_nonblock(&io::stdin(), Nonblock::No).unwrap();
        }
    }
}
