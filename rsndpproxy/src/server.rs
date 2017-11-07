use ::std::cell::*;
use ::std::collections::VecDeque;
use ::std::net::*;
use ::std::os::raw::c_uint;
use ::std::os::unix::prelude::*;
use ::std::rc::*;

use ::nix::sys::epoll::*;
use ::nix::unistd::*;
use ::pnet_packet::icmpv6::*;
use ::pnet_packet::ipv6::*;

use ::linux_network::*;

use ::config::*;
use ::util::*;
use super::errors::{Error, ErrorKind, Result, ResultExt};

pub struct Server<'a> {
    sock: IpV6PacketSocket,
//    tif: NetworkInterface,
    epoll: Rc<RefCell<EPoll<'a>>>,
    write_queue: VecDeque<Ipv6Packet<'a>>,
    prev_allmulti: bool
}

impl<'a> Server<'a> {
    pub fn new<'b>(config: &'b InterfaceConfig, epoll: Rc<RefCell<EPoll<'a>>>)
            -> Result<Server<'a>> {
        unimplemented!()
    }

    fn setup_bpf(&mut self) -> Result<()> {
        use ::libc::*;

        use ::linux_network::constants::raw::*;
        use ::linux_network::BpfCommandFlags::*;

        // TODO: process ipv6 extension headers properly
        let filter = bpf_filter!(
            bpf_stmt!(LD | H | ABS, 12);
            bpf_jump!(JMP | JEQ | K, ETHERTYPE_IPV6, 0, 5);

            bpf_stmt!(LD | B | ABS, 20);
            bpf_jump!(JMP | JEQ | K, IPPROTO_ICMPV6, 0, 3);

            bpf_stmt!(LD | B | ABS, 54);
            bpf_jump!(JMP | JEQ | K, ND_NEIGHBOR_SOLICIT, 0, 1);

            bpf_stmt!(RET | K, ::std::u32::MAX);

            bpf_stmt!(RET | K, 0);
        );

        self.sock.setsockopt(SockOptLevel::Socket, &SockOpt::AttachFilter(&filter))?;
        self.sock.setsockopt(SockOptLevel::Socket, &SockOpt::LockFilter(true))?;

        Ok(())
    }

    pub fn serve(&mut self, ev: EpollFlags) {
        let log_err = |x| {
            if let Err(e) = x {
                error!("{}", e);
            }
        };

        if ev.intersects(EPOLLIN) {
            log_err(self.serve_read());
        }

        if ev.intersects(EPOLLOUT) {
            log_err(self.serve_write());
        }
    }

    fn serve_read(&mut self) -> Result<()> {
        unimplemented!()
    }

    fn serve_write(&mut self) -> Result<()> {
        unimplemented!()
    }
}

impl<'a> Drop for Server<'a> {
    fn drop(&mut self) {
        unimplemented!() // restore allmulti
    }
}

impl<'a> AsRawFd for Server<'a> {
    fn as_raw_fd(&self) -> RawFd {
        unimplemented!()
    }
}
