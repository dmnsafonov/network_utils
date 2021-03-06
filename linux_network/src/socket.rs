use ::std::ffi::*;
use ::std::mem::*;
use ::std::net::*;
use ::std::os::unix::prelude::*;

use ::nlibc::*;
use ::nix::sys::socket::{AddressFamily, SockType, socket};
use ::pnet_packet::*;
use ::pnet_packet::ipv6::*;

use ::*;
use ::errors::{Error, Result};
use ::util::*;

pub use ::nix::sys::socket::SockFlag;

pub struct IPv6RawSocket(IPv6RawSocketImpl);
struct IPv6RawSocketImpl(RawFd);

impl IPv6RawSocket {
    pub fn new(proto: c_int, flags: SockFlag) -> Result<Self> {
        Ok(
            IPv6RawSocket(IPv6RawSocketImpl(
                socket(
                    AddressFamily::Inet6,
                    SockType::Raw,
                    flags,
                    proto
                )?
            ))
        )
    }

    pub fn bind(&mut self, addr: SocketAddrV6) -> Result<()> {
        self.0.bind(addr)
    }

    pub fn recvfrom<'a>(
        &mut self,
        buf: &'a mut [u8],
        flags: RecvFlags
    ) -> Result<(&'a mut [u8], SocketAddrV6)> {
        self.0.recvfrom(buf, flags)
    }

    pub fn sendto(
        &mut self,
        buf: &[u8],
        addr: SocketAddrV6,
        flags: SendFlags
    ) -> Result<size_t> {
        self.0.sendto(buf, addr, flags)
    }
}

#[allow(clippy::cast_possible_truncation)]
impl IPv6RawSocketImpl {
    fn bind(&mut self, addr: SocketAddrV6) -> Result<()> { unsafe {
        let addr_in = make_sockaddr_in6(addr);
        n1try!(bind(
            self.0,
            as_sockaddr(&addr_in),
            size_of_val(&addr_in) as socklen_t
        ));
        Ok(())
    }}

    #[allow(clippy::cast_sign_loss)]
    fn recvfrom<'a>(
        &mut self,
        buf: &'a mut [u8],
        flags: RecvFlags
    ) -> Result<(&'a mut [u8], SocketAddrV6)> { unsafe {
        let mut addr: sockaddr_in6 = zeroed();

        let mut addr_size = size_of_val(&addr) as socklen_t;
        let size = n1try!(::nlibc::recvfrom(
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

    #[allow(clippy::cast_sign_loss)]
    fn sendto(
        &mut self,
        buf: &[u8],
        addr: SocketAddrV6,
        flags: SendFlags
    ) -> Result<size_t> { unsafe {
        let addr_in = make_sockaddr_in6(addr);
        let addr_size = size_of_val(&addr_in) as socklen_t;

        Ok(n1try!(::nlibc::sendto(
            self.0,
            ref_to_cvoid(buf),
            buf.len() as size_t,
            flags.bits(),
            as_sockaddr(&addr_in),
            addr_size)) as size_t
        )
    }}
}

impl Drop for IPv6RawSocket {
    fn drop(&mut self) {
        log_if_err(::nix::unistd::close(self.as_raw_fd())
            .map_err(|e| e.into()));
    }
}

impl AsRawFd for IPv6RawSocketImpl {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

impl AsRawFd for IPv6RawSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
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

pub struct IPv6PacketSocket(IPv6PacketSocketImpl);
struct IPv6PacketSocketImpl {
    fd: RawFd,
    if_index: c_int,
    macaddr: MacAddr,
    proto: c_ushort
}

impl IPv6PacketSocket {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    pub fn new<T>(proto: u16, flags: SockFlag, if_name: T)
            -> Result<Self> where
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

        let proto = proto.to_be() as c_ushort;

        let sock = socket(
            AddressFamily::Packet,
            SockType::Datagram,
            flags,
            c_int::from(proto)
        )?;

        let mut ret = IPv6PacketSocketImpl {
            fd: sock,
            if_index: -1,
            macaddr: if_addr,
            proto
        };
        ret.if_index = get_interface_index(&ret, name)?;

        unsafe {
            let mut addr: sockaddr_ll = zeroed();
            addr.sll_family = AF_PACKET as c_ushort;
            addr.sll_protocol = proto;
            addr.sll_ifindex = ret.if_index;
            n1try!(bind(
                sock,
                as_sockaddr(&addr),
                size_of_val(&addr) as socklen_t
            ));
        }

        Ok(IPv6PacketSocket(ret))
    }

    pub fn recvpacket(
        &mut self,
        maxsize: size_t,
        flags: RecvFlags
    ) -> Result<(Ipv6, MacAddr)> {
        self.0.recvpacket(maxsize, flags)
    }

    pub fn sendpacket(
            &mut self,
            packet: &Ipv6,
            dest: Option<MacAddr>,
            flags: SendFlags
    ) -> Result<size_t> {
        self.0.sendpacket(packet, dest, flags)
    }

    pub fn get_interface_index(&self) -> c_int {
        self.0.get_interface_index()
    }

    pub fn get_interface_mac(&self) -> MacAddr {
        self.0.get_interface_mac()
    }
}

impl IPv6PacketSocketImpl {
    #[allow(clippy::cast_possible_truncation)]
    fn recvpacket(
        &mut self,
        maxsize: size_t,
        flags: RecvFlags
    ) -> Result<(Ipv6, MacAddr)> { unsafe {
        let mut packet = MutableIpv6Packet::owned(vec![0; maxsize])
            .ok_or(Error::BufferTooSmall {
                len: maxsize as usize
            })?;

        let mut addr: sockaddr_ll = zeroed();
        let mut addr_size = size_of_val(&addr) as socklen_t;
        n1try!(::nlibc::recvfrom(
            self.fd,
            ref_to_mut_cvoid(packet.packet_mut()),
            maxsize,
            flags.bits(),
            as_sockaddr_mut(&mut addr),
            &mut addr_size
        ));

        let mac = MacAddr::from_bytes(
            &addr.sll_addr[0..addr.sll_halen as usize]
        )?;

        Ok((packet.from_packet(), mac))
    }}

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn sendpacket(
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
        addr_ll.sll_protocol = self.proto;
        addr_ll.sll_ifindex = self.if_index;
        addr_ll.sll_halen = 6;
        addr_ll.sll_addr[0..6].copy_from_slice(
            dest.as_ref().unwrap_or(&self.macaddr).as_bytes()
        );

        Ok(n1try!(
            ::nlibc::sendto(
                self.fd,
                ref_to_cvoid(buf.packet()),
                len as size_t,
                flags.bits(),
                as_sockaddr(&addr_ll),
                addr_size)
            ) as size_t
        )
    }}

    fn get_interface_index(&self) -> c_int {
        self.if_index
    }

    fn get_interface_mac(&self) -> MacAddr {
        self.macaddr
    }
}

impl Drop for IPv6PacketSocket {
    fn drop(&mut self) {
        log_if_err(::nix::unistd::close(self.as_raw_fd())
            .map_err(|e| e.into()));
    }
}

impl AsRawFd for IPv6PacketSocketImpl {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsRawFd for IPv6PacketSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

pub trait SocketCommon where
        Self: AsRawFd + Sized {
    fn setsockopt<'a, T: SetSockOpt<'a>>(&mut self, opt: &'a T)
            -> Result<()> {
        opt.set(&*self)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn set_allmulti<T>(&mut self, allmulti: bool, ifname: T)
            -> Result<bool>
            where T: AsRef<str> {
        let name = ifname.as_ref();
        let iff_allmulti = IFF_ALLMULTI as c_short;

        let mut flags = get_interface_flags(self as &dyn AsRawFd, name)?;
        let prev = (flags & iff_allmulti) != 0;

        if allmulti {
            flags |= iff_allmulti;
        } else {
            flags &= !iff_allmulti;
        }
        set_interface_flags(self as &dyn AsRawFd, name, flags)?;

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

impl SocketCommon for IPv6RawSocket {}
impl SocketCommon for IPv6PacketSocket {}

pub trait SetSockOpt<'a> where Self: 'a {
    type Val: ?Sized;
    fn new(val: &'a Self::Val) -> Self;
    fn set<T: SocketCommon>(&self, fd: &T) -> Result<()>;
}

#[allow(non_snake_case)]
pub mod SockOpts {
    use super::*;
    use ::raw::sock_fprog;
    use ::nlibc::c_void;

    pub trait ToSetSockOptArg<'a> where Self: 'a {
        type Owner;
        unsafe fn to_set_sock_opt_arg(
            &'a self
        ) -> Result<(Self::Owner, *const c_void, socklen_t)>;
    }

    impl<'a> ToSetSockOptArg<'a> for bool {
        type Owner = Box<c_int>;

        #[allow(clippy::cast_possible_truncation)]
        unsafe fn to_set_sock_opt_arg(
            &'a self
        ) -> Result<(Self::Owner, *const c_void, socklen_t)> {
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

        #[allow(clippy::cast_possible_truncation)]
        unsafe fn to_set_sock_opt_arg(
            &'a self
        ) -> Result<(Self::Owner, *const c_void, socklen_t)> {
            let owner = CString::new(self)?;
            let ptr = owner.as_ptr() as *const c_void;
            let len = self.len() as socklen_t + 1;
            Ok((owner, ptr, len))
        }
    }

    #[allow(clippy::use_self)]
    impl<'a> ToSetSockOptArg<'a> for c_int {
        type Owner = Box<c_int>;

        #[allow(clippy::cast_possible_truncation)]
        unsafe fn to_set_sock_opt_arg(
            &'a self
        ) -> Result<(Self::Owner, *const c_void, socklen_t)> {
            let ptr = Box::into_raw(Box::new(*self));
            Ok((
                Box::from_raw(ptr),
                ptr as *const c_void,
                size_of::<c_int>() as socklen_t
            ))
        }
    }

    impl<'a> ToSetSockOptArg<'a> for V6PmtuType {
        type Owner = Box<c_int>;

        #[allow(clippy::cast_possible_truncation)]
        unsafe fn to_set_sock_opt_arg(
            &'a self
        ) -> Result<(Self::Owner, *const c_void, socklen_t)> {
            let ptr = Box::into_raw(Box::new(self.repr()));
            Ok((
                Box::from_raw(ptr),
                ptr as *const c_void,
                size_of::<c_int>() as socklen_t
            ))
        }
    }

    macro_rules! gen_sock_opt {
        ($name:ident, $opt:expr, $typ:ty) => (
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
                    n1try!(::nlibc::setsockopt(
                        fd.as_raw_fd(),
                        $opt.get_sock_opt_level().repr(),
                        $opt.repr(),
                        ptr,
                        len
                    ));
                    Ok(())
                }}
            }
        )
    }

    macro_rules! gen_sock_opt_any_sized {
        ($name:ident, $opt:expr, $typ:ty) => (
            impl<'a> ToSetSockOptArg<'a> for $typ {
                type Owner = &'a $typ;

                #[allow(clippy::cast_possible_truncation)]
                unsafe fn to_set_sock_opt_arg(&'a self)
                        -> Result<(&'a $typ, *const c_void, socklen_t)> {
                    Ok((
                        self,
                        self as *const $typ as *const c_void,
                        size_of::<$typ>() as socklen_t
                    ))
                }
            }

            gen_sock_opt!($name, $opt, $typ);
        )
    }

    gen_sock_opt!(IpHdrIncl, SockOptIPv6::IpHdrIncl, bool);
    gen_sock_opt_any_sized!(IcmpV6Filter, SockOptICMPv6::IcmpV6Filter,
        icmp6_filter);
    gen_sock_opt!(BindToDevice, SockOptSocket::BindToDevice, str);
    gen_sock_opt!(DontRoute, SockOptSocket::DontRoute, bool);
    gen_sock_opt!(V6Only, SockOptIPv6::V6Only, bool);
    gen_sock_opt_any_sized!(AttachFilter, SockOptSocket::AttachFilter,
        sock_fprog);
    gen_sock_opt!(LockFilter, SockOptSocket::LockFilter, bool);
    gen_sock_opt!(UnicastHops, SockOptIPv6::UnicastHops, c_int);
    gen_sock_opt!(V6MtuDiscover, SockOptIPv6::V6MtuDiscover, V6PmtuType);
}

#[cfg(feature = "seccomp")]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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
    use ::std::ops::Deref;
    use ::std::sync::Arc;

    use ::bytes::*;
    use ::mio;
    use ::mio::*;
    use ::mio::event::Evented;
    use ::mio::unix::EventedFd;
    use ::spin::{Mutex as SpinMutex, MutexGuard as SpinMutexGuard};
    use ::tokio::prelude::*;
    use ::tokio::prelude::Poll;
    use ::tokio::reactor::*;

    struct PollEventedLocker<T> where T: Evented {
        poll_evented: PollEvented2<T>,
        read_lock: SpinMutex<()>,
        write_lock: SpinMutex<()>
    }

    struct PollEventedLockerGuard<'a, T> where T: 'a + Evented {
        poll_evented: &'a PollEvented2<T>,
        _guard: SpinMutexGuard<'a, ()>
    }

    impl<T> PollEventedLocker<T> where T: Evented {
        fn new(inner: PollEvented2<T>) -> Self {
            Self {
                poll_evented: inner,
                read_lock: SpinMutex::new(()),
                write_lock: SpinMutex::new(())
            }
        }

        fn lock_read(&self) -> PollEventedLockerGuard<T> {
            PollEventedLockerGuard {
                poll_evented: &self.poll_evented,
                _guard: self.read_lock.lock()
            }
        }

        fn lock_write(&self) -> PollEventedLockerGuard<T> {
            PollEventedLockerGuard {
                poll_evented: &self.poll_evented,
                _guard: self.write_lock.lock()
            }
        }
    }

    impl<'a, T> Deref for PollEventedLockerGuard<'a, T>
    where T: Evented {
        type Target = PollEvented2<T>;
        fn deref(&self) -> &Self::Target {
            self.poll_evented
        }
    }

    gen_evented_eventedfd!(IPv6RawSocket);

    #[derive(Clone)]
    pub struct IPv6RawSocketAdapter(IPv6RawSocketRef);
    type IPv6RawSocketRef = Arc<PollEventedLocker<IPv6RawSocket>>;

    unsafe impl Send for IPv6RawSocketAdapter {}
    unsafe impl Sync for IPv6RawSocketAdapter {}

    impl IPv6RawSocketAdapter {
        pub fn new(handle: &Handle, inner: IPv6RawSocket) -> Result<Self> {
            set_fd_nonblock(&inner, Nonblock::Yes)?;
            Ok(
                IPv6RawSocketAdapter(
                    Arc::new(PollEventedLocker::new(
                        PollEvented2::new_with_handle(inner, handle)?
                    ))
                )
            )
        }

        pub fn bind(&mut self, addr: SocketAddrV6) -> Result<()> {
            let fd = self.as_raw_fd();
            IPv6RawSocketImpl(fd).bind(addr)
        }

        pub fn recvfrom_direct<'a>(
            &mut self,
            buf: &'a mut [u8],
            flags: RecvFlags
        ) -> ::std::result::Result<
            (&'a mut [u8], SocketAddrV6),
            ::errors::Error
        > {
            let poll_evented = self.0.lock_read();
            let ready = Ready::readable();

            if let Async::NotReady = poll_evented.poll_read_ready(ready)
                    .map_err(Error::TokioError)? {
                return Err(make_again());
            }

            let fd = poll_evented.get_ref().as_raw_fd();
            match IPv6RawSocketImpl(fd).recvfrom(buf, flags) {
                Err(e) => {
                    let err = e.downcast::<Error>().unwrap();
                    if let Again = (&err).into() {
                        poll_evented.clear_read_ready(ready)
                            .map_err(Error::TokioError)?;
                        return Err(err);
                    }
                    let new_e: ::failure::Error = err.into();
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
        ) -> ::std::result::Result<size_t, ::errors::Error> {
            let poll_evented = self.0.lock_write();

            if let Async::NotReady = poll_evented.poll_write_ready()
                    .map_err(Error::TokioError)? {
                return Err(make_again());
            }

            let fd = poll_evented.get_ref().as_raw_fd();
            match IPv6RawSocketImpl(fd).sendto(buf, addr, flags) {
                Err(e) => {
                    let err = e.downcast::<Error>().unwrap();
                    if let Again = (&err).into() {
                        poll_evented.clear_write_ready()
                            .map_err(Error::TokioError)?;
                        return Err(err);
                    }
                    let new_e: ::failure::Error = err.into();
                    Err(Error::SocketError(new_e.compat()))
                },
                Ok(x) => Ok(x)
            }
        }

        pub fn recvfrom(
            &mut self,
            buf: BytesMut,
            flags: RecvFlags
        ) -> IPv6RawSocketRecvfromFuture {
            IPv6RawSocketRecvfromFuture::new(self.0.clone(), buf, flags)
        }

        pub fn sendto(
            &mut self,
            buf: Bytes,
            addr: SocketAddrV6,
            flags: SendFlags
        ) -> IPv6RawSocketSendtoFuture {
            IPv6RawSocketSendtoFuture::new(self.0.clone(), buf, addr, flags)
        }

    }

    impl AsRawFd for IPv6RawSocketAdapter {
        fn as_raw_fd(&self) -> RawFd {
            self.0.poll_evented.get_ref().as_raw_fd()
        }
    }

    pub struct IPv6RawSocketRecvfromFuture(
        Option<IPv6RawSocketRecvfromFutureState>
    );

    struct IPv6RawSocketRecvfromFutureState {
        sock: IPv6RawSocketRef,
        buf: BytesMut,
        flags: RecvFlags
    }

    unsafe impl Send for IPv6RawSocketRecvfromFuture {}
    unsafe impl Sync for IPv6RawSocketRecvfromFuture {}

    impl IPv6RawSocketRecvfromFuture {
        fn new(
            sock: IPv6RawSocketRef,
            buf: BytesMut,
            flags: RecvFlags
        ) -> Self {
            IPv6RawSocketRecvfromFuture(
                Some(IPv6RawSocketRecvfromFutureState {
                    sock,
                    buf,
                    flags
                })
            )
        }
    }

    impl Future for IPv6RawSocketRecvfromFuture {
        type Item = (Bytes, SocketAddrV6);
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            let (len, addr) = {
                let state = self.0.as_mut().expect("pending recvfrom future");
                let (slice, addr) = try_async_val!(
                    IPv6RawSocketAdapter(state.sock.clone())
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

    pub struct IPv6RawSocketSendtoFuture(
        Option<IPv6RawSocketSendtoFutureState>
    );

    struct IPv6RawSocketSendtoFutureState {
        sock: IPv6RawSocketRef,
        buf: Bytes,
        addr: SocketAddrV6,
        flags: SendFlags
    }

    unsafe impl Send for IPv6RawSocketSendtoFuture {}
    unsafe impl Sync for IPv6RawSocketSendtoFuture {}

    impl IPv6RawSocketSendtoFuture {
        fn new(
            sock: IPv6RawSocketRef,
            buf: Bytes,
            addr: SocketAddrV6,
            flags: SendFlags
        ) -> Self {
            IPv6RawSocketSendtoFuture(
                Some(IPv6RawSocketSendtoFutureState {
                    sock,
                    buf,
                    addr,
                    flags
                })
            )
        }
    }

    impl Future for IPv6RawSocketSendtoFuture {
        type Item = size_t;
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            let len = {
                let state = self.0.as_mut().expect("pending sendto future");
                try_async!(IPv6RawSocketAdapter(state.sock.clone())
                    .sendto_direct(&state.buf, state.addr,
                        state.flags))
            };
            self.0.take();
            len
        }
    }

    gen_evented_eventedfd!(IPv6PacketSocket);

    #[derive(Clone)]
    pub struct IPv6PacketSocketAdapter(IPv6PacketSocketRef);
    type IPv6PacketSocketRef = Arc<PollEventedLocker<IPv6PacketSocket>>;

    unsafe impl Send for IPv6PacketSocketAdapter {}
    unsafe impl Sync for IPv6PacketSocketAdapter {}

    impl IPv6PacketSocketAdapter {
        pub fn new(handle: &Handle, inner: IPv6PacketSocket)
                -> Result<Self> {
            set_fd_nonblock(&inner, Nonblock::Yes)?;
            Ok(
                IPv6PacketSocketAdapter(
                    Arc::new(PollEventedLocker::new(
                        PollEvented2::new_with_handle(inner, handle)?
                    ))
                )
            )
        }

        pub fn recvpacket_direct(
            &mut self,
            maxsize: size_t,
            flags: RecvFlags
        ) -> ::std::result::Result<
            (Ipv6, MacAddr),
            ::errors::Error
        > {
            let poll_evented = self.0.lock_read();

            let ready = Ready::readable();

            if let Async::NotReady = poll_evented.poll_read_ready(ready)
                    .map_err(Error::TokioError)? {
                return Err(make_again());
            }

            let common_sock = &poll_evented.get_ref().0;
            let mut sock = IPv6PacketSocketImpl { .. *common_sock };

            match sock.recvpacket(maxsize, flags) {
                Err(e) => {
                    let err = e.downcast::<Error>().unwrap();
                    if let Again = (&err).into() {
                        poll_evented.clear_read_ready(ready)
                            .map_err(Error::TokioError)?;
                        return Err(err);
                    }
                    let new_e: ::failure::Error = err.into();
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
        ) -> ::std::result::Result<size_t, ::errors::Error> {
            let poll_evented = self.0.lock_write();

            if let Async::NotReady = poll_evented.poll_write_ready()
                    .map_err(Error::TokioError)? {
                return Err(make_again());
            }

            let common_sock = &poll_evented.get_ref().0;
            let mut sock = IPv6PacketSocketImpl { .. *common_sock };

            match sock.sendpacket(packet, dest, flags) {
                Err(e) => {
                    let err = e.downcast::<Error>().unwrap();
                    if let Again = (&err).into() {
                        poll_evented.clear_write_ready()
                            .map_err(Error::TokioError)?;
                        return Err(err);
                    }
                    let new_e: ::failure::Error = err.into();
                    Err(Error::SocketError(new_e.compat()))
                },
                Ok(x) => Ok(x)
            }
        }

        pub fn recvpacket(&mut self, maxsize: size_t, flags: RecvFlags)
                -> IPv6PacketSocketRecvpacketFuture {
            IPv6PacketSocketRecvpacketFuture::new(
                self.0.clone(),
                maxsize,
                flags
            )
        }

        pub fn sendpacket(
            &mut self,
            packet: Ipv6,
            dest: Option<MacAddr>,
            flags: SendFlags
        ) -> IPv6PacketSocketSendpacketFuture {
            IPv6PacketSocketSendpacketFuture::new(
                self.0.clone(),
                packet,
                dest,
                flags
            )
        }

        pub fn get_interface_mac(&self) -> MacAddr {
            self.0.poll_evented.get_ref().get_interface_mac()
        }

        pub fn get_interface_index(&self) -> c_int {
            self.0.poll_evented.get_ref().get_interface_index()
        }
    }

    impl AsRawFd for IPv6PacketSocketAdapter {
        fn as_raw_fd(&self) -> RawFd {
            self.0.poll_evented.get_ref().as_raw_fd()
        }
    }

    pub struct IPv6PacketSocketRecvpacketFuture(
        Option<IPv6PacketSocketRecvpacketFutureState>
    );

    struct IPv6PacketSocketRecvpacketFutureState {
        sock: IPv6PacketSocketRef,
        maxsize: size_t,
        flags: RecvFlags
    }

    unsafe impl Send for IPv6PacketSocketRecvpacketFuture {}
    unsafe impl Sync for IPv6PacketSocketRecvpacketFuture {}

    impl IPv6PacketSocketRecvpacketFuture {
        fn new(
            sock: IPv6PacketSocketRef,
            maxsize: size_t,
            flags: RecvFlags
        ) -> Self {
            IPv6PacketSocketRecvpacketFuture(
                Some(IPv6PacketSocketRecvpacketFutureState {
                    sock,
                    maxsize,
                    flags
                })
            )
        }
    }

    impl Future for IPv6PacketSocketRecvpacketFuture {
        type Item = (Ipv6, MacAddr);
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            let ret = {
                let state = self.0.as_mut()
                    .expect("pending recvpacket future");
                try_async!(IPv6PacketSocketAdapter(state.sock.clone())
                    .recvpacket_direct(state.maxsize, state.flags))
            };
            self.0.take();
            ret
        }
    }

    pub struct IPv6PacketSocketSendpacketFuture(
        Option<IPv6PacketSocketSendpacketFutureState>
    );

    struct IPv6PacketSocketSendpacketFutureState {
        sock: IPv6PacketSocketRef,
        packet: Ipv6,
        destination: Option<MacAddr>,
        flags: SendFlags
    }

    unsafe impl Send for IPv6PacketSocketSendpacketFuture {}
    unsafe impl Sync for IPv6PacketSocketSendpacketFuture {}

    impl IPv6PacketSocketSendpacketFuture {
        fn new(
            sock: IPv6PacketSocketRef,
            packet: Ipv6,
            destination: Option<MacAddr>,
            flags: SendFlags
        ) -> Self {
            IPv6PacketSocketSendpacketFuture(
                Some(IPv6PacketSocketSendpacketFutureState {
                    sock,
                    packet,
                    destination,
                    flags
                })
            )
        }
    }

    impl Future for IPv6PacketSocketSendpacketFuture {
        type Item = size_t;
        type Error = Error;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            let len = {
                let state = self.0.as_mut()
                    .expect("pending sendpacket future");
                try_async!(IPv6PacketSocketAdapter(state.sock.clone())
                    .sendpacket_direct(
                        &state.packet,
                        state.destination,
                        state.flags
                    )
                )
            };
            self.0.take();
            len
        }
    }

    impl SocketCommon for IPv6RawSocketAdapter {}
    impl SocketCommon for IPv6PacketSocketAdapter {}

    fn make_again() -> Error {
        Error::Again(io::Error::new(
            io::ErrorKind::WouldBlock,
            "request would block"
        ))
    }
}

#[allow(clippy::transmute_ptr_to_ptr)]
unsafe fn as_sockaddr<T>(x: &T) -> &sockaddr {
    transmute::<&T, &sockaddr>(x)
}

#[allow(clippy::transmute_ptr_to_ptr)]
unsafe fn as_sockaddr_mut<T>(x: &mut T) -> &mut sockaddr {
    transmute::<&mut T, &mut sockaddr>(x)
}
