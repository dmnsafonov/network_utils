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

    pub const F_WRLCK: c_short = 1;

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

gen_enum!(pub SecBit: c_int;
    (SECBIT_NOROOT => NoRoot),
    (SECBIT_NOROOT_LOCKED => NoRootLocked),
    (SECBIT_NO_SETUID_FIXUP => NoSetuidFixup),
    (SECBIT_NO_SETUID_FIXUP_LOCKED => NoSetuidFixupLocked),
    (SECBIT_KEEP_CAPS => KeepCaps),
    (SECBIT_KEEP_CAPS_LOCKED => KeepCapsLocked),
    (SECBIT_NO_CAP_AMBIENT_RAISE => NoCapAmbientRaise),
    (SECBIT_NO_CAP_AMBIENT_RAISE_LOCKED => NoCapAmbientRaiseLocked)
);
gen_flag_set!(pub SecBitSet, SecBit: c_int);

gen_enum!(pub Permissions: mode_t;
    (S_IXUSR => UserExecute),
    (S_IWUSR => UserWrite),
    (S_IRUSR => UserRead),
    (S_IXGRP => GroupExecute),
    (S_IWGRP => GroupWrite),
    (S_IRGRP => GroupRead),
    (S_IXOTH => OtherExecute),
    (S_IWOTH => OtherWrite),
    (S_IROTH => OtherRead),

    (S_ISUID => SetUid),
    (S_ISGID => SetGid),
    (S_ISVTX => Sticky)
);
gen_flag_set!(pub PermissionSet, Permissions: mode_t);

gen_enum!(pub UmaskPermissions: mode_t;
    (S_IXUSR => UserExecute),
    (S_IWUSR => UserWrite),
    (S_IRUSR => UserRead),
    (S_IXGRP => GroupExecute),
    (S_IWGRP => GroupWrite),
    (S_IRGRP => GroupRead),
    (S_IXOTH => OtherExecute),
    (S_IWOTH => OtherWrite),
    (S_IROTH => OtherRead)
);
gen_flag_set!(pub UmaskPermissionSet, UmaskPermissions: mode_t);

gen_enum!(pub FileOpenFlags: c_int;
    (O_RDONLY => ReadOnly),
    (O_WRONLY => WriteOnly),
    (O_RDWR => ReadWrite),
    (O_APPEND => Append),
    (O_ASYNC => Async),
    (O_CLOEXEC => CloseOnExec),
    (O_CREAT => Create),
    (O_DIRECT => Direct),
    (O_DIRECTORY => Directory),
    (O_DSYNC => DSync),
    (O_EXCL => Exclusive),
    (O_LARGEFILE => LargeFile),
    (O_NOATIME => NoATime),
    (O_NOCTTY => NoCTty),
    (O_NOFOLLOW => NoFollow),
    (O_NONBLOCK => Nonblock),
    (O_NDELAY => NDelay),
    (O_PATH => Path),
    (O_SYNC => Sync),
    (O_TMPFILE => TmpFile),
    (O_TRUNC => Truncate)
);
gen_flag_set!(pub FileOpenFlagSet, FileOpenFlags: c_int);

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

gen_enum!(pub RecvFlags: c_int;
    (MSG_CMSG_CLOEXEC => CmsgCloexec),
    (MSG_DONTWAIT => DontWait),
    (MSG_ERRQUEUE => ErrQueue),
    (MSG_OOB => Oob),
    (MSG_PEEK => Peek),
    (MSG_TRUNC => Trunc),
    (MSG_WAITALL => WaitAll)
);
gen_flag_set!(pub RecvFlagSet, RecvFlags: c_int);

gen_enum!(pub SendFlags: c_int;
    (MSG_CONFIRM => Confirm),
    (MSG_DONTROUTE => DontRoute),
    (MSG_DONTWAIT => DontWait),
    (MSG_EOR => Eor),
    (MSG_MORE => More),
    (MSG_NOSIGNAL => NoSignal),
    (MSG_OOB => Oob)
);
gen_flag_set!(pub SendFlagSet, SendFlags: c_int);

gen_enum!(pub BpfCommandFlags: u16;
    (BPF_LD => LD),
    (BPF_LDX => LDX),
    (BPF_ST => ST),
    (BPF_STX => STX),
    (BPF_ALU => ALU),
    (BPF_JMP => JMP),
    (BPF_RET => RET),
    (BPF_MISC => MISC),

    (BPF_W => W),
    (BPF_H => H),
    (BPF_B => B),

    (BPF_IMM => IMM),
    (BPF_ABS => ABS),
    (BPF_IND => IND),
    (BPF_MEM => MEM),
    (BPF_LEN => LEN),
    (BPF_MSH => MSH),

    (BPF_ADD => ADD),
    (BPF_SUB => SUB),
    (BPF_MUL => MUL),
    (BPF_DIV => DIV),
    (BPF_OR => OR),
    (BPF_AND => AND),
    (BPF_LSH => LSH),
    (BPF_RSH => RSH),
    (BPF_NEG => NEG),
    (BPF_MOD => MOD),
    (BPF_XOR => XOR),

    (BPF_JA => JA),
    (BPF_JEQ => JEQ),
    (BPF_JGT => JGT),
    (BPF_JGE => JGE),
    (BPF_JSET => JSET),

    (BPF_K => K),
    (BPF_X => X)
);
gen_flag_set!(pub BpfCommand, BpfCommandFlags: u16);

gen_enum!(pub AddrInfoFlags: c_int;
    (AI_ADDRCONFIG => AddrConfig),
    (AI_ALL => All),
    (AI_CANONNAME => CanonName),
    (AI_NUMERICHOST => NumericHost),
    (AI_NUMERICSERV => NumericServ),
    (AI_PASSIVE => Passive),
    (AI_V4MAPPED => V4Mapped)
);
gen_flag_set!(pub AddrInfoFlagSet, AddrInfoFlags: c_int);
