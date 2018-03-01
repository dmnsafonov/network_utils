mod raw {
    pub const STREAM_SYN: u8 = 128;
    pub const STREAM_ACK: u8 = 64;
    pub const STREAM_FIN: u8 = 32;
}

use self::raw::*;

gen_enum!(pub StreamPacketFlags: u8;
    (STREAM_SYN => Syn),
    (STREAM_ACK => Ack),
    (STREAM_FIN => Fin)
);
gen_flag_set!(pub StreamPacketFlagSet, StreamPacketFlags: u8);
pub const ALL_STREAM_PACKET_FLAGS: u8 = STREAM_SYN | STREAM_ACK | STREAM_FIN;

pub const ICMPV6_ECHO_REQUEST_HEADER_SIZE: u16 = 4;

pub const STREAM_CLIENT_HEADER_SIZE: u16 = 6;
pub const STREAM_CLIENT_FULL_HEADER_SIZE: u16
    = ICMPV6_ECHO_REQUEST_HEADER_SIZE + STREAM_CLIENT_HEADER_SIZE;

pub const STREAM_SERVER_HEADER_SIZE: u16 = 8;
pub const STREAM_SERVER_FULL_HEADER_SIZE: u16
    = ICMPV6_ECHO_REQUEST_HEADER_SIZE + STREAM_SERVER_HEADER_SIZE;

pub const IPV6_MIN_MTU: u16 = 1280;

pub const PACKET_LOSS_TIMEOUT: u16 = 5000;
pub const RETRANSMISSIONS_NUMBER: u32 = 3;