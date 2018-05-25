use ::std::ffi::*;
use ::std::mem::*;
use ::std::ptr::*;
use ::std::os::unix::prelude::*;

use ::libc::*;
use ::nix::sys::socket::SockType;

use ::*;
use ::raw::*;
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

    ioctl!(get_interface_flags; SIOCGIFFLAGS; ifreq);
    ioctl!(set_interface_flags; SIOCSIFFLAGS; ifreq);
    ioctl!(get_interface_index; SIOCGIFINDEX; ifreq);
    ioctl!(get_interface_mtu; SIOCGIFMTU; ifreq);
}

pub fn get_securebits() -> Result<SecBits> { unsafe {
    Ok(
        SecBits::from_bits(
            n1try!(
                prctl(PR_GET_SECUREBITS)
            )
        ).expect("valid secure bits")
    )
}}

pub fn set_securebits(bits: SecBits) -> Result<()> { unsafe {
    n1try!(prctl(PR_SET_SECUREBITS, bits.bits() as c_ulong));
    Ok(())
}}

pub fn set_no_new_privs() -> Result<()> { unsafe {
    // see kernel Documentation/prctl/no_new_privs.txt for the 0's
    n1try!(prctl(
        PR_SET_NO_NEW_PRIVS,
        1 as c_ulong,
        0 as c_ulong,
        0 as c_ulong,
        0 as c_ulong));
    Ok(())
}}

pub fn drop_supplementary_groups() -> Result<()> { unsafe {
    n1try!(setgroups(0, null_mut()));
    Ok(())
}}

pub fn umask(mask: UmaskPermissions)
        -> Result<UmaskPermissions> { unsafe {
    Ok(
        UmaskPermissions::from_bits(::libc::umask(mask.bits()))
            .expect("valid umask bits")
    )
}}

pub fn fcntl_lock_fd<F>(fd: &mut F) -> Result<()>
        where F: AsRawFd { unsafe {
    let mut lock: flock = zeroed();
    lock.l_type = F_WRLCK as i16;
    lock.l_whence = SEEK_SET as c_short;
    n1try!(fcntl(fd.as_raw_fd(), F_SETLK, &mut lock));
    Ok(())
}}

fn ifreq_with_ifname<T>(ifname: T) -> Result<ifreq> where
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
    let mut ifr = ifreq_with_ifname(ifname)?;
    self::raw::get_interface_flags(fd, &mut ifr)?;
    Ok(ifr.un.ifr_flags)
}}

pub fn set_interface_flags<F,T>(fd: &F, ifname: T, flags: c_short)
        -> Result<()> where
        F: AsRawFd + ?Sized,
        T: AsRef<str> { unsafe {
    let mut ifr = ifreq_with_ifname(ifname)?;
    ifr.un.ifr_flags = flags;
    self::raw::set_interface_flags(fd, &mut ifr)?;
    Ok(())
}}

pub fn get_interface_index<F,T>(fd: &F, ifname: T) -> Result<c_int> where
        F: AsRawFd + ?Sized,
        T: AsRef<str> { unsafe {
    let mut ifr = ifreq_with_ifname(ifname)?;
    self::raw::get_interface_index(fd, &mut ifr)?;
    Ok(ifr.un.ifr_ifindex)
}}

pub fn get_interface_mtu<F,T>(fd: &F, ifname: T) -> Result<c_int> where
        F: AsRawFd + ?Sized,
        T: AsRef<str> { unsafe {
    let mut ifr = ifreq_with_ifname(ifname)?;
    self::raw::get_interface_mtu(fd, &mut ifr)?;
    Ok(ifr.un.ifr_mtu)
}}

pub fn make_sockaddr_in6_v6_dgram<T>(
    addr_str: T,
    socktype: Option<SockType>,
    proto: c_int,
    port: in_port_t,
    flags: AddrInfoFlags
) -> Result<sockaddr_in6> where T: AsRef<str> { unsafe {
    let mut ai: addrinfo = zeroed();
    ai.ai_family = AF_INET6;
    ai.ai_socktype = socktype.map(|x| x as c_int).unwrap_or(0);
    ai.ai_protocol = proto;
    ai.ai_flags = flags.bits();

    let mut res: *mut addrinfo = null_mut();

    let err = getaddrinfo(
        CString::new(addr_str.as_ref())?.as_ptr(),
        null(),
        &ai,
        &mut res
    );
    if err != 0 {
        match err {
            EAI_SYSTEM => bail!(::std::io::Error::last_os_error()),
            _ => bail!(ErrorKind::AddrError(
                    addr_str.as_ref().to_string(),
                    CStr::from_ptr(gai_strerror(err))
                    .to_string_lossy()
                    .into_owned()
                ))
        }
    }

    assert_eq!(((*res).ai_addrlen) as usize, size_of::<sockaddr_in6>());
    let mut sa = ::std::ptr::read(
        transmute::<*mut sockaddr, *mut sockaddr_in6>((*res).ai_addr)
    );
    assert_eq!(sa.sin6_family, AF_INET6 as sa_family_t);
    sa.sin6_port = port;

    freeaddrinfo(res);

    Ok(sa)
}}

fn get_fd_flags<F>(fd: &F)
        -> Result<FileOpenFlags> where F: AsRawFd + ?Sized { unsafe {
    Ok(
        FileOpenFlags::from_bits(
            n1try!(
                fcntl(fd.as_raw_fd(), F_GETFL)
            )
        ).expect("valid open file flags")
    )
}}

fn set_fd_flags<F>(fd: &F, flags: FileOpenFlags)
        -> Result<()> where F: AsRawFd + ?Sized { unsafe {
    n1try!(fcntl(fd.as_raw_fd(), F_SETFL, flags.bits()));
    Ok(())
}}

pub fn get_fd_nonblock<F>(fd: &F) -> Result<bool> where F: AsRawFd + ?Sized {
    Ok(get_fd_flags(fd)?.contains(FileOpenFlags::Nonblock))
}

gen_boolean_enum!(pub Nonblock);

pub fn set_fd_nonblock<F>(fd: &F, nonblock: Nonblock)
        -> Result<bool> where F: AsRawFd + ?Sized {
    let flags = get_fd_flags(fd)?;
    let new_flags = match nonblock {
        Nonblock::Yes => flags | FileOpenFlags::Nonblock,
        Nonblock::No => flags & FileOpenFlags::Nonblock
    };
    if flags != new_flags {
        set_fd_flags(fd, new_flags)?;
    }
    Ok(flags.contains(FileOpenFlags::Nonblock))
}
