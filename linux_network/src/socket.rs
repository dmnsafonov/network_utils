use ::std::ffi::*;
use ::std::mem::*;
use ::std::net::*;
use ::std::os::unix::prelude::*;
use ::std::ptr::*;

use ::libc::*;
use ::nix::sys::socket::{AddressFamily, SockType, SOCK_NONBLOCK};
use ::pnet_packet::*;
use ::pnet_packet::ip::IpNextHeaderProtocols;
use ::pnet_packet::ipv6::*;

use ::numeric_enums::*;

use ::*;
use ::errors::{Error, ErrorKind, Result};
use ::constants::raw::*;
use ::functions::raw::*;
use ::structs::raw::*;
use ::util::*;

// TODO: split to packet and raw socket
pub struct IpV6Socket {
    fd: RawFd,
    proto: c_int
}

impl IpV6Socket {
    pub fn new(family: AddressFamily, typ: SockType, proto: c_int)
            -> Result<IpV6Socket> {
        let proto_arg = match family {
            AddressFamily::Inet6 => proto,
            AddressFamily::Packet => (proto as u16).to_be() as i32,
            _ => unimplemented!()
        };

        Ok(
            IpV6Socket {
                fd: ::nix::sys::socket::socket(
                    family,
                    typ,
                    SOCK_NONBLOCK,
                    proto_arg
                )?,
                proto: proto
            }
        )
    }

    pub fn recvfrom<'a>(&mut self, buf: &'a mut [u8], flags: RecvFlagSet)
            -> Result<(&'a mut [u8], SocketAddrV6)> { unsafe {
        let mut addr: sockaddr_in6 = zeroed();

        let mut addr_size = size_of_val(&addr) as socklen_t;
        let size = n1try!(::libc::recvfrom(
            self.fd,
            ref_to_mut_cvoid(buf),
            buf.len() as size_t,
            flags.get(),
            transmute::<&mut sockaddr_in6, &mut sockaddr>(&mut addr),
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
        let mut addr_in: sockaddr_in6 = zeroed();
        let addr_size = size_of_val(&addr_in) as socklen_t;

        addr_in.sin6_family = AddressFamily::Inet6 as u16;
        addr_in.sin6_flowinfo = addr.flowinfo();
        addr_in.sin6_scope_id = addr.scope_id();

        let mut addr_raw: in6_addr = zeroed();
        addr_raw.s6_addr = addr.ip().octets();
        addr_in.sin6_addr = addr_raw;

        Ok(n1try!(::libc::sendto(
            self.fd,
            ref_to_cvoid(buf),
            buf.len() as size_t,
            flags.get(),
            transmute::<&sockaddr_in6, &sockaddr>(&addr_in),
            addr_size)) as size_t
        )
    }}

    pub fn recvpacket(&mut self, maxsize: size_t, flags: RecvFlagSet)
            -> Result<Ipv6> {
        let mut packet = MutableIpv6Packet::owned(vec![0; maxsize])
            .ok_or(ErrorKind::BufferTooSmall(maxsize))?;
        match self.proto {
            IPPROTO_IPV6 => unimplemented!(),
            _ => {
                let (len, addr) = (|(x,y): (&mut [u8],_)| (x.len(),y))
                    (self.recvfrom(packet.payload_mut(), flags)?);
                packet.set_version(6);
                packet.set_flow_label(addr.flowinfo());
                packet.set_payload_length(len as u16);
                packet.set_next_header(match self.proto {
                    IPPROTO_ICMPV6 => IpNextHeaderProtocols::Icmpv6,
                    _ => unimplemented!()
                });
                packet.set_source(*addr.ip());

                Ok(packet.from_packet())
            }
        }
    }

    pub fn sendpacket(
            &mut self,
            packet: &Ipv6,
            scope_id: u32,
            flags: SendFlagSet)
                -> Result<size_t> {
        let mut buf = MutableIpv6Packet::owned(
            vec![0; Ipv6Packet::packet_size(&packet)]).unwrap();
        buf.populate(&packet);

        Ok(self.sendto(
            buf.packet(),
            SocketAddrV6::new(
                packet.destination,
                0,
                packet.flow_label,
                scope_id
            ),
            flags
        )?)
    }
}

impl Drop for IpV6Socket {
    fn drop(&mut self) {
        log_if_err(::nix::unistd::close(self.fd).map_err(Error::from));
    }
}

impl AsRawFd for IpV6Socket {
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

                if size + 1 > raw::IFNAMSIZ as socklen_t {
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
            where T: AsRef<str> { unsafe {
        let mut ifr: ifreq = zeroed();
        let ifname_bytes = ifname.as_ref().as_bytes();

        if ifname_bytes.len() >= IFNAMSIZ {
            bail!(ErrorKind::IfNameTooLong(ifname.as_ref().to_string()));
        }
        copy_nonoverlapping(ifname_bytes.as_ptr(),
            ifr.ifr_name.as_mut_ptr() as *mut u8,
            ifname_bytes.len());

        let iff_allmulti = IFF_ALLMULTI as c_short;

        get_interface_flags(self as &AsRawFd, &mut ifr)?;
        let prev = ifr.un.ifr_flags & iff_allmulti;

        if allmulti {
            ifr.un.ifr_flags |= iff_allmulti;
        } else {
            ifr.un.ifr_flags &= !iff_allmulti;
        }
        set_interface_flags(self as &AsRawFd, &mut ifr)?;

        Ok(prev != 0)
    }}
}

impl SocketCommon for IpV6Socket {}
