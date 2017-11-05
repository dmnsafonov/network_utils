use ::std::ffi::*;
use ::std::mem::*;
use ::std::net::*;
use ::std::os::unix::prelude::*;
use ::std::ptr::*;

use ::libc::*;
use ::nix::sys::socket::{AddressFamily, SockType, SOCK_NONBLOCK};

use ::numeric_enums::*;

use ::*;
use ::errors::{Error, ErrorKind, Result};
use ::constants::raw::*;
use ::functions::raw::*;
use ::structs::raw::*;
use ::util::*;

pub struct IpV6Socket(RawFd);

impl IpV6Socket {
    pub fn new(family: AddressFamily, typ: SockType, proto: c_int) -> Result<IpV6Socket> {
        let proto_arg = match family {
            AddressFamily::Inet6 => proto,
            AddressFamily::Packet => (proto as u16).to_be() as i32,
            _ => unimplemented!()
        };

        Ok(
            IpV6Socket(
                ::nix::sys::socket::socket(
                    family,
                    typ,
                    SOCK_NONBLOCK,
                    proto_arg
                )?
            )
        )
    }

    pub fn setsockopt(&mut self, level: SockOptLevel, opt: &SockOpt)
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
            self.0,
            level.to_num(),
            opt.to_num(),
            arg,
            size
        ));

        Ok(())
    }}

    pub fn set_allmulti<T>(&mut self, allmulti: bool, ifname: T) -> Result<bool>
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

        get_interface_flags(self, &mut ifr)?;
        let prev = ifr.un.ifr_flags & iff_allmulti;

        if allmulti {
            ifr.un.ifr_flags |= iff_allmulti;
        } else {
            ifr.un.ifr_flags &= !iff_allmulti;
        }
        set_interface_flags(self, &mut ifr)?;

        Ok(prev != 0)
    }}

    pub fn recvfrom<'a>(&mut self, buf: &'a mut [u8], flags: SendRcvFlagSet)
            -> Result<(&'a mut [u8], SocketAddrV6)> { unsafe {
        let mut addr: sockaddr_in6 = zeroed();

        let mut addr_size = size_of_val(&addr) as socklen_t;
        let size = n1try!(::libc::recvfrom(
            self.0,
            ref_to_mut_cvoid(buf),
            buf.len(),
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
}

impl Drop for IpV6Socket {
    fn drop(&mut self) {
        log_if_err(::nix::unistd::close(self.0).map_err(Error::from));
    }
}

impl AsRawFd for IpV6Socket {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}
