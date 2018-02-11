mod buffers;
mod constants;
mod packet;
mod stm;

use ::linux_network::*;

use ::config::*;
use ::errors::Result;
use ::stdin_iterator::StdinBytesReader;
use ::util::InitState;

use self::stm::*;

pub fn stream_mode((config, src, dst, sock): InitState) -> Result<()> {
    let _stream_conf = extract!(ModeConfig::Stream(_), config.mode.clone())
        .unwrap();

    let mut core = ::tokio_core::reactor::Core::new()?;
    let core_handle = core.handle();

    let async_sock = futures::IpV6RawSocketAdapter::new(&core_handle, sock)?;
    let stdin = ::std::io::stdin();
    let data = StdinBytesReader::new(&core_handle, stdin.lock())?;
    let init_state = StreamInitState {
        config: &config,
        src: src,
        dst: dst,
        sock: async_sock,
        data_source: data
    };

    let stm = StreamMachine::start(init_state, 0);
    match core.run(stm)? {
        TerminationReason::DataSent => unimplemented!(),
        TerminationReason::ServerFin => unimplemented!()
    }
}
