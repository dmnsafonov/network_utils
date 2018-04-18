use ::std::collections::VecDeque;
use ::std::net::*;
use ::std::num::Wrapping;

use ::tokio::prelude::*;

use ::ping6_datacommon::*;
use ::linux_network::SendFlagSet;
use ::linux_network::futures::*;
use ::sliceable_rcref::SArcRef;

use ::stream::buffers::TimedAckSeqnoGenerator;
use ::stream::packet::make_stream_server_icmpv6_packet;

pub struct AckSender {
    ack_gen: TimedAckSeqnoGenerator,
    src: Ipv6Addr,
    dst: SocketAddrV6,
    send_buf: SArcRef<Vec<u8>>,
    sock: IpV6RawSocketAdapter,
    send_fut: Option<IpV6RawSocketSendtoFuture>,
    ranges_to_send: VecDeque<IRange<Wrapping<u16>>>
}

impl AckSender {
    pub fn new(
        ack_gen: TimedAckSeqnoGenerator,
        src: Ipv6Addr,
        dst: SocketAddrV6,
        send_buf: SArcRef<Vec<u8>>,
        sock: IpV6RawSocketAdapter
    ) -> AckSender {
        AckSender {
            ack_gen, src, dst, send_buf, sock,
            send_fut: None,
            ranges_to_send: VecDeque::new()
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

            if self.send_fut.is_some() {
                match self.send_fut.as_mut().unwrap().poll().map_err(|_| ())? {
                    Async::NotReady => return Ok(Async::NotReady),
                    Async::Ready(size) => {
                        debug_assert!(size == STREAM_SERVER_FULL_HEADER_SIZE as usize);
                        self.send_fut.take();
                        active = true;
                    }
                }
            }

            if let Some(IRange(l,r)) = self.ranges_to_send.pop_front() {
                debug!("sending ACK for range {} .. {}", l, r);
                let send_buf_ref = self.send_buf
                    .range(0 .. STREAM_SERVER_FULL_HEADER_SIZE as usize);
                make_stream_server_icmpv6_packet(
                    &mut send_buf_ref.borrow_mut(),
                    self.src,
                    *self.dst.ip(),
                    l.0,
                    r.0,
                    StreamPacketFlags::Ack.into(),
                    &[]
                );
                self.send_fut = Some(self.sock.sendto(
                    send_buf_ref,
                    self.dst,
                    SendFlagSet::new()
                ));
                active = true;
            }

            if active {
                continue;
            }

            if let Async::Ready(ranges_opt)
                    = self.ack_gen.poll().map_err(|_| ())? {
                match ranges_opt {
                    Some(ranges) => {
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
