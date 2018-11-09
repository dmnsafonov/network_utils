use ::std::cell::UnsafeCell;
use ::std::io;
use ::std::io::prelude::*;
use ::std::os::unix::prelude::*;
use ::std::sync::Arc;

use ::mio;
use ::mio::*;
use ::mio::event::Evented;
use ::mio::unix::EventedFd;
use ::tokio::prelude::*;
use ::tokio::reactor::*;

use ::ping6_datacommon::*;
use ::linux_network::*;

use ::errors::{Error, Result};

pub struct StdinBytesIterator<'a>(MovableIoLock<'a, io::Stdin>);

impl<'a> StdinBytesIterator<'a> {
    pub fn new() -> StdinBytesIterator<'a> {
        StdinBytesIterator(movable_io_lock(io::stdin()))
    }
}

impl<'a> Iterator for StdinBytesIterator<'a> {
    type Item = Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut len_buf = [0; 2];
        match self.0.read(&mut len_buf) {
            Ok(0) => return None,
            Err(e) => return Some(Err(e.into())),
            _ => ()
        };
        let len = ((len_buf[0] as usize) << 8) | (len_buf[1] as usize);

        let mut buf = vec![0; len];
        match self.0.read(&mut buf[..len]) {
            Ok(x) if x == len => (),
            Ok(exp) => return Some(Err(Error::WrongLengthMessage {
                len,
                exp
            }.into())),
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

struct StdinWrapper(io::Stdin);

impl AsRawFd for StdinWrapper {
    fn as_raw_fd(&self) -> RawFd {
        io::stdin().as_raw_fd()
    }
}

gen_evented_eventedfd!(StdinWrapper);

#[derive(Clone)]
pub struct StdinBytesReader(Arc<UnsafeCell<StdinBytesReaderImpl>>);
unsafe impl Send for StdinBytesReader {}
unsafe impl Sync for StdinBytesReader {}

struct StdinBytesReaderImpl {
    stdin: PollEvented2<StdinWrapper>,
    drop_nonblock: bool
}

impl StdinBytesReader {
    pub fn new(handle: &Handle) -> Result<Self> {
        let old = set_fd_nonblock(&io::stdin(), Nonblock::Yes)?;
        Ok(StdinBytesReader(Arc::new(UnsafeCell::new(StdinBytesReaderImpl {
            stdin: PollEvented2::new_with_handle(
                StdinWrapper(io::stdin()),
                handle
            )?,
            drop_nonblock: !old
        }))))
    }
}

impl Read for StdinBytesReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let theself = unsafe { self.0.get().as_ref().unwrap() };
        theself.stdin.get_ref().0.lock().read(buf)
    }
}

impl AsyncRead for StdinBytesReader {}

impl Drop for StdinBytesReader {
    fn drop(&mut self) {
        let theself = unsafe { self.0.get().as_ref().unwrap() };
        if theself.drop_nonblock {
            set_fd_nonblock(theself.stdin.get_ref(), Nonblock::No).unwrap();
        }
    }
}
