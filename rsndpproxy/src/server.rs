use ::std::net::Ipv6Addr;
use ::std::sync::{Arc, atomic::*};

use ::failure::ResultExt;
use ::futures::future::poll_fn;
use ::futures::stream::unfold;
use ::ip_network::Ipv6Network;
use ::pnet_packet::icmpv6::{Icmpv6Types, ndp::*};
use ::tokio::prelude::*;

use ::linux_network::{*, futures, futures::*};
use ::send_box::SendBox;

use ::broadcast::*;
use ::config::*;
use ::errors::{Error, Result};
use ::packet::*;
use ::util::*;

type StreamE<T> = Stream<Item = T, Error = ::failure::Error>;

pub struct Server {
    sock: futures::IpV6PacketSocketAdapter,
    input: SendBox<StreamE<(Solicitation, Override)>>,
    quit: Receiver<::QuitKind>,
    got_a_normal_quit: bool,
    drop_allmulti: bool,
    ifname: String,
    mtu: usize,
    prefixes: Arc<Vec<PrefixConfig>>,
    queued_sends: Arc<AtomicUsize>,
    max_queued: usize
}

impl Server {
    pub fn new(
        ifc: &InterfaceConfig,
        quit: Receiver<::QuitKind>
    ) -> Result<Server> {
        let sock_raw = IpV6PacketSocket::new(
            ::linux_network::raw::ETHERTYPE_IPV6,
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
                Self::make_input_stream(
                    sock,
                    mtu,
                    prefixes.clone(),
                    ifc.name.clone()
                )
            )) },
            quit,
            got_a_normal_quit: false,
            drop_allmulti,
            ifname: ifc.name.clone(),
            mtu,
            prefixes,
            queued_sends: Arc::new(AtomicUsize::new(0)),
            max_queued: ifc.max_queued
        })
    }

    fn create_filter() -> Box<BpfProg> {
        use ::linux_network::BpfCommandFlags as B;
        use ::linux_network::raw::*;
        use ::nix::libc::*;

        bpf_filter!(
            bpf_stmt!(B::LD | B::B | B::ABS, 6);
            bpf_jump!(B::JMP | B::JEQ | B::K, IPPROTO_ICMPV6, 0, 3);

            bpf_stmt!(B::LD | B::B | B::ABS, 40);
            bpf_jump!(B::JMP | B::JEQ | B::K, ND_NEIGHBOR_SOLICIT, 0, 1);

            bpf_stmt!(B::RET | B::K, ::std::u32::MAX);

            bpf_stmt!(B::RET | B::K, 0);
        )
    }

    fn make_input_stream(
        sock: IpV6PacketSocketAdapter,
        mtu: usize,
        prefixes: Arc<Vec<PrefixConfig>>,
        if_name: impl AsRef<str>
    ) -> impl Stream<
        Item = (Solicitation, Override),
        Error = ::failure::Error
    > {
        let if_name_clone = if_name.as_ref().to_string();

        unfold((sock, mtu), move |(mut sock, mtu)| {
            Some(sock.recvpacket(mtu, RecvFlags::empty())
                .map(move |x| (x, (sock, mtu)))
                .map_err(|e| e.into())
            )
        }).filter_map(move |(packet, _)| { // TODO: use macaddr later
            // validate common solicitation features
            debug!("received a packet on {}", if_name.as_ref());

            let solicit = match Solicitation::parse(&packet) {
                Some(s) => s,
                None => return None
            };

            let mut prefix = None;
            for i in &*prefixes {
                if i.prefix.contains(solicit.src) {
                    continue;
                }

                if !i.prefix.contains(solicit.target) {
                    continue;
                }

                if solicit.src.is_unspecified() {
                    warn!(
                        "Duplicate address detection occurred \
                            on interface {} for address {} (configured \
                            prefix {}).  Part of the proxied subnet \
                            is on the {} side!",
                        if_name.as_ref(),
                        solicit.src,
                        i.prefix,
                        if_name.as_ref()
                    );
                    return None;
                }

                prefix = Some((i.prefix.clone(), i.override_flag));
                break;
            }

            match prefix {
                Some((p, o)) => Some((solicit, p, o.into())),
                None => None
            }
        }).filter_map(move |(solicit, prefix, override_flag)| {
            // validate type-specific solicitation features
            debug!(
                "the packet received on {} is generally valid",
                if_name_clone
            );

            if is_solicited_node_multicast(&solicit.dst) {
                // ll address resolution

                if solicit.ll_addr_opt.is_none() {
                    return None;
                }

                if prefix.netmask() > 104 {
                    let n_mask = prefix.netmask();
                    let get_bits = |addr: &Ipv6Addr| -> u32 {
                        let s = addr.segments();
                        let last_bits = ((s[6] as u32 & 0xff) << 16)
                            | s[7] as u32;
                        last_bits >> (128 - n_mask)
                    };

                    let dst_bits = get_bits(&solicit.dst);
                    let prefix_bits = get_bits(&prefix.network_address());
                    if dst_bits != prefix_bits {
                        return None;
                    }
                }
            } else {
                // neighbor reachability detection

                if !prefix.contains(solicit.dst) {
                    return None;
                }
            }

            Some((solicit, override_flag))
        })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.drop_allmulti {
            ::util::log_if_err(
                self.sock.set_allmulti(false, &self.ifname)
                    .context("error returning allmilti flag to the previous \
                        state")
                    .map_err(|e| e.into())
            );
        }
    }
}

impl Future for Server {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        debug!("waiting for a solicitation");

        let mut active = true;
        while active {
            active = false;

            if let Async::Ready(qk) = self.quit.poll()
                    .map_err(|e| log_err(e.into()))? {
                debug!("received a signal, quitting");
                active = true;
                match qk.expect("a quit signal") {
                    // the distinction will be important when implementing
                    // querying the target network's interface
                    // currently queued packets are purposefully omitted
                    ::QuitKind::Fast | ::QuitKind::Normal =>
                        return Ok(Async::Ready(()))
                }
            }

            if let Async::Ready(Some((solicit, override_flag)))
                    = self.input.poll().map_err(log_err)? {
                debug!(
                    "the solicitation received on {} must be proxied, \
                        the solicitation is {:?}",
                    self.ifname,
                    solicit
                );
                active = true;

                let adv = Advertisement {
                    src: solicit.target,
                    dst: solicit.src,
                    target: solicit.target,
                    ll_addr_opt: Some(self.sock.get_mac())
                };
                let adv_packet = adv.solicited_to_ipv6(override_flag.into());

                let queued_sends = self.queued_sends.clone();

                let queued = queued_sends.fetch_add(1, Ordering::Relaxed);
                if queued >= self.max_queued {
                    warn!(
                        "Maximum queued packet number ({}) \
                            for interface {} exceeded.",
                        self.max_queued,
                        self.ifname
                    );
                    queued_sends.fetch_sub(1, Ordering::Relaxed);
                    continue;
                }

                ::tokio::spawn(
                    self.sock.sendpacket(
                        adv_packet,
                        Some(solicit.ll_addr_opt.unwrap()),
                        SendFlags::empty()
                    ).map(
                        |_| ()
                    ).map_err(
                        |e| log_err(Error::LinuxNetworkError(e).into())
                    ).inspect(move |_| {
                        queued_sends.fetch_sub(1, Ordering::Relaxed);
                    })
                );
                debug!("advertisement queued on {}", self.ifname);
            }
        }

        Ok(Async::NotReady)
    }
}
