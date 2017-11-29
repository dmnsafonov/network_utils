use ::std::ffi::*;
use ::std::mem::*;
use ::std::net::*;
use ::std::os::unix::prelude::*;

use ::libc::*;
use ::nix::sys::socket::{AddressFamily, SockType, socket};
use ::pnet_packet::*;
use ::pnet_packet::ipv6::*;

use ::numeric_enums::*;

use ::*;
use ::errors::{Error, ErrorKind, Result, ResultExt};
use ::raw::*;
use ::util::*;

pub use ::nix::sys::socket::SockFlag;

pub struct IpV6RawSocket(RawFd);

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

    pub fn recvfrom<'a>(&mut self, buf: &'a mut [u8], flags: RecvFlagSet)
            -> Result<(&'a mut [u8], SocketAddrV6)> { unsafe {
        let mut addr: sockaddr_in6 = zeroed();

        let mut addr_size = size_of_val(&addr) as socklen_t;
        let size = n1try!(::libc::recvfrom(
            self.0,
            ref_to_mut_cvoid(buf),
            buf.len() as size_t,
            flags.get(),
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
        flags: SendFlagSet
    ) -> Result<size_t> { unsafe {
        let addr_in = make_sockaddr_in6(addr);
        let addr_size = size_of_val(&addr_in) as socklen_t;

        Ok(n1try!(::libc::sendto(
            self.0,
            ref_to_cvoid(buf),
            buf.len() as size_t,
            flags.get(),
            as_sockaddr(&addr_in),
            addr_size)) as size_t
        )
    }}
}

impl Drop for IpV6RawSocket {
    fn drop(&mut self) {
        log_if_err(::nix::unistd::close(self.0).map_err(Error::from));
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

impl IpV6PacketSocket {
    pub fn new<T>(proto: c_int, flags: SockFlag, if_name: T)
            -> Result<IpV6PacketSocket> where
            T: AsRef<str> {
        let name = if_name.as_ref();
        let err = || ErrorKind::NoInterface(name.to_string());
        let iface = ::interfaces::Interface::get_by_name(name)
            .chain_err(&err)?
            .ok_or(err())?;
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

    pub fn recvpacket(&mut self, maxsize: size_t, flags: RecvFlagSet)
            -> Result<(Ipv6, MacAddr)> { unsafe {
        let mut packet = MutableIpv6Packet::owned(vec![0; maxsize])
            .ok_or(ErrorKind::BufferTooSmall(maxsize))?;

        let mut addr: sockaddr_ll = zeroed();
        let mut addr_size = size_of_val(&addr) as socklen_t;
        n1try!(::libc::recvfrom(
            self.fd,
            ref_to_mut_cvoid(packet.packet_mut()),
            maxsize,
            flags.get(),
            as_sockaddr_mut(&mut addr),
            &mut addr_size
        ));

        Ok((packet.from_packet(), MacAddr::from_bytes(addr.sll_addr)?))
    }}

    pub fn sendpacket(
            &mut self,
            packet: &Ipv6,
            dest: Option<MacAddr>,
            flags: SendFlagSet)
                -> Result<size_t> { unsafe {
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
            dest.unwrap_or(self.macaddr).as_bytes()
        );

        Ok(n1try!(::libc::sendto(
            self.fd,
            ref_to_cvoid(buf.packet()),
            len as size_t,
            flags.get(),
            as_sockaddr(&addr_ll),
            addr_size)) as size_t
        )
    }}
}

impl Drop for IpV6PacketSocket {
    fn drop(&mut self) {
        log_if_err(::nix::unistd::close(self.fd).map_err(Error::from));
    }
}

impl AsRawFd for IpV6PacketSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

pub trait SocketCommon where
        Self: AsRawFd + Sized {
    fn setsockopt(&mut self, level: SockOptLevel, opt: &SockOpt)
            -> Result<()> { unsafe {
        let cint_arg: c_int;
        let cstring_arg: CString;
        let arg: *const c_void;
        let size: socklen_t;

        macro_rules! bool_opt {
            ( $flag:ident ) => ({
                cint_arg = $flag as c_int;
                arg = ref_to_cvoid(&cint_arg);
                size = size_of_val(&cint_arg) as socklen_t;
            })
        }

        macro_rules! struct_opt {
            ( $struct_ref:ident ) => ({
                arg = ref_to_cvoid($struct_ref);
                size = size_of_val($struct_ref) as socklen_t;
            })
        }

        macro_rules! string_opt {
            ( $str:ident ) => ({
                cstring_arg = CString::new($str)?;
                arg = cstring_arg.as_ptr() as *const c_void;
                size = ($str.len() + 1) as socklen_t;

                if size + 1 > IFNAMSIZ as socklen_t {
                    bail!(ErrorKind::IfNameTooLong($str.to_string()));
                }
            })
        }

        match opt {
            &SockOpt::IpHdrIncl(f) => bool_opt!(f),
            &SockOpt::IcmpV6Filter(filter) => struct_opt!(filter),
            &SockOpt::BindToDevice(str) => string_opt!(str),
            &SockOpt::DontRoute(f) => bool_opt!(f),
            &SockOpt::V6Only(f) => bool_opt!(f),
            &SockOpt::AttachFilter(filter) => struct_opt!(filter),
            &SockOpt::LockFilter(f) => bool_opt!(f)
        };

        n1try!(::libc::setsockopt(
            self.as_raw_fd(),
            level.to_num(),
            opt.to_num(),
            arg,
            size
        ));

       Ok(())
    }}

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

unsafe fn as_sockaddr<T>(x: &T) -> &sockaddr {
    transmute::<&T, &sockaddr>(x)
}

unsafe fn as_sockaddr_mut<T>(x: &mut T) -> &mut sockaddr {
    transmute::<&mut T, &mut sockaddr>(x)
}
