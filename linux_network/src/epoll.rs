use ::std::cell::*;
use ::std::marker::PhantomData;
use ::std::os::unix::prelude::*;
use ::std::rc::*;

use ::nix::errno::*;
use ::nix::unistd::*;
use ::nix::sys::epoll::*;

use errors::{Error, Result};
use ::util::*;

type EPollRef<'a> = Rc<RefCell<EPoll<'a>>>;
type EPollWeakRef<'a> = Weak<RefCell<EPoll<'a>>>;

pub use ::nix::sys::epoll::{
    EpollFlags,

    EPOLLIN,
    EPOLLOUT,
    EPOLLRDHUP,
    EPOLLPRI,
    EPOLLERR,
    EPOLLHUP,
    EPOLLET,
    EPOLLONESHOT,
    EPOLLWAKEUP,
    EPOLLEXCLUSIVE
};

pub struct EPoll<'a> {
    fd: RawFd,
    rc: EPollWeakRef<'a>,
    _phantom: PhantomData<&'a RawFd>
}

impl<'a> EPoll<'a> {
    pub fn new() -> Result<EPollRef<'a>> {
        let ret = Rc::new(RefCell::new(EPoll {
            fd: epoll_create()?,
            rc: Weak::new(),
            _phantom: PhantomData
        }));
        let weak = Rc::downgrade(&ret);
        ret.borrow_mut().rc = weak;
        Ok(ret)
    }

    pub fn add<'b, T>(&mut self, fd: Rc<RefCell<T>>, flags: EpollFlags)
            -> Result<()>
            where
                T: AsRawFd,
                'b: 'a {
        let raw = fd.borrow().as_raw_fd();
        epoll_ctl(
            self.fd,
            EpollOp::EpollCtlAdd,
            raw,
            &mut EpollEvent::new(flags, raw as u64)
        )?;
        Ok(())
    }

    pub fn del<T>(&mut self, fd: &mut T) -> Result<()> where T: AsRawFd {
        epoll_ctl(self.fd, EpollOp::EpollCtlDel, fd.as_raw_fd(),
            &mut EpollEvent::empty())?;
        Ok(())
    }
}

impl<'a> Drop for EPoll<'a> {
    fn drop(&mut self) {
        log_if_err(close(self.fd).map_err(Error::from));
    }
}

impl<'a, 'b> IntoIterator for &'b EPoll<'a> {
    type Item = <EPollIterator<'a> as Iterator>::Item;
    type IntoIter = EPollIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        EPollIterator::new(self.rc.upgrade().unwrap())
    }
}

pub struct EPollIterator<'a>(EPollRef<'a>);

impl<'a> EPollIterator<'a> {
    fn new(epoll: EPollRef<'a>) -> EPollIterator<'a> {
        EPollIterator(epoll)
    }
}

impl<'a> Iterator for EPollIterator<'a> {
    type Item = EpollEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let mut event = [EpollEvent::empty()];
        let res = {
            let epoll = self.0.borrow();
            epoll_wait(epoll.fd, &mut event, -1)
        };
        match res {
            Err(::nix::Error::Sys(Errno::EINTR)) => {
                debug!("epoll wait interrupted");
                self.next()
            },
            Err(e) => {
                error!("epoll failed with {}", e);
                panic!()
            },
            Ok(_) => {
                Some(event[0])
            }
        }
    }
}
