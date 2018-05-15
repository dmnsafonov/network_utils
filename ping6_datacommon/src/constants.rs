mod raw {
    pub const STREAM_SYN: u8 = 128;
    pub const STREAM_ACK: u8 = 64;
    pub const STREAM_FIN: u8 = 32;
    pub const STREAM_WS: u8 = 16;
}

use self::raw::*;

gen_enum!(pub StreamPacketFlags: u8;
    (STREAM_SYN => Syn),
    (STREAM_ACK => Ack),
    (STREAM_FIN => Fin),
    (STREAM_WS => WS)
);
gen_flag_set!(pub StreamPacketFlagSet, StreamPacketFlags: u8);
pub const ALL_STREAM_PACKET_FLAGS: u8
    = STREAM_SYN | STREAM_ACK | STREAM_FIN | STREAM_WS;

pub const IPV6_HEADER_SIZE: usize = 40;

pub const ICMPV6_ECHO_REQUEST_HEADER_SIZE: usize = 4;

pub const STREAM_CLIENT_HEADER_SIZE: usize = 6;
pub const STREAM_CLIENT_FULL_HEADER_SIZE: usize
    = ICMPV6_ECHO_REQUEST_HEADER_SIZE + STREAM_CLIENT_HEADER_SIZE;
pub const STREAM_CLIENT_HEADER_SIZE_WITH_IP: usize
    = STREAM_CLIENT_FULL_HEADER_SIZE + IPV6_HEADER_SIZE;

pub const STREAM_SERVER_HEADER_SIZE: usize = 8;
pub const STREAM_SERVER_FULL_HEADER_SIZE: usize
    = ICMPV6_ECHO_REQUEST_HEADER_SIZE + STREAM_SERVER_HEADER_SIZE;
pub const STREAM_SERVER_HEADER_SIZE_WITH_IP: usize
    = STREAM_SERVER_FULL_HEADER_SIZE + IPV6_HEADER_SIZE;

pub const IPV6_MIN_MTU: u16 = 1280;

pub const PACKET_LOSS_TIMEOUT: u64 = 2000;
pub const CONNECTION_LOSS_TIMEOUT: u64 = 30000;
pub const RETRANSMISSIONS_NUMBER: u64 = 3;
pub const ACK_SEND_PERIOD: u64 = PACKET_LOSS_TIMEOUT / 3;
