use ::libc::*;

use ::numeric_enums::*;

use ::*;

pub mod raw {
    use super::*;
    use ::constants::raw::*;

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

#[derive(Debug)]
#[repr(C)]
pub struct icmp6_filter {
    icmp6_filt: [uint32_t; 8]
}

impl icmp6_filter {
    pub fn new() -> icmp6_filter {
        icmp6_filter { icmp6_filt: [0xff; 8] }
    }

    pub fn new_pass() -> icmp6_filter {
        icmp6_filter { icmp6_filt: [0x00; 8] }
    }

    pub fn pass(&mut self, icmp_type: IcmpV6Type) {
        let tp = icmp_type.to_num();
        self.icmp6_filt[tp as usize >> 5] &= !(1 << (tp & 31));
    }

    pub fn block(&mut self, icmp_type: IcmpV6Type) {
        let tp = icmp_type.to_num();
        self.icmp6_filt[tp as usize >> 5] |= 1 << ((tp & 31));
    }
}
