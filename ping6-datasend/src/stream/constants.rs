use ::ping6_datacommon::ICMPV6_ECHO_REQUEST_HEADER_SIZE;

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

pub const HEADER_SIZE: u16 = 6;
pub const FULL_HEADER_SIZE: u16
    = ICMPV6_ECHO_REQUEST_HEADER_SIZE + HEADER_SIZE;
pub const IPV6_MIN_MTU: u16 = 1280;

pub const PACKET_LOSS_TIMEOUT: u16 = 5000;
pub const RETRANSMISSION_NUMBER: u32 = 3;
