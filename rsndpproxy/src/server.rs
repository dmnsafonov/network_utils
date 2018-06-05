/*
BPF filter

bpf_stmt!(B::LD | B::H | B::ABS, 12);
bpf_jump!(B::JMP | B::JEQ | B::K, ETHERTYPE_IPV6, 0, 5);

bpf_stmt!(B::LD | B::B | B::ABS, 20);
bpf_jump!(B::JMP | B::JEQ | B::K, IPPROTO_ICMPV6, 0, 3);

bpf_stmt!(B::LD | B::B | B::ABS, 54);
bpf_jump!(B::JMP | B::JEQ | B::K, ND_NEIGHBOR_SOLICIT, 0, 1);

bpf_stmt!(B::RET | B::K, ::std::u32::MAX);

bpf_stmt!(B::RET | B::K, 0);
*/

use ::std::sync::{Arc, atomic::*};

use ::tokio::prelude::*;

use ::config::InterfaceConfig;

pub struct Server {

}

impl Server {
    pub fn new(
        ifc: &InterfaceConfig,
        fast_quit: Arc<AtomicBool>,
        quit: Arc<AtomicBool>
    ) -> Server {
        unimplemented!()
    }
}

impl Future for Server {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        unimplemented!()
    }
}
