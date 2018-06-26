mod ack_sender;
mod buffers;
pub mod packet;
pub mod stdout;
pub mod stm;
pub mod util;

use ::std::io;

use ::bytes::BytesMut;

use ::linux_network::*;
use ::ping6_datacommon::*;

use ::config::*;
use ::errors::{Error, Result};
use self::stdout::*;
use ::tokio::prelude::*;
use ::util::InitState;

use self::stm::*;

pub fn stream_mode((config, _, sock): InitState) -> Result<()> {
    let stream_conf = match config.mode {
        ModeConfig::Stream(ref x) => x,
        _ => unreachable!()
    };

    let mut rt = ::tokio::runtime::Builder::new()
        .threadpool_builder({
            let mut builder = ::tokio::executor::thread_pool::Builder::new();
            builder.pool_size(2);
            builder
        }).build()?;

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

    let async_sock = futures::IPv6RawSocketAdapter::new(rt.reactor(), sock)?;
    let stdout = io::stdout();
    let data_out = StdoutBytesWriter::new(
        rt.reactor(),
        unsafe { (&stdout as *const io::Stdout).as_ref().unwrap().lock() }
    )?;

    let init_state = StreamCommonState {
        config: unsafe { (&config as *const Config).as_ref().unwrap() },
        src: make_socket_addr(
            config.bind_address.as_ref().unwrap(),
            Resolve::No
        )?,
        window_size: stream_conf.window_size,
        sock: async_sock,
        mtu: mtu,
        data_out: data_out,
        send_buf: BytesMut::with_capacity(mtu as usize),
        // if we assumed default mtu, then the incoming packet size is unknown
        recv_buf: BytesMut::with_capacity(::std::u16::MAX as usize),
        handle: rt.executor()
    };

    let mut stm = StreamMachine::start(init_state);
    rt.spawn(future::poll_fn(move || {
        match stm.poll() {
            Err(e) => {
                error!("{}", e);
                Err(())
            },
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(TerminationReason::DataReceived)) => {
                info!("data received successfully");
                Ok(Async::Ready(()))
            },
            Ok(Async::Ready(TerminationReason::Interrupted)) => {
                info!("connection was interrupted");
                Ok(Async::Ready(()))
            }
        }
    }));
    debug!("protocol state machine spawned");

    rt.shutdown_on_idle().wait().map_err(|_| Error::SpawnError)?;
    Ok(())
}
