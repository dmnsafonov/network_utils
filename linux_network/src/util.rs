use ::std::net::Ipv6Addr;
use ::libc::*;

macro_rules! n1try {
    ( $e: expr ) => ({
        let ret = $e;
        if ret == -1 {
            bail!(::std::io::Error::last_os_error())
        } else {
            ret
        }
    })
}

pub unsafe fn ref_to_cvoid<T: ?Sized>(x: &T) -> *const c_void {
    x as *const T as *const c_void
}

pub unsafe fn ref_to_mut_cvoid<T: ?Sized>(x: &mut T) -> *mut c_void {
    x as *mut T as *mut c_void
}

pub fn addr_from_segments(ad: &[u8; 16]) -> Ipv6Addr {
    Ipv6Addr::new(
        (ad[0] as u16) << 8 | (ad[1] as u16),
        (ad[2] as u16) << 8 | (ad[3] as u16),
        (ad[4] as u16) << 8 | (ad[5] as u16),
        (ad[6] as u16) << 8 | (ad[7] as u16),
        (ad[8] as u16) << 8 | (ad[9] as u16),
        (ad[10] as u16) << 8 | (ad[11] as u16),
        (ad[12] as u16) << 8 | (ad[13] as u16),
        (ad[14] as u16) << 8 | (ad[15] as u16)
   )
}

pub fn log_if_err<T,E>(x: ::std::result::Result<T,E>)
        where E: ::error_chain::ChainedError {
    if let Err(e) = x {
        error!("{}", e.display_chain());
    }
}
