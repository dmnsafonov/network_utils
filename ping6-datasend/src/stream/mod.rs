mod buffers;
mod packet;
mod stm;

use ::std::io;
use ::std::num::Wrapping;

use ::rand::*;

use ::bytes::BytesMut;
use ::tokio::prelude::*;

use ::linux_network::*;
use ::ping6_datacommon::*;

use ::config::*;
use ::errors::{Error, Result};
use ::stdin_iterator::StdinBytesReader;
use ::util::InitState;

use self::stm::*;

pub fn stream_mode((config, src, dst, sock): InitState) -> Result<()> {
    let _stream_conf = extract!(ModeConfig::Stream(_), config.mode.clone())
        .unwrap();

    let mut rt = ::tokio::runtime::Builder::new()
        .threadpool_builder({
            let mut builder = ::tokio::executor::thread_pool::Builder::new();
            builder.pool_size(1);
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
    let stdin = io::stdin();
    let data = StdinBytesReader::new(
        rt.reactor(),
        unsafe { (&stdin as *const io::Stdin).as_ref().unwrap().lock() }
    )?;

    let init_state = StreamCommonState {
        config: unsafe { (&config as *const Config).as_ref().unwrap() },
        src: src,
        dst: dst,
        sock: async_sock,
        mtu: mtu,
        data_source: data,
        send_buf: BytesMut::with_capacity(mtu as usize),
        // if we assumed default mtu, then the incoming packet size is unknown
        recv_buf: BytesMut::with_capacity(::std::u16::MAX as usize),
        next_seqno: Wrapping(thread_rng().gen())
    };

    let mut stm = StreamMachine::start(init_state);
    rt.spawn(future::poll_fn(move || {
        match stm.poll() {
            Err(e) => {
                error!("{}", e);
                Err(())
            },
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(TerminationReason::DataSent)) => {
                info!("data sent successfully");
                Ok(Async::Ready(()))
            },
            Ok(Async::Ready(TerminationReason::ServerFin)) => {
                info!("connection dropped by server");
                Ok(Async::Ready(()))
            }
        }
    }));
    debug!("protocol state machine spawned");

    rt.shutdown_on_idle().wait().map_err(|_| Error::SpawnError)?;
    Ok(())
}
