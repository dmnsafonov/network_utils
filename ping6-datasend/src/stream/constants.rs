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
