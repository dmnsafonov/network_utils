use ::std::cell::UnsafeCell;
use ::std::io;
use ::std::io::prelude::*;
use ::std::os::unix::prelude::*;
use ::std::sync::Arc;

use ::futures::prelude::*;
use ::mio;
use ::mio::*;
use ::mio::event::Evented;
use ::mio::unix::EventedFd;
use ::tokio::prelude::*;
use ::tokio::reactor::*;

use ::linux_network::*;

use ::errors::Result;

struct StdoutWrapper(io::Stdout);

impl AsRawFd for StdoutWrapper {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

gen_evented_eventedfd!(StdoutWrapper);

#[derive(Clone)]
pub struct StdoutBytesWriter(Arc<UnsafeCell<StdoutBytesWriterImpl>>);
unsafe impl Send for StdoutBytesWriter {}

struct StdoutBytesWriterImpl {
    stdout: PollEvented2<StdoutWrapper>,
    drop_nonblock: bool
}

impl StdoutBytesWriter {
    pub fn new(handle: &Handle)
            -> Result<StdoutBytesWriter> {
        let old = set_fd_nonblock(&io::stdout(), Nonblock::Yes)?;
        let ret = Arc::new(UnsafeCell::new(
            StdoutBytesWriterImpl {
                stdout: PollEvented2::new_with_handle(
                    StdoutWrapper(io::stdout()),
                    handle
                )?,
                drop_nonblock: !old
            }
        ));
        Ok(StdoutBytesWriter(ret))
    }
}

impl Write for StdoutBytesWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let theself = unsafe { self.0.get().as_ref().unwrap() };
        theself.stdout.get_ref().0.lock().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let theself = unsafe { self.0.get().as_ref().unwrap() };
        theself.stdout.get_ref().0.lock().flush()
    }
}

impl AsyncWrite for StdoutBytesWriter {
    fn shutdown(&mut self) -> ::futures::Poll<(), io::Error> {
        try_nb!(self.flush());
        Ok(Async::Ready(()))
    }
}

impl Drop for StdoutBytesWriter {
    fn drop(&mut self) {
        let theself = unsafe { self.0.get().as_ref().unwrap() };
        if theself.drop_nonblock {
            set_fd_nonblock(
                &theself.stdout.get_ref().0,
                Nonblock::No
            ).expect("unable to drop the nonblock flag on stdout");
        }
    }
}
