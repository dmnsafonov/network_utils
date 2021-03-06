use ::std::collections::VecDeque;
use ::std::net::*;
use ::std::num::Wrapping;

use ::bytes::BytesMut;
use ::tokio::prelude::*;

use ::ping6_datacommon::*;
use ::linux_network::futures::*;

use ::stream::buffers::TimedAckSeqnoGenerator;
use ::stream::util::make_send_fut_raw;

pub struct AckSender {
    ack_gen: TimedAckSeqnoGenerator,
    src: Ipv6Addr,
    dst: SocketAddrV6,
    send_buf: BytesMut,
    sock: IPv6RawSocketAdapter,
    send_fut: Option<IPv6RawSocketSendtoFuture>,
    ranges_to_send: VecDeque<IRange<Wrapping<u16>>>,
    set_ws_packet: bool
}

impl AckSender {
    pub fn new(
        ack_gen: TimedAckSeqnoGenerator,
        src: Ipv6Addr,
        dst: SocketAddrV6,
        mtu: u16,
        sock: IPv6RawSocketAdapter
    ) -> Self {
        Self {
            ack_gen, src, dst,
            send_buf: BytesMut::with_capacity(mtu as usize),
            sock,
            send_fut: None,
            ranges_to_send: VecDeque::new(),
            set_ws_packet: false
        }
    }
}

impl Future for AckSender {
    type Item = ();
    type Error = ();
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut active = true;
        while active {
            active = false;

            if self.send_fut.is_none() {
                if let Some(IRange(l,r)) = self.ranges_to_send.pop_front() {
                    let mut flags = StreamPacketFlags::Ack;
                    if self.set_ws_packet {
                        flags |= StreamPacketFlags::WS;
                        self.set_ws_packet = false;
                    }

                    debug!("sending ACK for range {} .. {}", l, r);
                    self.send_fut = Some(make_send_fut_raw(
                        self.sock.clone(),
                        &mut self.send_buf,
                        self.src,
                        self.dst,
                        flags,
                        l.0,
                        r.0,
                        &[]
                    ));
                    active = true;
                }
            }

            if self.send_fut.is_some() {
                match self.send_fut.as_mut().unwrap().poll().map_err(|_| ())? {
                    Async::NotReady => return Ok(Async::NotReady),
                    Async::Ready(size) => {
                        debug_assert_eq!(size, STREAM_SERVER_FULL_HEADER_SIZE);
                        self.send_fut.take();
                        active = true;
                    }
                }
            }

            if active {
                continue;
            }

            if let Async::Ready(ranges_opt)
                    = self.ack_gen.poll().map_err(|_| ())? {
                match ranges_opt {
                    Some((ranges, ws)) => {
                        self.set_ws_packet = ws == ranges.front().unwrap().0;
                        self.ranges_to_send = ranges;
                        active = true;
                    },
                    None => return Ok(Async::Ready(()))
                }
            }
        }

        Ok(Async::NotReady)
    }
}
