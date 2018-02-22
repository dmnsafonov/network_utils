mod buffers;
mod constants;
mod packet;
mod stm;

use ::std::cell::RefCell;
use ::std::num::Wrapping;

use ::rand::*;

use ::linux_network::*;

use ::config::*;
use ::errors::Result;
use ::stdin_iterator::StdinBytesReader;
use self::constants::IPV6_MIN_MTU;
use ::util::InitState;

use self::stm::*;

pub fn stream_mode((config, src, dst, sock): InitState) -> Result<()> {
    let _stream_conf = extract!(ModeConfig::Stream(_), config.mode.clone())
        .unwrap();

    let mut core = ::tokio_core::reactor::Core::new()?;
    let core_handle = core.handle();

    let mtu = match config.bind_interface {
        Some(ref s) => {
            let mtu = get_interface_mtu(&sock, s)?;
            assert!(mtu >= 1280);
            if mtu as usize >= ::std::u16::MAX as usize {
                ::std::u16::MAX
            } else {
                mtu as u16
            }
        },
        None => IPV6_MIN_MTU
    };

    let async_sock = futures::IpV6RawSocketAdapter::new(&core_handle, sock)?;
    let stdin = ::std::io::stdin();
    let data = StdinBytesReader::new(&core_handle, stdin.lock())?;
    let timer = ::tokio_timer::wheel()
        .num_slots(::std::u16::MAX as usize + 1)
        .build();
    let init_state = StreamState {
        config: &config,
        src: src,
        dst: dst,
        sock: Box::new(async_sock),
        mtu: mtu,
        data_source: data,
        timer: timer,
        send_buf: RefCell::new(vec![0; ::std::u16::MAX as usize]),
        recv_buf: RefCell::new(vec![0; mtu as usize]),
        next_seqno: Wrapping(thread_rng().gen())
    };

    let stm = StreamMachine::start(init_state);
    match core.run(stm)? {
        TerminationReason::DataSent => unimplemented!(),
        TerminationReason::ServerFin => unimplemented!()
    }
}
