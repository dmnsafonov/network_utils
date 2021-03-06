use ::std::fmt::*;

use ::nlibc::*;

use ::errors::{Error, Result};
use ::*;

pub mod raw {
    use super::*;

    #[repr(C)]
    pub struct ifreq {
        pub ifr_name: [c_char; IFNAMSIZ],
        pub un: ifreq_un
    }

    #[repr(C)]
    pub union ifreq_un {
        pub ifr_addr: sockaddr,
        pub ifr_dstaddr: sockaddr,
        pub ifr_broadaddr: sockaddr,
        pub ifr_netmask: sockaddr,
        pub ifr_hwaddr: sockaddr,
        pub ifr_flags: c_short,
        pub ifr_ifindex: c_int,
        pub ifr_metric: c_int,
        pub ifr_mtu: c_int,
        pub ifr_map: ifmap,
        pub ifr_slave: [c_char; IFNAMSIZ],
        pub ifr_newname: [c_char; IFNAMSIZ],
        pub ifr_data: *mut c_char
    }

    #[derive(Clone, Copy)] // issue #32836 of rust-lang
    #[repr(C)]
    pub union ifmap {
        pub mem_start: c_ulong,
        pub mem_end: c_ulong,
        pub base_addr: c_ushort,
        pub irq: c_uchar,
        pub dma: c_uchar,
        pub port: c_uchar
    }

    #[repr(C)]
    pub struct sock_fprog {
        pub len: c_ushort,
        pub filter: *mut sock_filter
    }

    #[derive(Clone, Copy, Debug)]
    #[repr(C)]
    pub struct sock_filter {
        pub code: u16,
        pub jt: u8,
        pub jf: u8,
        pub k: u32
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct icmp6_filter {
    icmp6_filt: [uint32_t; 8]
}

impl icmp6_filter {
    pub fn new() -> Self {
        Self { icmp6_filt: [0xffff_ffff; 8] }
    }

    pub fn new_pass() -> Self {
        Self { icmp6_filt: [0; 8] }
    }

    pub fn pass(&mut self, icmp_type: IcmpV6Type) {
        let tp = icmp_type.repr();
        self.icmp6_filt[tp as usize >> 5] &= !(1 << (tp & 31));
    }

    pub fn block(&mut self, icmp_type: IcmpV6Type) {
        let tp = icmp_type.repr();
        self.icmp6_filt[tp as usize >> 5] |= 1 << (tp & 31);
    }
}

impl Default for icmp6_filter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct MacAddr([u8; 6]);

impl MacAddr {
    #[allow(clippy::many_single_char_names)]
    pub fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> Self {
        MacAddr([a, b, c, d, e, f])
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0[..]
    }

    pub fn from_bytes<T>(x: T) -> Result<Self> where T: AsRef<[u8]> {
        let s = x.as_ref();
        if s.len() != 6 {
            return Err(Error::WrongSize.into());
        }

        let mut arr = [0; 6];
        arr.copy_from_slice(s);
        Ok(MacAddr(arr))
    }
}

impl Debug for MacAddr {
    fn fmt(&self, f: &mut Formatter) -> ::std::fmt::Result {
        (self as &dyn Display).fmt(f)
    }
}

impl Display for MacAddr {
    fn fmt(&self, f: &mut Formatter) -> ::std::fmt::Result {
        let octets = self.0.iter()
            .map(|x| format!("{:x}", x))
            .map(|s| if s.len() == 1 {"0".to_string() + &s} else {s})
            .map(|s| ":".to_string() + &s)
            .collect::<Vec<String>>();
        let s = octets.iter()
            .flat_map(|x| x.chars())
            .skip(1)
            .collect::<String>();
        write!(f, "{}", s)
    }
}
