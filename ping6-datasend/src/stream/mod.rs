mod buffers;
mod packet;
mod stm;

use ::std::num::Wrapping;

use ::rand::*;

use ::bytes::BytesMut;
use ::tokio::prelude::*;

use ::linux_network::*;
use ::ping6_datacommon::*;

use ::config::*;
use ::errors::{Error, Result};
use ::stdin::StdinBytesReader;
use ::util::InitState;

use self::stm::*;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn stream_mode((config, src, dst, sock): InitState) -> Result<()> {
    let _stream_conf = match config.mode {
        ModeConfig::Stream(ref conf) => conf,
        _ => unreachable!()
    };

    let mut rt = ::tokio::runtime::Builder::new()
        .core_threads(1)
        .build()?;

    let mtu = match config.bind_interface {
        Some(ref s) => {
            let mtu = get_interface_mtu(&sock, s)?;
            assert!(mtu >= 1280);
            if mtu as usize >= u16::max_value() as usize {
                u16::max_value()
            } else {
                mtu as u16
            }
        },
        None => IPV6_MIN_MTU
    };

    let async_sock = futures::IPv6RawSocketAdapter::new(rt.reactor(), sock)?;
    let data = StdinBytesReader::new(rt.reactor())?;

    let init_state = StreamCommonState {
        config: unsafe { (&config as *const Config).as_ref().unwrap() },
        src,
        dst,
        sock: async_sock,
        mtu,
        data_source: data,
        send_buf: BytesMut::with_capacity(mtu as usize),
        // if we assumed default mtu, then the incoming packet size is unknown
        recv_buf: BytesMut::with_capacity(u16::max_value() as usize),
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
