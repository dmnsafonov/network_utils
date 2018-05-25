use ::std::net::*;

use ::bytes::*;

use ::linux_network::*;

use ::ping6_datacommon::*;

use ::stream::packet::make_stream_server_icmpv6_packet;

pub fn make_send_fut_raw<'a>(
    mut sock: futures::IpV6RawSocketAdapter,
    mut send_buf: &mut BytesMut,
    src: Ipv6Addr,
    dst: SocketAddrV6,
    flags: StreamPacketFlags,
    seqno_start: u16,
    seqno_end: u16,
    payload: &[u8]
) -> futures::IpV6RawSocketSendtoFuture {
    let packet = make_stream_server_icmpv6_packet(
        &mut send_buf,
        src,
        *dst.ip(),
        seqno_start,
        seqno_end,
        flags,
        payload
    );

    sock.sendto(
        packet,
        dst,
        SendFlags::empty()
    )
}

pub fn make_send_fut<'a>(
    common: &mut ::stream::stm::StreamCommonState<'a>,
    dst: SocketAddrV6,
    flags: StreamPacketFlags,
    seqno_start: u16,
    seqno_end: u16,
    payload: &[u8]
) -> futures::IpV6RawSocketSendtoFuture {
    make_send_fut_raw(
        common.sock.clone(),
        &mut common.send_buf,
        *common.src.ip(),
        dst,
        flags,
        seqno_start,
        seqno_end,
        payload
    )
}
