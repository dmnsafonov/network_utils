use ::std::cell::RefCell;
use ::std::io;
use ::std::io::prelude::*;
use ::std::os::unix::prelude::*;
use ::std::rc::Rc;

use ::futures::prelude::*;
use ::mio;
use ::mio::*;
use ::mio::event::Evented;
use ::mio::unix::EventedFd;
use ::tokio_core::reactor::*;
use ::tokio_io::AsyncWrite;

use ::linux_network::*;

use ::errors::Result;

struct StdoutLockWrapper<'a>(io::StdoutLock<'a>);

impl<'a> Write for StdoutLockWrapper<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<'a> AsRawFd for StdoutLockWrapper<'a> {
    fn as_raw_fd(&self) -> RawFd {
        io::stdout().as_raw_fd()
    }
}

gen_evented_eventedfd_lifetimed!(StdoutLockWrapper<'gen_lifetime>);

#[derive(Clone)]
pub struct StdoutBytesWriter<'a>(Rc<RefCell<StdoutBytesWriterImpl<'a>>>);

struct StdoutBytesWriterImpl<'a> {
    stdout: PollEvented<StdoutLockWrapper<'a>>,
    drop_nonblock: bool
}

impl<'a> StdoutBytesWriter<'a> {
    pub fn new(handle: &Handle, stdout: io::StdoutLock<'a>)
            -> Result<StdoutBytesWriter<'a>> {
        let old = set_fd_nonblock(&io::stdout(), Nonblock::Yes)?;
        let ret = Rc::new(RefCell::new(
            StdoutBytesWriterImpl {
                stdout: PollEvented::new(StdoutLockWrapper(stdout), handle)?,
                drop_nonblock: !old
            }
        ));
        Ok(StdoutBytesWriter(ret))
    }
}

impl<'a> Write for StdoutBytesWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut theself = self.0.borrow_mut();
        theself.stdout.get_mut().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut theself = self.0.borrow_mut();
        theself.stdout.get_mut().flush()
    }
}

impl<'a> AsyncWrite for StdoutBytesWriter<'a> {
    fn shutdown(&mut self) -> ::futures::Poll<(), io::Error> {
        Ok(Async::Ready(try_nb!(self.flush())))
    }
}

impl<'a> Drop for StdoutBytesWriter<'a> {
    fn drop(&mut self) {
        let theself = self.0.borrow();
        if theself.drop_nonblock {
            set_fd_nonblock(&io::stdout(), Nonblock::No).unwrap();
        }
    }
}
