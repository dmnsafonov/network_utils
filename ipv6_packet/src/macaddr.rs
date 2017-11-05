use ::std::fmt::*;

#[derive(Clone)]
pub struct MacAddr([u8; 6]);

named!(pub mac_addr(&[u8]) -> MacAddr, map!(
    take!(6),
    |x| MacAddr({
        let mut ret = [0; 6];
        &mut ret.copy_from_slice(x);
        ret
    })
));

named!(pub mac_addr_eof(&[u8]) -> MacAddr, map!(
    pair!(mac_addr, eof!()),
    |(x,_)| x
));

impl Debug for MacAddr {
    fn fmt(&self, f: &mut Formatter) -> Result {
        (self as &Display).fmt(f)
    }
}

impl Display for MacAddr {
    fn fmt(&self, f: &mut Formatter) -> Result {
        let octets = self.0.iter()
            .map(|x| format!("{:x}", x))
            .map(|s| if s.len() == 1 {"0".to_string() + &s} else {s})
            .map(|s| ":".to_string() + &s)
            .collect::<Vec<String>>();
        let s = octets.iter()
            .flat_map(|x| x.chars())
            .collect::<String>()
            [1..]
            .to_string();
        write!(f, "{}", s)
    }
}
