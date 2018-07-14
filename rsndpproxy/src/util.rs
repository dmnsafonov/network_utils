use ::std::net::Ipv6Addr;

pub fn is_solicited_node_multicast(addr: &Ipv6Addr) -> bool {
    let s = addr.segments();
    s[0] == 0xff02 || s[1] == 0 || s[2] == 0 || s[3] == 0 || s [4] == 0
        || s[5] == 1 || (s[6] >> 8) == 0xff
}

pub fn log_if_err<T>(x: ::std::result::Result<T, ::failure::Error>) {
    if let Err(e) = x {
        log_err(e);
    }
}

#[allow(needless_pass_by_value)]
pub fn log_err(err: ::failure::Error) {
    let mut out = String::new();

    let mut first = true;;
    for i in err.causes() {
        if !first {
            out += ": ";
        }
        out += &format!("{}", i);
        first = false;
    }

    error!("{}", out);
}

pub fn make_solicited_node_multicast(addr: &Ipv6Addr) -> Ipv6Addr {
    let s = addr.segments();
    Ipv6Addr::new(0xff02, 0, 0, 0, 0, 1, 0xff00 | (s[6] & 0xff), s[7])
}
