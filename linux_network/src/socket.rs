use ::std::ffi::*;
use ::std::mem::*;
use ::std::net::*;
use ::std::os::unix::prelude::*;

use ::libc::*;
use ::nix::sys::socket::{AddressFamily, SockType, socket};
use ::pnet_packet::*;
use ::pnet_packet::ipv6::*;

use ::*;
use ::errors::{Error, Result};
use ::util::*;

pub use ::nix::sys::socket::SockFlag;

pub struct IpV6RawSocket(RawFd);
unsafe impl Sync for IpV6RawSocket {}

impl IpV6RawSocket {
    pub fn new(proto: c_int, flags: SockFlag)
            -> Result<IpV6RawSocket> {
        Ok(
            IpV6RawSocket(
                socket(
                    AddressFamily::Inet6,
                    SockType::Raw,
                    flags,
                    proto
                )?
            )
        )
    }

    pub fn bind(&mut self, addr: SocketAddrV6) -> Result<()> { unsafe {
        let addr_in = make_sockaddr_in6(addr);
        n1try!(bind(
            self.0,
            as_sockaddr(&addr_in),
            size_of_val(&addr_in) as socklen_t
        ));
        Ok(())
    }}

    pub fn recvfrom<'a>(&mut self, buf: &'a mut [u8], flags: RecvFlags)
            -> Result<(&'a mut [u8], SocketAddrV6)> { unsafe {
        let mut addr: sockaddr_in6 = zeroed();

        let mut addr_size = size_of_val(&addr) as socklen_t;
        let size = n1try!(::libc::recvfrom(
            self.0,
            ref_to_mut_cvoid(buf),
            buf.len() as size_t,
            flags.bits(),
            as_sockaddr_mut(&mut addr),
            &mut addr_size
        ));

        let sockaddr = SocketAddrV6::new(
            addr_from_segments(&addr.sin6_addr.s6_addr),
            in_port_t::from_be(addr.sin6_port),
            addr.sin6_flowinfo,
            addr.sin6_scope_id
        );

        Ok((&mut buf[..size as usize], sockaddr))
    }}

    pub fn sendto(
        &mut self,
        buf: &[u8],
        addr: SocketAddrV6,
        flags: SendFlags
    ) -> Result<size_t> { unsafe {
        let addr_in = make_sockaddr_in6(addr);
        let addr_size = size_of_val(&addr_in) as socklen_t;

        Ok(n1try!(::libc::sendto(
            self.0,
            ref_to_cvoid(buf),
            buf.len() as size_t,
            flags.bits(),
            as_sockaddr(&addr_in),
            addr_size)) as size_t
        )
    }}
}

impl Drop for IpV6RawSocket {
    fn drop(&mut self) {
        log_if_err(::nix::unistd::close(self.0).map_err(|e| e.into()));
    }
}

impl AsRawFd for IpV6RawSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

fn make_sockaddr_in6(addr: SocketAddrV6) -> sockaddr_in6 { unsafe {
    let mut addr_in: sockaddr_in6 = zeroed();

    addr_in.sin6_family = AddressFamily::Inet6 as u16;
    addr_in.sin6_flowinfo = addr.flowinfo();
    addr_in.sin6_scope_id = addr.scope_id();

    let mut addr_raw: in6_addr = zeroed();
    addr_raw.s6_addr = addr.ip().octets();
    addr_in.sin6_addr = addr_raw;

    addr_in
}}

pub struct IpV6PacketSocket {
    fd: RawFd,
    if_index: c_int,
    macaddr: MacAddr,
    proto: c_int
}
unsafe impl Sync for IpV6PacketSocket {}

impl IpV6PacketSocket {
    pub fn new<T>(proto: c_int, flags: SockFlag, if_name: T)
            -> Result<IpV6PacketSocket> where
            T: AsRef<str> {
        let name = if_name.as_ref();
        let iface = ::interfaces::Interface::get_by_name(name)
            .map_err(|e| Error::GetInterfaceError {
                name: name.to_string(),
                cause: e
            })?.ok_or(Error::NoInterface {
                name: name.to_string()
            })?;
        let if_addr = MacAddr::from_bytes(iface.hardware_addr()?.as_bytes())?;

        let sock = socket(
            AddressFamily::Packet,
            SockType::Datagram,
            flags,
            proto
        )?;

        let mut ret = IpV6PacketSocket {
            fd: sock,
            if_index: -1,
            macaddr: if_addr,
            proto: proto
        };
        ret.if_index = get_interface_index(&ret, name)?;

        unsafe {
            let mut addr: sockaddr_ll = zeroed();
            addr.sll_family = AF_PACKET as c_ushort;
            addr.sll_protocol = proto as c_ushort;
            addr.sll_ifindex = ret.if_index;
            n1try!(bind(
                sock,
                as_sockaddr(&addr),
                size_of_val(&addr) as socklen_t
            ));
        }

        Ok(ret)
    }

    pub fn recvpacket(&mut self, maxsize: size_t, flags: RecvFlags)
            -> Result<(Ipv6, MacAddr)> { unsafe {
        let mut packet = MutableIpv6Packet::owned(vec![0; maxsize])
            .ok_or(Error::BufferTooSmall {
                len: maxsize as usize
            })?;

        let mut addr: sockaddr_ll = zeroed();
        let mut addr_size = size_of_val(&addr) as socklen_t;
        n1try!(::libc::recvfrom(
            self.fd,
            ref_to_mut_cvoid(packet.packet_mut()),
            maxsize,
            flags.bits(),
            as_sockaddr_mut(&mut addr),
            &mut addr_size
        ));

        Ok((packet.from_packet(), MacAddr::from_bytes(addr.sll_addr)?))
    }}

    pub fn sendpacket(
            &mut self,
            packet: &Ipv6,
            dest: Option<MacAddr>,
            flags: SendFlags
    ) -> Result<size_t> { unsafe {
        let len = Ipv6Packet::packet_size(&packet);
        let mut buf = MutableIpv6Packet::owned(
            vec![0; len]).unwrap();
        buf.populate(&packet);

        let mut addr_ll: sockaddr_ll = zeroed();
        let addr_size = size_of_val(&addr_ll) as socklen_t;

        addr_ll.sll_family = AF_PACKET as c_ushort;
        addr_ll.sll_protocol = self.proto as c_ushort;
        addr_ll.sll_ifindex = self.if_index;
        addr_ll.sll_halen = 6;
        addr_ll.sll_addr[0..6].copy_from_slice(
            dest.as_ref().unwrap_or(&self.macaddr).as_bytes()
        );

        Ok(n1try!(::libc::sendto(
            self.fd,
            ref_to_cvoid(buf.packet()),
            len as size_t,
            flags.bits(),
            as_sockaddr(&addr_ll),
            addr_size)) as size_t
        )
    }}

    pub fn get_mac(&self) -> MacAddr {
        self.macaddr.clone()
    }
}

impl Drop for IpV6PacketSocket {
    fn drop(&mut self) {
        log_if_err(::nix::unistd::close(self.fd).map_err(|e| e.into()));
    }
}

impl AsRawFd for IpV6PacketSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

pub trait SocketCommon where
        Self: AsRawFd + Sized {
    fn setsockopt<'a, T: SetSockOpt<'a>>(&mut self, opt: &'a T)
            -> Result<()> {
        opt.set(&*self)
    }

    fn set_allmulti<T>(&mut self, allmulti: bool, ifname: T)
            -> Result<bool>
            where T: AsRef<str> {
        let name = ifname.as_ref();
        let iff_allmulti = IFF_ALLMULTI as c_short;

        let mut flags = get_interface_flags(self as &AsRawFd, name)?;
        let prev = (flags & iff_allmulti) != 0;

        if allmulti {
            flags |= iff_allmulti;
        } else {
            flags &= !iff_allmulti;
        }
        set_interface_flags(self as &AsRawFd, name, flags)?;

        Ok(prev)
    }

    #[cfg(feature = "seccomp")]
    fn allow_sending(&self, ctx: &mut ::seccomp::Context) -> Result<()> {
        allow_syscall(ctx, self, SYS_sendto)
    }

    #[cfg(feature = "seccomp")]
    fn allow_receiving(&self, ctx: &mut ::seccomp::Context) -> Result<()> {
        allow_syscall(ctx, self, SYS_recvfrom)
    }
}

impl SocketCommon for IpV6RawSocket {}
impl SocketCommon for IpV6PacketSocket {}

pub trait SetSockOpt<'a> where Self: 'a {
    type Val: ?Sized;
    fn new(val: &'a Self::Val) -> Self;
    fn set<T: SocketCommon>(&self, fd: &T) -> Result<()>;
}

#[allow(non_snake_case)]
pub mod SockOpts {
    use super::*;
    use ::raw::sock_fprog;

    pub trait ToSetSockOptArg<'a> where Self: 'a {
        type Owner;
        unsafe fn to_set_sock_opt_arg(&'a self)
            -> Result<(Self::Owner, *const c_void, socklen_t)>;
    }

    impl<'a> ToSetSockOptArg<'a> for bool {
        type Owner = Box<c_int>;

        unsafe fn to_set_sock_opt_arg(&'a self)
                -> Result<(Box<c_int>, *const c_void, socklen_t)> {
            let ptr = Box::into_raw(Box::new(if *self {1} else {0}));
            Ok((
                Box::from_raw(ptr),
                ptr as *const c_void,
                size_of::<c_int>() as socklen_t
            ))
        }
    }

    impl<'a> ToSetSockOptArg<'a> for str {
        type Owner = CString;

        unsafe fn to_set_sock_opt_arg(&'a self)
                -> Result<(CString, *const c_void, socklen_t)> {
            let owner = CString::new(self)?;
            let ptr = owner.as_ptr() as *const c_void;
            let len = self.len() as socklen_t + 1;
            Ok((owner, ptr, len))
        }
    }

    macro_rules! gen_sock_opt {
        ($name:ident, $opt:expr, $level:expr, $typ:ty) => (
            pub struct $name<'a> {
                val: &'a $typ
            }

            impl<'a> SetSockOpt<'a> for $name<'a> {
                type Val = $typ;

                fn new(val: &'a $typ) -> $name<'a> {
                    $name {
                        val: val
                    }
                }

                fn set<T: SocketCommon>(&self, fd: &T)
                        -> Result<()> { unsafe {
                    let (_, ptr, len) = self.val.to_set_sock_opt_arg()?;
                    n1try!(::libc::setsockopt(
                        fd.as_raw_fd(),
                        $level.bits(),
                        $opt.bits(),
                        ptr,
                        len
                    ));
                    Ok(())
                }}
            }
        )
    }

    macro_rules! gen_sock_opt_any_sized {
        ($name:ident, $opt:expr, $level:expr, $typ:ty) => (
            impl<'a> ToSetSockOptArg<'a> for $typ {
                type Owner = &'a $typ;

                unsafe fn to_set_sock_opt_arg(&'a self)
                        -> Result<(&'a $typ, *const c_void, socklen_t)> {
                    Ok((
                        self,
                        self as *const $typ as *const c_void,
                        size_of::<$typ>() as socklen_t
                    ))
                }
            }

            gen_sock_opt!($name, $opt, $level, $typ);
        )
    }

    gen_sock_opt!(IpHdrIncl, SockOpt::IpHdrIncl, SockOptLevel::IpV6, bool);
    gen_sock_opt_any_sized!(IcmpV6Filter, SockOpt::IcmpV6Filter,
        SockOptLevel::IcmpV6, icmp6_filter);
    gen_sock_opt!(BindToDevice, SockOpt::BindToDevice, SockOptLevel::Socket,
        str);
    gen_sock_opt!(DontRoute, SockOpt::DontRoute, SockOptLevel::Socket, bool);
    gen_sock_opt!(V6Only, SockOpt::V6Only, SockOptLevel::IpV6, bool);
    gen_sock_opt_any_sized!(AttachFilter, SockOpt::AttachFilter,
        SockOptLevel::Socket, sock_fprog);
    gen_sock_opt!(LockFilter, SockOpt::LockFilter, SockOptLevel::Socket,
        bool);
}

#[cfg(feature = "seccomp")]
fn allow_syscall<T>(ctx: &mut ::seccomp::Context, fd: &T, syscall: c_long)
        -> Result<()> where T: AsRawFd {
    use ::seccomp::*;
    ctx.add_rule(
        Rule::new(
            syscall as usize,
            Compare::arg(0)
                .using(Op::Eq)
                .with(fd.as_raw_fd() as u64)
                .build()
                .unwrap(),
            Action::Allow
        )
    )?;
    Ok(())
}

#[cfg(feature = "async")]
pub mod futures {
    use super::*;

    use ::std::io;
    use ::std::sync::*;

    use ::bytes::*;
    use ::mio;
    use ::mio::*;
    use ::mio::event::Evented;
    use ::mio::unix::EventedFd;
    use ::tokio::prelude::*;
    use ::tokio::prelude::Poll;
    use ::tokio::reactor::*;

    gen_evented_eventedfd!(IpV6RawSocket);

    #[derive(Clone)]
    pub struct IpV6RawSocketAdapter(IpV6RawSocketPE);
    type IpV6RawSocketPE = Arc<Mutex<PollEvented2<IpV6RawSocket>>>;

    impl IpV6RawSocketAdapter {
        pub fn new(handle: &Handle, inner: IpV6RawSocket)
                -> Result<IpV6RawSocketAdapter> {
            set_fd_nonblock(&inner, Nonblock::Yes)?;
            Ok(
                IpV6RawSocketAdapter(
                    Arc::new(Mutex::new(
                        PollEvented2::new_with_handle(inner, handle)?
                    ))
                )
            )
        }

        pub fn bind(&mut self, addr: SocketAddrV6) -> Result<()> {
            let mut poll_evented = self.0.lock().unwrap();
            poll_evented.get_mut().bind(addr)
        }

        pub fn recvfrom_direct<'a>(
            &mut self,
            buf: &'a mut [u8],
            flags: RecvFlags
        ) -> std::result::Result<(&'a mut [u8], SocketAddrV6), Error> {
            let mut poll_evented = self.0.lock().unwrap();
            let ready = Ready::readable();

            if let Async::NotReady = poll_evented.poll_read_ready(ready)
                    .map_err(Error::TokioError)? {
                return Err(make_again());
            }

            match poll_evented.get_mut().recvfrom(buf, flags) {
                Err(e) => {
                    if let Again = e.downcast_ref::<Error>().unwrap().kind() {
                        poll_evented.clear_read_ready(ready)
                            .map_err(Error::TokioError)?;
                    }
                    let new_e: ::failure::Error = e.into();
                    Err(Error::SocketError(new_e.compat()))
                },
                Ok(x) => Ok(x)
            }
        }

        pub fn sendto_direct(
            &mut self,
            buf: &[u8],
            addr: SocketAddrV6,
            flags: SendFlags
        ) -> ::std::result::Result<size_t, Error> {
            let mut poll_evented = self.0.lock().unwrap();

            if let Async::NotReady = poll_evented.poll_write_ready()
                    .map_err(Error::TokioError)? {
                return Err(make_again());
            }

            match poll_evented.get_mut().sendto(buf, addr, flags) {
                Err(e) => {
                    if let Again = e.downcast_ref::<Error>().unwrap().kind() {
                        poll_evented.clear_write_ready()
                            .map_err(Error::TokioError)?;
                    }
                    let new_e: ::failure::Error = e.into();
                    Err(Error::SocketError(new_e.compat()))
                },
                Ok(x) => Ok(x)
            }
        }

        pub fn recvfrom(
            &mut self,
            buf: BytesMut,
            flags: RecvFlags
        ) -> IpV6RawSocketRecvfromFuture {
            IpV6RawSocketRecvfromFuture::new(self.0.clone(), buf, flags)
        }

        pub fn sendto(
            &mut self,
            buf: Bytes,
            addr: SocketAddrV6,
            flags: SendFlags
        ) -> IpV6RawSocketSendtoFuture {
            IpV6RawSocketSendtoFuture::new(self.0.clone(), buf, addr, flags)
        }

    }

    impl AsRawFd for IpV6RawSocketAdapter {
        fn as_raw_fd(&self) -> RawFd {
            let poll_evented = self.0.lock().unwrap();
            poll_evented.get_ref().as_raw_fd()
        }
    }

    pub struct IpV6RawSocketRecvfromFuture(
        Option<IpV6RawSocketRecvfromFutureState>
    );

    struct IpV6RawSocketRecvfromFutureState {
        sock: IpV6RawSocketPE,
        buf: BytesMut,
        flags: RecvFlags
    }

    impl IpV6RawSocketRecvfromFuture {
        fn new(
            sock: IpV6RawSocketPE,
            buf: BytesMut,
            flags: RecvFlags
        ) -> IpV6RawSocketRecvfromFuture {
            IpV6RawSocketRecvfromFuture(
                Some(IpV6RawSocketRecvfromFutureState {
                    sock: sock,
                    buf: buf,
                    flags: flags
                })
            )
        }
    }

    impl Future for IpV6RawSocketRecvfromFuture {
        type Item = (Bytes, SocketAddrV6);
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            let (len, addr) = {
                let state = self.0.as_mut().expect("pending recvfrom future");
                let (slice, addr) = try_async_val!(
                    IpV6RawSocketAdapter(state.sock.clone())
                        .recvfrom_direct(&mut state.buf, state.flags)
                );
                (slice.len(), addr)
            };

            let mut state = self.0.take().unwrap();
            state.buf.truncate(len);
            let data = state.buf.freeze();

            Ok(Async::Ready((
                data,
                addr
            )))
        }
    }

    pub struct IpV6RawSocketSendtoFuture(
        Option<IpV6RawSocketSendtoFutureState>
    );

    struct IpV6RawSocketSendtoFutureState {
        sock: IpV6RawSocketPE,
        buf: Bytes,
        addr: SocketAddrV6,
        flags: SendFlags
    }

    impl IpV6RawSocketSendtoFuture {
        fn new(
            sock: IpV6RawSocketPE,
            buf: Bytes,
            addr: SocketAddrV6,
            flags: SendFlags
        ) -> IpV6RawSocketSendtoFuture {
            IpV6RawSocketSendtoFuture(
                Some(IpV6RawSocketSendtoFutureState {
                    sock: sock,
                    buf: buf,
                    addr: addr,
                    flags: flags
                })
            )
        }
    }

    impl Future for IpV6RawSocketSendtoFuture {
        type Item = size_t;
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            let len = {
                let state = self.0.as_mut().expect("pending sendto future");
                try_async!(IpV6RawSocketAdapter(state.sock.clone())
                    .sendto_direct(&mut state.buf, state.addr,
                        state.flags))
            };
            self.0.take();
            len
        }
    }

    gen_evented_eventedfd!(IpV6PacketSocket);

    #[derive(Clone)]
    pub struct IpV6PacketSocketAdapter(IpV6PacketSocketPE);
    type IpV6PacketSocketPE = Arc<Mutex<PollEvented2<IpV6PacketSocket>>>;

    impl IpV6PacketSocketAdapter {
        pub fn new(handle: &Handle, inner: IpV6PacketSocket)
                -> Result<IpV6PacketSocketAdapter> {
            set_fd_nonblock(&inner, Nonblock::Yes)?;
            Ok(
                IpV6PacketSocketAdapter(
                    Arc::new(Mutex::new(
                        PollEvented2::new_with_handle(inner, handle)?
                    ))
                )
            )
        }

        pub fn recvpacket_direct(
            &mut self,
            maxsize: size_t,
            flags: RecvFlags
        ) -> ::std::result::Result<(Ipv6, MacAddr), Error> {
            let mut poll_evented = self.0.lock().unwrap();
            let ready = Ready::readable();

            if let Async::NotReady = poll_evented.poll_read_ready(ready)
                    .map_err(Error::TokioError)? {
                return Err(make_again());
            }

            match poll_evented.get_mut().recvpacket(maxsize, flags) {
                Err(e) => {
                    if let Again = e.downcast_ref::<Error>().unwrap().kind() {
                        poll_evented.clear_read_ready(ready)
                            .map_err(Error::TokioError)?;
                    }
                    let new_e: ::failure::Error = e.into();
                    Err(Error::SocketError(new_e.compat()))
                },
                Ok(x) => Ok(x)
            }
        }

        pub fn sendpacket_direct(
            &mut self,
            packet: &Ipv6,
            dest: Option<MacAddr>,
            flags: SendFlags
        ) -> ::std::result::Result<size_t, Error> {
            let mut poll_evented = self.0.lock().unwrap();

            if let Async::NotReady = poll_evented.poll_write_ready()
                    .map_err(Error::TokioError)? {
                return Err(make_again());
            }

            match poll_evented.get_mut().sendpacket(packet, dest, flags) {
                Err(e) => {
                    if let Again = e.downcast_ref::<Error>().unwrap().kind() {
                        poll_evented.clear_write_ready()
                            .map_err(Error::TokioError)?;
                    }
                    let new_e: ::failure::Error = e.into();
                    Err(Error::SocketError(new_e.compat()))
                },
                Ok(x) => Ok(x)
            }
        }

        pub fn recvpacket(&mut self, maxsize: size_t, flags: RecvFlags)
                -> IpV6PacketSocketRecvpacketFuture {
            IpV6PacketSocketRecvpacketFuture::new(
                self.0.clone(),
                maxsize,
                flags
            )
        }

        pub fn sendpacket<'a, 'b>(
            &'a mut self,
            packet: &'b Ipv6,
            dest: Option<MacAddr>,
            flags: SendFlags
        ) -> IpV6PacketSocketSendpacketFuture<'b> where 'a: 'b {
            IpV6PacketSocketSendpacketFuture::new(
                self.0.clone(),
                packet,
                dest,
                flags
            )
        }

        pub fn get_mac(&self) -> MacAddr {
            self.0.lock().unwrap().get_ref().get_mac()
        }
    }

    impl AsRawFd for IpV6PacketSocketAdapter {
        fn as_raw_fd(&self) -> RawFd {
            let poll_evented = self.0.lock().unwrap();
            poll_evented.get_ref().as_raw_fd()
        }
    }

    pub struct IpV6PacketSocketRecvpacketFuture(
        Option<IpV6PacketSocketRecvpacketFutureState>
    );

    struct IpV6PacketSocketRecvpacketFutureState {
        sock: IpV6PacketSocketPE,
        maxsize: size_t,
        flags: RecvFlags
    }

    impl IpV6PacketSocketRecvpacketFuture {
        fn new(
            sock: IpV6PacketSocketPE,
            maxsize: size_t,
            flags: RecvFlags
        ) -> IpV6PacketSocketRecvpacketFuture {
            IpV6PacketSocketRecvpacketFuture(
                Some(IpV6PacketSocketRecvpacketFutureState {
                    sock: sock,
                    maxsize: maxsize,
                    flags: flags
                })
            )
        }
    }

    impl Future for IpV6PacketSocketRecvpacketFuture {
        type Item = (Ipv6, MacAddr);
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            let ret = {
                let state = self.0.as_mut()
                    .expect("pending recvpacket future");
                try_async!(IpV6PacketSocketAdapter(state.sock.clone())
                    .recvpacket_direct(state.maxsize, state.flags))
            };
            self.0.take();
            ret
        }
    }

    pub struct IpV6PacketSocketSendpacketFuture<'a>(
        Option<IpV6PacketSocketSendpacketFutureState<'a>>
    );

    struct IpV6PacketSocketSendpacketFutureState<'a> {
        sock: IpV6PacketSocketPE,
        packet: &'a Ipv6,
        destination: Option<MacAddr>,
        flags: SendFlags
    }

    impl<'a> IpV6PacketSocketSendpacketFuture<'a> {
        fn new<'b>(
            sock: IpV6PacketSocketPE,
            packet: &'a Ipv6,
            destination: Option<MacAddr>,
            flags: SendFlags
        ) -> IpV6PacketSocketSendpacketFuture<'a> where 'b: 'a {
            IpV6PacketSocketSendpacketFuture(
                Some(IpV6PacketSocketSendpacketFutureState {
                    sock: sock,
                    packet: packet,
                    destination: destination,
                    flags: flags
                })
            )
        }
    }

    impl<'a> Future for IpV6PacketSocketSendpacketFuture<'a> {
        type Item = size_t;
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            let len = {
                let state = self.0.as_mut()
                    .expect("pending sendpacket future");
                try_async!(IpV6PacketSocketAdapter(state.sock.clone())
                    .sendpacket_direct(state.packet, state.destination.clone(),
                        state.flags))
            };
            self.0.take();
            len
        }
    }

    impl SocketCommon for IpV6RawSocketAdapter {}
    impl SocketCommon for IpV6PacketSocketAdapter {}

    fn make_again() -> Error {
        Error::Again(io::Error::new(
            io::ErrorKind::WouldBlock,
            "request would block"
        ))
    }
}

unsafe fn as_sockaddr<T>(x: &T) -> &sockaddr {
    transmute::<&T, &sockaddr>(x)
}

unsafe fn as_sockaddr_mut<T>(x: &mut T) -> &mut sockaddr {
    transmute::<&mut T, &mut sockaddr>(x)
}
