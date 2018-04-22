use ::std::net::*;

use ::linux_network::*;
use ::sliceable_rcref::SArcRef;

use ::ping6_datacommon::*;

use ::stream::packet::make_stream_server_icmpv6_packet;

pub fn make_send_fut_raw<'a>(
    mut sock: futures::IpV6RawSocketAdapter,
    send_buf: SArcRef<Vec<u8>>,
    src: Ipv6Addr,
    dst: SocketAddrV6,
    flags: StreamPacketFlagSet,
    seqno_start: u16,
    seqno_end: u16,
    payload: &[u8]
) -> futures::IpV6RawSocketSendtoFuture {
    let send_buf_ref = send_buf
        .range(0 .. STREAM_SERVER_FULL_HEADER_SIZE as usize + payload.len());

    make_stream_server_icmpv6_packet(
        &mut send_buf_ref.borrow_mut(),
        src,
        *dst.ip(),
        seqno_start,
        seqno_end,
        flags,
        payload
    );

    sock.sendto(
        send_buf_ref,
        dst,
        SendFlagSet::new()
    )
}

pub fn make_send_fut<'a>(
    common: &mut ::stream::stm::StreamCommonState<'a>,
    dst: SocketAddrV6,
    flags: StreamPacketFlagSet,
    seqno_start: u16,
    seqno_end: u16,
    payload: &[u8]
) -> futures::IpV6RawSocketSendtoFuture {
    make_send_fut_raw(
        common.sock.clone(),
        common.send_buf.clone(),
        *common.src.ip(),
        dst,
        flags,
        seqno_start,
        seqno_end,
        payload
    )
}
