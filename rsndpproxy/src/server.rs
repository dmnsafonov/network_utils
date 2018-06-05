use ::std::sync::{Arc, atomic::*};

use ::failure::ResultExt;
use ::tokio::prelude::*;

use ::linux_network::{*, futures, futures::*};

use ::config::InterfaceConfig;
use ::errors::{Error, Result};

pub struct Server {
    sock: futures::IpV6PacketSocketAdapter,
    fast_quit: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
    drop_allmulti: bool,
    ifname: String
}

impl Server {
    pub fn new(
        ifc: &InterfaceConfig,
        fast_quit: Arc<AtomicBool>,
        quit: Arc<AtomicBool>
    ) -> Result<Server> {
        let sock_raw = IpV6PacketSocket::new(
            ::linux_network::raw::ETHERTYPE_IPV6 as ::nix::libc::c_int,
            SockFlag::empty(),
            &ifc.name
        )?;

        let mut sock = futures::IpV6PacketSocketAdapter::new(
            &::tokio::reactor::Handle::current(),
            sock_raw
        )?;

        sock.setsockopt(&SockOpts::BindToDevice::new(&ifc.name))?;
        sock.setsockopt(&SockOpts::DontRoute::new(&true))?;
        sock.setsockopt(&SockOpts::V6Only::new(&true))?;

        let filter = Self::create_filter();
        sock.setsockopt(&SockOpts::AttachFilter::new(filter.get()))?;
        sock.setsockopt(&SockOpts::LockFilter::new(&true))?;

        let drop_allmulti = !sock.set_allmulti(true, &ifc.name)?;

        Ok(Server {
            sock,
            fast_quit,
            quit,
            drop_allmulti,
            ifname: ifc.name.clone()
        })
    }

    fn create_filter() -> Box<BpfProg> {
        use ::linux_network::BpfCommandFlags as B;
        use ::linux_network::raw::*;
        use ::nix::libc::*;

        // TODO: expand to cover possible ipv6 options
        bpf_filter!(
            bpf_stmt!(B::LD | B::H | B::ABS, 12);
            bpf_jump!(B::JMP | B::JEQ | B::K, ETHERTYPE_IPV6, 0, 5);

            bpf_stmt!(B::LD | B::B | B::ABS, 20);
            bpf_jump!(B::JMP | B::JEQ | B::K, IPPROTO_ICMPV6, 0, 3);

            bpf_stmt!(B::LD | B::B | B::ABS, 54);
            bpf_jump!(B::JMP | B::JEQ | B::K, ND_NEIGHBOR_SOLICIT, 0, 1);

            bpf_stmt!(B::RET | B::K, ::std::u32::MAX);

            bpf_stmt!(B::RET | B::K, 0);
        )
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.drop_allmulti {
            ::util::log_if_err(
                self.sock.set_allmulti(false, &self.ifname)
                    .context("").map_err(|e| e.into())
            );
        }
    }
}

impl Future for Server {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        unimplemented!()
    }
}
