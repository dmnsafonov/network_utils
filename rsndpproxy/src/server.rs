use ::std::net::Ipv6Addr;
use ::std::sync::{Arc, atomic::*};

use ::failure::ResultExt;
use ::futures::stream::unfold;
use ::pnet_packet::icmpv6::{Icmpv6Types, ndp::*};
use ::tokio::prelude::*;

use ::linux_network::{*, futures, futures::*};

use ::config::*;
use ::errors::{Error, Result};
use ::packet::*;
use ::util::make_solicited_node_multicast;
use ::send_box::SendBox;

type StreamE<T> = Stream<Item = T, Error = ::failure::Error>;

pub struct Server {
    sock: futures::IpV6PacketSocketAdapter,
    input: SendBox<StreamE<Solicitation>>,
    fast_quit: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
    drop_allmulti: bool,
    ifname: String,
    mtu: usize,
    prefixes: Arc<Vec<PrefixConfig>>
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
        debug!("created raw socket");

        let mut sock = futures::IpV6PacketSocketAdapter::new(
            &::tokio::reactor::Handle::current(),
            sock_raw
        )?;
        debug!("registered raw socket in the reactor");

        sock.setsockopt(&SockOpts::DontRoute::new(&true))?;

        let filter = Self::create_filter();
        sock.setsockopt(&SockOpts::AttachFilter::new(filter.get()))?;
        sock.setsockopt(&SockOpts::LockFilter::new(&true))?;

        debug!("packet filtration set");

        let drop_allmulti = !sock.set_allmulti(true, &ifc.name)?;
        debug!("ensured allmulti is set on the interface");

        let mtu = get_interface_mtu(&sock, &ifc.name)? as usize;

        let prefixes = Arc::new(ifc.prefixes.clone());

        Ok(Server {
            sock: sock.clone(),
            input: unsafe { SendBox::new(Box::new(
                Self::make_input_stream(sock, mtu, prefixes.clone())
            )) },
            fast_quit,
            quit,
            drop_allmulti,
            ifname: ifc.name.clone(),
            mtu,
            prefixes
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

    fn make_input_stream(
        sock: IpV6PacketSocketAdapter,
        mtu: usize,
        prefixes: Arc<Vec<PrefixConfig>>
    ) -> impl Stream<
        Item = Solicitation,
        Error = ::failure::Error
    > {
        unfold((sock, mtu), move |(mut sock, mtu)| {
            Some(sock.recvpacket(mtu, RecvFlags::empty())
                .map(move |x| (x, (sock, mtu)))
                .map_err(|e| e.into())
            )
        }).filter_map(move |packet| {
            let solicit = match parse_solicitation(&packet.0.payload) {
                Some(s) => s,
                None => return None
            };

            for i in &*prefixes {
                if i.prefix.contains(solicit.src) {
                    return None;
                }

                if i.prefix.get_netmask() > 104 {
                    let n_mask = i.prefix.get_netmask();
                    let get_bits = |addr: &Ipv6Addr| -> u32 {
                        let s = addr.segments();
                        let last_bits = ((s[6] as u32 & 0xff) << 16)
                            | s[7] as u32;
                        last_bits >> (128 - n_mask)
                    };

                    let dst_bits = get_bits(&solicit.dst);
                    let prefix_bits = get_bits(&i.prefix.get_network_address());
                    if dst_bits != prefix_bits {
                        return None;
                    }
                }

                if !i.prefix.contains(solicit.target) {
                    return None;
                }
            }

            Some(solicit)
        })
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
        debug!("receiving a solicitation");
        let solicit = try_ready!(self.input.poll().map_err(|_| ()));
        debug!("received a solicitation: {:?}", solicit);

        Ok(Async::NotReady)
    }
}
