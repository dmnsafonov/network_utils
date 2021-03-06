use ::std::net::*;
use ::std::sync::{Arc, atomic::*};

use ::failure::ResultExt;
use ::futures::stream::unfold;
use ::tokio::prelude::*;

use ::linux_network::{*, futures, futures::*};
use ::send_box::SendBox;

use ::broadcast::*;
use ::config::*;
use ::errors::{Error, Result};
use ::packet::*;
use ::util::*;

type StreamE<T> = dyn(Stream<Item = T, Error = ::failure::Error>);

pub struct Server {
    recv_sock: futures::IPv6PacketSocketAdapter,
    send_sock: futures::IPv6RawSocketAdapter,
    input: SendBox<StreamE<(Solicitation, Arc<PrefixConfig>)>>,
    quit: Receiver<::QuitKind>,
    drop_allmulti: DropAllmulti,
    ifname: String,
    queued_sends: Arc<AtomicUsize>,
    max_queued: usize,
    stats: Arc<PacketStats>
}

struct PacketStats {
    packets_received: AtomicUsize,
    packets_sent: AtomicUsize
}

gen_boolean_enum!(DropAllmulti);

impl Server {
    #[allow(clippy::cast_sign_loss)]
    pub fn new(
        ifc: &InterfaceConfig,
        quit: Receiver<::QuitKind>
    ) -> Result<Self> {
        for i in &ifc.prefixes {
            if !i.reply_unconditionally {
                unimplemented!();
            }
        }

        let (recv_sock, drop_allmulti) = Self::setup_recv_socket(ifc)?;
        let send_sock = Self::setup_send_socket(ifc)?;

        let mtu = get_interface_mtu(&recv_sock, &ifc.name)? as usize;
        let prefixes = ifc.prefixes.clone();

        let stats = Arc::new(PacketStats::new());

        let input = Self::make_input_stream(
                recv_sock.clone(),
                mtu,
                prefixes.clone(),
                ifc.name.clone(),
                stats.clone()
            );

        Ok(Self {
            recv_sock,
            send_sock,
            input: unsafe { SendBox::new(Box::new(input)) },
            quit,
            drop_allmulti,
            ifname: ifc.name.clone(),
            queued_sends: Arc::new(AtomicUsize::new(0)),
            max_queued: ifc.max_queued,
            stats
        })
    }

    fn setup_recv_socket(
        ifc: &InterfaceConfig
    ) -> Result<(futures::IPv6PacketSocketAdapter, DropAllmulti)> {
        let recv_sock_raw = IPv6PacketSocket::new(
            ::linux_network::raw::ETHERTYPE_IPV6,
            SockFlag::empty(),
            &ifc.name
        )?;
        debug!("created a packet socket for interface {}", ifc.name);

        let mut recv_sock = futures::IPv6PacketSocketAdapter::new(
            &::tokio::reactor::Handle::current(),
            recv_sock_raw
        )?;
        debug!(
            "registered the packet socket for interface {} in the reactor",
            ifc.name
        );

        recv_sock.setsockopt(&SockOpts::DontRoute::new(&true))?;

        let filter = Self::create_filter();
        recv_sock.setsockopt(&SockOpts::AttachFilter::new(filter.get()))?;
        recv_sock.setsockopt(&SockOpts::LockFilter::new(&true))?;

        debug!(
            "packet filtration set on the packet socket for interface {}",
            ifc.name
        );

        let drop_allmulti = !recv_sock.set_allmulti(true, &ifc.name)?;
        debug!("ensured allmulti is set on interface {}", ifc.name);

        Ok((recv_sock, drop_allmulti.into()))
    }

    fn setup_send_socket(
        ifc: &InterfaceConfig
    ) -> Result<futures::IPv6RawSocketAdapter> {
        let send_sock_raw = IPv6RawSocket::new(
            IpProto::IcmpV6.repr(),
            SockFlag::empty()
        )?;
        debug!("created a raw socket for interface {}", ifc.name);

        let mut send_sock = futures::IPv6RawSocketAdapter::new(
            &::tokio::reactor::Handle::current(),
            send_sock_raw
        )?;
        debug!(
            "registered the raw socket for interface {} in the reactor",
            ifc.name
        );

        send_sock.setsockopt(&SockOpts::BindToDevice::new(&ifc.name))?;
        debug!("bound the raw socket to interface {}", ifc.name);

        let filter = icmp6_filter::new();
        send_sock.setsockopt(&SockOpts::IcmpV6Filter::new(&filter))?;
        debug!(
            "set icmpv6 filter on the raw socket for interface {}",
            ifc.name
        );

        send_sock.setsockopt(&SockOpts::DontRoute::new(&true))?;
        send_sock.setsockopt(&SockOpts::UnicastHops::new(&255))?;
        send_sock.setsockopt(&SockOpts::V6MtuDiscover::new(&V6PmtuType::Do))?;

        Ok(send_sock)
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

            bpf_stmt!(B::RET | B::K, u32::max_value());

            bpf_stmt!(B::RET | B::K, 0);
        )
    }

    fn make_input_stream(
        sock: IPv6PacketSocketAdapter,
        mtu: usize,
        prefixes: Vec<Arc<PrefixConfig>>,
        if_name: impl AsRef<str>,
        stats: Arc<PacketStats>
    ) -> impl Stream<
        Item = (Solicitation, Arc<PrefixConfig>),
        Error = ::failure::Error
    > {
        let if_name_clone = if_name.as_ref().to_string();

        unfold((sock, mtu), move |(mut sock, mtu)| {
            Some(sock.recvpacket(mtu, RecvFlags::empty())
                .map(move |x| (x, (sock, mtu)))
                .map_err(|e| e.into())
            )
        }).filter_map(move |(packet, _)| {
            // validate common solicitation features
            debug!("received a packet on {}", if_name.as_ref());

            stats.add_received();

            let solicit = match Solicitation::parse(&packet) {
                Some(s) => s,
                None => return None
            };

            // TODO: use source macaddr to validate the multicast macs
            // to correspond to the ipv6 multicast address

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

                prefix = Some(i.clone());
                break;
            }

            match prefix {
                Some(p) => Some((solicit, p)),
                None => None
            }
        }).filter_map(move |(solicit, prefix_conf)| {
            // validate type-specific solicitation features
            debug!(
                "the packet received on {} is generally valid",
                if_name_clone
            );

            let is_ll_res = Self::validate_ll_address_resolution(
                &solicit,
                &prefix_conf
            );
            let is_nud = Self::validate_neighbor_unreachability_detection(
                &solicit,
                &prefix_conf
            );
            if !is_ll_res && !is_nud {
                return None;
            }

            Some((solicit, prefix_conf))
        })
    }

    fn validate_ll_address_resolution(
        solicit: &Solicitation,
        prefix_conf: &PrefixConfig
    ) -> bool {
        if !is_solicited_node_multicast(&solicit.dst)
                || solicit.ll_addr_opt.is_none() {
            return false;
        }

        if prefix_conf.prefix.netmask() > 104 {
            let n_mask = prefix_conf.prefix.netmask();
            let get_bits = |addr: &Ipv6Addr| -> u32 {
                let s = addr.segments();
                let last_bits = ((u32::from(s[6]) & 0xff) << 16)
                    | u32::from(s[7]);
                last_bits >> (128 - n_mask)
            };

            let dst_bits = get_bits(&solicit.dst);
            let prefix_bits = get_bits(
                &prefix_conf.prefix.network_address()
            );
            if dst_bits != prefix_bits {
                return false;
            }
        }

        true
    }

    fn validate_neighbor_unreachability_detection(
        solicit: &Solicitation,
        prefix_conf: &PrefixConfig
    ) -> bool {
        if is_solicited_node_multicast(&solicit.dst)
                || !prefix_conf.prefix.contains(solicit.dst) {
            return false;
        }
        true
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.drop_allmulti.into() {
            ::util::log_if_err(
                self.recv_sock.set_allmulti(false, &self.ifname)
                    .context("error returning allmilti flag to the previous \
                        state")
                    .map_err(|e| e.into())
            );
        }

        info!(
            "received {} and sent {} packets on interface {}",
            self.stats.get_received(),
            self.stats.get_sent(),
            self.ifname
        );
    }
}

impl Future for Server {
    type Item = ();
    type Error = ();

    #[allow(clippy::cast_sign_loss)]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        debug!("waiting for a solicitation");

        let mut active = true;
        while active {
            active = false;

            if let Async::Ready(qk) = self.quit.poll()
                    .map_err(|e| log_err(e.into()))? {
                debug!("received a signal, quitting");
                // active = true;
                match qk.expect("a quit signal") {
                    // the distinction will be important when implementing
                    // querying the target network's interface
                    // currently queued packets are purposefully omitted
                    ::QuitKind::Fast | ::QuitKind::Normal =>
                        return Ok(Async::Ready(()))
                }
            }

            if let Async::Ready(Some((solicit, prefix_conf)))
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
                    ll_addr_opt: Some(self.recv_sock.get_interface_mac())
                };
                let adv_packet = adv.solicited_to_packet(
                    prefix_conf.override_flag,
                    prefix_conf.router_flag
                );

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

                let dst = SocketAddrV6::new(
                    solicit.src,
                    0,
                    0,
                    self.recv_sock.get_interface_index() as u32
                );
                ::tokio::spawn(
                    self.send_sock.sendto(
                        adv_packet,
                        dst,
                        SendFlags::empty()
                    ).map(
                        |_| ()
                    ).map_err(
                        |e| log_err(Error::LinuxNetworkError(e).into())
                    ).then(move |x| {
                        queued_sends.fetch_sub(1, Ordering::Relaxed);
                        x
                    })
                );
                self.stats.add_sent();
                debug!("advertisement queued on {}", self.ifname);
            }
        }

        Ok(Async::NotReady)
    }
}

impl PacketStats {
    fn new() -> Self {
        Self {
            packets_received: AtomicUsize::new(0),
            packets_sent: AtomicUsize::new(0)
        }
    }

    fn add_received(&self) {
        self.packets_received.fetch_add(1, Ordering::Relaxed);
    }

    fn add_sent(&self) {
        self.packets_sent.fetch_add(1, Ordering::Relaxed);
    }

    fn get_received(&self) -> usize {
        self.packets_received.load(Ordering::Relaxed)
    }

    fn get_sent(&self) -> usize {
        self.packets_sent.load(Ordering::Relaxed)
    }
}
