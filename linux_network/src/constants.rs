#![allow(non_upper_case_globals)]

pub mod raw {
    use ::libc::*;

    pub const SECBIT_NOROOT: c_int = 1;
    pub const SECBIT_NOROOT_LOCKED: c_int = 1 << 1;
    pub const SECBIT_NO_SETUID_FIXUP: c_int = 1 << 2;
    pub const SECBIT_NO_SETUID_FIXUP_LOCKED: c_int = 1 << 3;
    pub const SECBIT_KEEP_CAPS: c_int = 1 << 4;
    pub const SECBIT_KEEP_CAPS_LOCKED: c_int = 1 << 5;
    pub const SECBIT_NO_CAP_AMBIENT_RAISE: c_int = 1 << 6;
    pub const SECBIT_NO_CAP_AMBIENT_RAISE_LOCKED: c_int = 1 << 7;

    pub const S_ISUID: mode_t = ::libc::S_ISUID as mode_t;
    pub const S_ISGID: mode_t = ::libc::S_ISGID as mode_t;
    pub const S_ISVTX: mode_t = ::libc::S_ISVTX as mode_t;

    pub const ICMPV6_FILTER: c_int = 1;

    pub const ICMP6_ECHO_REQUEST: uint8_t = 128;
    pub const ICMP6_ECHO_REPLY: uint8_t = 129;
    pub const MLD_LISTENER_QUERY: uint8_t = 130;
    pub const MLD_LISTENER_REPORT: uint8_t = 131;
    pub const MLD_LISTENER_REDUCTION: uint8_t = 132;
    pub const ND_ROUTER_SOLICIT: uint8_t = 133;
    pub const ND_ROUTER_ADVERT: uint8_t = 134;
    pub const ND_NEIGHBOR_SOLICIT: uint8_t = 135;
    pub const ND_NEIGHBOR_ADVERT: uint8_t = 136;
    pub const ND_REDIRECT: uint8_t = 137;

    pub const SIOCGIFFLAGS: c_ulong = 0x8913;
    pub const SIOCSIFFLAGS: c_ulong = 0x8914;
    pub const SIOCGIFINDEX: c_ulong = 0x8933;
    pub const SIOCGIFMTU: c_ulong = 0x8921;

    pub const BPF_LD: u16 = 0x00;
    pub const BPF_LDX: u16 = 0x01;
    pub const BPF_ST: u16 = 0x02;
    pub const BPF_STX: u16 = 0x03;
    pub const BPF_ALU: u16 = 0x04;
    pub const BPF_JMP: u16 = 0x05;
    pub const BPF_RET: u16 = 0x06;
    pub const BPF_MISC: u16 = 0x07;

    pub const BPF_W: u16 = 0x00;
    pub const BPF_H: u16 = 0x08;
    pub const BPF_B: u16 = 0x10;

    pub const BPF_IMM: u16 = 0x00;
    pub const BPF_ABS: u16 = 0x20;
    pub const BPF_IND: u16 = 0x40;
    pub const BPF_MEM: u16 = 0x60;
    pub const BPF_LEN: u16 = 0x80;
    pub const BPF_MSH: u16 = 0xa0;

    pub const BPF_ADD: u16 = 0x00;
    pub const BPF_SUB: u16 = 0x10;
    pub const BPF_MUL: u16 = 0x20;
    pub const BPF_DIV: u16 = 0x30;
    pub const BPF_OR: u16 = 0x40;
    pub const BPF_AND: u16 = 0x50;
    pub const BPF_LSH: u16 = 0x60;
    pub const BPF_RSH: u16 = 0x70;
    pub const BPF_NEG: u16 = 0x80;
    pub const BPF_MOD: u16 = 0x90;
    pub const BPF_XOR: u16 = 0xa0;

    pub const BPF_JA: u16 = 0x00;
    pub const BPF_JEQ: u16 = 0x10;
    pub const BPF_JGT: u16 = 0x20;
    pub const BPF_JGE: u16 = 0x30;
    pub const BPF_JSET: u16 = 0x40;

    pub const BPF_K: u16 = 0x00;
    pub const BPF_X: u16 = 0x08;

    pub const ETHERTYPE_IPV6: u32 = 0x86dd;
}

use ::libc::*;

use self::raw::*;
use self::raw::{S_ISUID, S_ISGID, S_ISVTX};

bitflags!(
    pub struct SecBits: c_int {
        const NoRoot = SECBIT_NOROOT;
        const NoRootLocked = SECBIT_NOROOT_LOCKED;
        const NoSetuidFixup = SECBIT_NO_SETUID_FIXUP;
        const NoSetuidFixupLocked = SECBIT_NO_SETUID_FIXUP_LOCKED;
        const KeepCaps = SECBIT_KEEP_CAPS;
        const KeepCapsLocked = SECBIT_KEEP_CAPS_LOCKED;
        const NoCapAmbientRaise = SECBIT_NO_CAP_AMBIENT_RAISE;
        const NoCapAmbientRaiseLocked = SECBIT_NO_CAP_AMBIENT_RAISE_LOCKED;
    }
);

bitflags!(
    pub struct Permissions: mode_t {
        const UserExecute = S_IXUSR;
        const UserWrite = S_IWUSR;
        const UserRead = S_IRUSR;
        const GroupExecute = S_IXGRP;
        const GroupWrite = S_IWGRP;
        const GroupRead = S_IRGRP;
        const OtherExecute = S_IXOTH;
        const OtherWrite = S_IWOTH;
        const OtherRead = S_IROTH;

        const SetUid = S_ISUID;
        const SetGid = S_ISGID;
        const Sticky = S_ISVTX;
    }
);

bitflags!(
    pub struct UmaskPermissions: mode_t {
        const UserExecute = S_IXUSR;
        const UserWrite = S_IWUSR;
        const UserRead = S_IRUSR;
        const GroupExecute = S_IXGRP;
        const GroupWrite = S_IWGRP;
        const GroupRead = S_IRGRP;
        const OtherExecute = S_IXOTH;
        const OtherWrite = S_IWOTH;
        const OtherRead = S_IROTH;
    }
);

bitflags!(
    pub struct FileOpenFlags: c_int {
        const ReadOnly = O_RDONLY;
        const WriteOnly = O_WRONLY;
        const ReadWrite = O_RDWR;
        const Append = O_APPEND;
        const Async = O_ASYNC;
        const CloseOnExec = O_CLOEXEC;
        const Create = O_CREAT;
        const Direct = O_DIRECT;
        const Directory = O_DIRECTORY;
        const DSync = O_DSYNC;
        const Exclusive = O_EXCL;
        const LargeFile = O_LARGEFILE;
        const NoATime = O_NOATIME;
        const NoCTty = O_NOCTTY;
        const NoFollow = O_NOFOLLOW;
        const Nonblock = O_NONBLOCK;
        const NDelay = O_NDELAY;
        const Path = O_PATH;
        const Sync = O_SYNC;
        const TmpFile = O_TMPFILE;
        const Truncate = O_TRUNC;
    }
);

// not exhaustive
gen_enum!(pub IpProto: c_int;
    (IPPROTO_IPV6 => IpV6),
    (IPPROTO_ICMPV6 => IcmpV6)
);

// not exhaustive
gen_enum!(pub SockOptLevel: c_int;
    (SOL_SOCKET => Socket),
    (IPPROTO_IPV6 => IpV6),
    (IPPROTO_ICMPV6 => IcmpV6)
);

// not exhaustive
gen_enum!(pub SockOpt: c_int;
    (IP_HDRINCL => IpHdrIncl),
    (ICMPV6_FILTER => IcmpV6Filter),
    (SO_BINDTODEVICE => BindToDevice),
    (SO_DONTROUTE => DontRoute),
    (IPV6_V6ONLY => V6Only),
    (SO_ATTACH_FILTER => AttachFilter),
    (SO_LOCK_FILTER => LockFilter)
);

gen_enum!(pub IcmpV6Type: uint8_t;
    (ICMP6_ECHO_REQUEST => EchoRequest),
    (ICMP6_ECHO_REPLY => EchoReply),
    (MLD_LISTENER_QUERY => MldListenerQuery),
    (MLD_LISTENER_REPORT => MldListenerReport),
    (MLD_LISTENER_REDUCTION => MldListenerReduction),
    (ND_ROUTER_SOLICIT => NdRouterSolicit),
    (ND_ROUTER_ADVERT => NdRouterAdvert),
    (ND_NEIGHBOR_SOLICIT => NdNeighborSolicit),
    (ND_NEIGHBOR_ADVERT => NdNeighborAdvert),
    (ND_REDIRECT => NdRedirect)
);

bitflags!(
    pub struct RecvFlags: c_int {
        const CmsgCloexec = MSG_CMSG_CLOEXEC;
        const DontWait = MSG_DONTWAIT;
        const ErrQueue = MSG_ERRQUEUE;
        const Oob = MSG_OOB;
        const Peek = MSG_PEEK;
        const Trunc = MSG_TRUNC;
        const WaitAll = MSG_WAITALL;
    }
);

bitflags!(
    pub struct SendFlags: c_int {
        const Confirm = MSG_CONFIRM;
        const DontRoute = MSG_DONTROUTE;
        const DontWait = MSG_DONTWAIT;
        const Eor = MSG_EOR;
        const More = MSG_MORE;
        const NoSignal = MSG_NOSIGNAL;
        const Oob = MSG_OOB;
    }
);

bitflags!(
    pub struct BpfCommandFlags: u16 {
        const LD = BPF_LD;
        const LDX = BPF_LDX;
        const ST = BPF_ST;
        const STX = BPF_STX;
        const ALU = BPF_ALU;
        const JMP = BPF_JMP;
        const RET = BPF_RET;
        const MISC = BPF_MISC;

        const W = BPF_W;
        const H = BPF_H;
        const B = BPF_B;

        const IMM = BPF_IMM;
        const ABS = BPF_ABS;
        const IND = BPF_IND;
        const MEM = BPF_MEM;
        const LEN = BPF_LEN;
        const MSH = BPF_MSH;

        const ADD = BPF_ADD;
        const SUB = BPF_SUB;
        const MUL = BPF_MUL;
        const DIV = BPF_DIV;
        const OR = BPF_OR;
        const AND = BPF_AND;
        const LSH = BPF_LSH;
        const RSH = BPF_RSH;
        const NEG = BPF_NEG;
        const MOD = BPF_MOD;
        const XOR = BPF_XOR;

        const JA = BPF_JA;
        const JEQ = BPF_JEQ;
        const JGT = BPF_JGT;
        const JGE = BPF_JGE;
        const JSET = BPF_JSET;

        const K = BPF_K;
        const X = BPF_X;
    }
);

bitflags!(
    pub struct AddrInfoFlags: c_int {
        const AddrConfig = AI_ADDRCONFIG;
        const All = AI_ALL;
        const CanonName = AI_CANONNAME;
        const NumericHost = AI_NUMERICHOST;
        const NumericServ = AI_NUMERICSERV;
        const Passive = AI_PASSIVE;
        const V4Mapped = AI_V4MAPPED;
    }
);
