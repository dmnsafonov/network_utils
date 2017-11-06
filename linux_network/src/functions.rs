use ::std::mem::*;
use ::std::ptr::*;
use ::std::os::unix::prelude::*;

use ::libc::*;

use ::numeric_enums::*;

use ::*;
use ::constants::raw::*;
use ::structs::raw::*;
use ::errors::*;

pub mod raw {
    use super::*;

    macro_rules! ioctl {
        ( $name:ident; $command:expr; $typ:ty ) => (
            pub unsafe fn $name<T>(fd: &T, arg: &mut $typ) -> Result<()>
                    where T: AsRawFd + ?Sized {
                n1try!(ioctl(fd.as_raw_fd(), $command, arg as *mut $typ));
                Ok(())
            }
        )
    }

    ioctl!(get_interface_flags; super::constants::raw::SIOCGIFFLAGS; ifreq);
    ioctl!(set_interface_flags; super::constants::raw::SIOCSIFFLAGS; ifreq);
    ioctl!(get_interface_index; super::constants::raw::SIOCGIFINDEX; ifreq);
}

pub fn get_securebits() -> Result<SecBitSet> { unsafe {
    Ok(
        SecBitSet::from_num(
            n1try!(
                prctl(PR_GET_SECUREBITS)
            )
        )
    )
}}

pub fn set_securebits(bits: SecBitSet) -> Result<()> { unsafe {
    n1try!(prctl(PR_SET_SECUREBITS, bits.get() as c_ulong));
    Ok(())
}}

pub fn set_no_new_privs(x: bool) -> Result<()> { unsafe {
    // see kernel Documentation/prctl/no_new_privs.txt for the 0's
    n1try!(prctl(
        PR_SET_NO_NEW_PRIVS,
        if x {1} else {0} as c_ulong,
        0 as c_ulong,
        0 as c_ulong,
        0 as c_ulong));
    Ok(())
}}

pub fn drop_supplementary_groups() -> Result<()> { unsafe {
    n1try!(setgroups(0, null_mut()));
    Ok(())
}}

pub fn umask(mask: UmaskPermissionSet)
        -> Result<UmaskPermissionSet> { unsafe {
    Ok(UmaskPermissionSet::from_num(::libc::umask(mask.get())))
}}

pub fn fcntl_lock_fd<F>(fd: &mut F) -> Result<()>
        where F: AsRawFd { unsafe {
    let mut lock: flock = zeroed();
    lock.l_type = F_WRLCK;
    lock.l_whence = SEEK_SET as c_short;
    n1try!(fcntl(fd.as_raw_fd(), F_SETLK, &mut lock));
    Ok(())
}}

fn ifreq_with_ifname<F,T>(fd: &F, ifname: T)
        -> Result<ifreq> where
        F: AsRawFd + ?Sized,
        T: AsRef<str> { unsafe {
    let mut ifr: ifreq = zeroed();

    let ifname_ref = ifname.as_ref();
    let ifname_bytes = ifname_ref.as_bytes();
    if ifname_bytes.len() >= IFNAMSIZ {
        bail!(ErrorKind::IfNameTooLong(ifname_ref.to_string()));
    }
    copy_nonoverlapping(ifname_bytes.as_ptr(),
        ifr.ifr_name.as_mut_ptr() as *mut u8,
        ifname_bytes.len());

    Ok(ifr)
}}

pub fn get_interface_flags<F,T>(fd: &F, ifname: T) -> Result<c_short> where
        F: AsRawFd + ?Sized,
        T: AsRef<str> { unsafe {
    let mut ifr = ifreq_with_ifname(fd, ifname)?;
    self::raw::get_interface_flags(fd, &mut ifr)?;
    Ok(ifr.un.ifr_flags)
}}

pub fn set_interface_flags<F,T>(fd: &F, ifname: T, flags: c_short)
        -> Result<()> where
        F: AsRawFd + ?Sized,
        T: AsRef<str> { unsafe {
    let mut ifr = ifreq_with_ifname(fd, ifname)?;
    ifr.un.ifr_flags = flags;
    self::raw::set_interface_flags(fd, &mut ifr)?;
    Ok(())
}}

pub fn get_interface_index<F,T>(fd: &F, ifname: T) -> Result<c_int> where
        F: AsRawFd + ?Sized,
        T: AsRef<str> { unsafe {
    let mut ifr = ifreq_with_ifname(fd, ifname)?;
    self::raw::get_interface_index(fd, &mut ifr)?;
    Ok(ifr.un.ifr_ifindex)
}}
