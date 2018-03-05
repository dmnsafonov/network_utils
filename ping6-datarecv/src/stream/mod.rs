mod buffers;
mod packet;
mod stm;

use ::linux_network::*;
use ::ping6_datacommon::*;
use ::sliceable_rcref::SRcRef;

use ::config::*;
use ::errors::Result;
use ::stdout_iterator::*;
use ::util::InitState;

use self::stm::*;

pub fn stream_mode((config, bound_addr, sock): InitState) -> Result<()> {
    let stream_conf = match config.mode {
        ModeConfig::Stream(ref x) => x,
        _ => unreachable!()
    };

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
    let stdout = ::std::io::stdout();
    let data_out = StdoutBytesWriter::new(&core_handle, stdout.lock())?;
    let timer = ::tokio_timer::wheel()
        .num_slots(::std::u16::MAX as usize + 1)
        .build();

    let init_state = StreamCommonState {
        config: &config,
        src: make_socket_addr(
            config.bind_address.as_ref().unwrap(),
            Resolve::No
        )?,
        window_size: stream_conf.window_size,
        sock: async_sock,
        mtu: mtu,
        data_out: data_out,
        timer: timer,
        send_buf: SRcRef::new(vec![0; mtu as usize], 0 .. (mtu as usize)),
        // if we assumed default mtu, then the incoming packet size is unknown
        recv_buf: SRcRef::new(vec![0; ::std::u16::MAX as usize],
            0 .. (::std::u16::MAX as usize))
    };

    let stm = StreamMachine::start(init_state);
    match core.run(stm)? {
        TerminationReason::DataReceived => unimplemented!(),
        TerminationReason::Interrupted => unimplemented!()
    }
}
