#![allow(unknown_lints)]
#![warn(bare_trait_objects)]
#![warn(clippy::pedantic)]
#![allow(clippy::stutter)]

#[macro_use] extern crate bitflags;
extern crate byteorder;
extern crate capabilities;
#[macro_use] extern crate failure;
extern crate futures;
#[macro_use] extern crate log;
extern crate nix;
extern crate owning_ref;
extern crate pnet_packet;
extern crate seahash;
extern crate seccomp;
extern crate tokio;
extern crate tokio_timer;

#[macro_use] extern crate boolean_enums;
extern crate linux_network;

pub mod buffer;
pub mod constants;
pub mod errors;
pub mod range_tracker;
pub mod timeout;

use std::cell::RefCell;
use std::io;
use std::net::*;
use std::ops::*;
use std::os::unix::prelude::*;
use std::sync::atomic::*;

use byteorder::*;
use capabilities::*;
use libc::{c_int, c_long, IPPROTO_ICMPV6};
use nix::libc;
use nix::sys::signal::*;
use owning_ref::OwningHandle;
use ::pnet_packet::icmpv6::*;
use ::pnet_packet::icmpv6::ndp::Icmpv6Codes;
use ::pnet_packet::*;
use seccomp::*;

use linux_network::*;

pub use buffer::*;
pub use constants::*;
pub use errors::*;
pub use range_tracker::*;
pub use timeout::*;

gen_boolean_enum!(pub UseStreamMode);

gen_boolean_enum!(pub Resolve);

pub fn make_socket_addr<T>(addr_str: T, resolve: Resolve)
        -> Result<SocketAddrV6> where T: AsRef<str> {
    let sockaddr_in = make_sockaddr_in6_v6_dgram(
        addr_str,
        None,
        IPPROTO_ICMPV6,
        0,
        match resolve {
            Resolve::Yes => AddrInfoFlags::empty(),
            Resolve::No => AddrInfoFlags::NumericHost
        }
    )?;

    Ok(SocketAddrV6::new(
        sockaddr_in.sin6_addr.s6_addr.into(),
        0,
        0,
        sockaddr_in.sin6_scope_id
    ))
}

pub fn gain_net_raw() -> Result<()> {
    let mut caps = Capabilities::from_current_proc()
        .map_err(Error::Priv)?;
    if !caps.update(&[Capability::CAP_NET_RAW], Flag::Effective, true) {
        return Err(Error::Priv(io::Error::new(
            io::ErrorKind::Other,
            "cannot update capset"
        )).into());
    }
    caps.apply().map_err(Error::Priv)?;
    debug!("gained CAP_NET_RAW");
    Ok(())
}

pub fn drop_caps() -> Result<()> {
    Capabilities::new()?
        .apply()
        .map_err(Error::Priv)?;
    debug!("dropped all capabilities");
    Ok(())
}

#[allow(clippy::cast_possible_truncation)]
pub fn ping6_data_checksum<T>(payload: T) -> u16 where T: AsRef<[u8]> {
    use std::hash::Hasher;
    use seahash::*;
    let b = payload.as_ref();
    let mut hasher = SeaHasher::new();
    let mut buf = [0;2];
    BE::write_u16(&mut buf, b.len() as u16);
    hasher.write(&buf);
    hasher.write(b);
    (hasher.finish() & 0xffff) as u16
}

static SIGNAL_FLAG: AtomicBool = AtomicBool::new(false);

pub fn setup_signal_handler() -> Result<()> {
    let sigact = SigAction::new(
        SigHandler::Handler(signal_handler),
        SaFlags::empty(),
        SigSet::empty()
    );

    unsafe {
        sigaction(Signal::SIGINT, &sigact)?;
        sigaction(Signal::SIGQUIT, &sigact)?;
        sigaction(Signal::SIGTERM, &sigact)?;
    }

    debug!("set up signal handlers");
    Ok(())
}

extern "C" fn signal_handler(_: c_int) {
    SIGNAL_FLAG.store(true, Ordering::Relaxed);
}

pub fn signal_received() -> bool {
    SIGNAL_FLAG.swap(false, Ordering::Relaxed)
}

gen_boolean_enum!(pub StdoutUse);

pub fn allow_defaults() -> Result<Context> {
    let mut ctx = Context::default(Action::Kill)?;
    allow_syscall(&mut ctx, libc::SYS_close)?;
    allow_syscall(&mut ctx, libc::SYS_exit)?;
    allow_syscall(&mut ctx, libc::SYS_sigaltstack)?;
    allow_syscall(&mut ctx, libc::SYS_munmap)?;
    allow_syscall(&mut ctx, libc::SYS_exit_group)?;
    allow_syscall(&mut ctx, libc::SYS_rt_sigreturn)?;
    allow_syscall(&mut ctx, libc::SYS_futex)?;
    allow_syscall(&mut ctx, libc::SYS_mmap)?;
    allow_syscall(&mut ctx, libc::SYS_brk)?;
    Ok(ctx)
}

#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn allow_syscall(ctx: &mut Context, syscall: c_long) -> Result<()> {
    ctx.add_rule(
        Rule::new(
            syscall as usize,
            Compare::arg(0)
                .using(Op::Ge)
                .with(0)
                .build()
                .unwrap(),
            Action::Allow
        )
    )?;
    Ok(())
}

pub fn allow_console_out(ctx: &mut Context, out: StdoutUse) -> Result<()> {
    if let StdoutUse::Yes = out {
        allow_write_on(ctx, libc::STDOUT_FILENO)?;
    }
    allow_write_on(ctx, libc::STDERR_FILENO)?;
    Ok(())
}

pub fn allow_console_in(ctx: &mut Context) -> Result<()> {
    allow_fd_syscall(ctx, libc::STDIN_FILENO, libc::SYS_read)?;
    allow_fd_syscall(ctx, libc::STDIN_FILENO, libc::SYS_readv)?;
    allow_fd_syscall(ctx, libc::STDIN_FILENO, libc::SYS_preadv)?;
    allow_fd_syscall(ctx, libc::STDIN_FILENO, libc::SYS_preadv2)?;
    allow_fd_syscall(ctx, libc::STDIN_FILENO, libc::SYS_pread64)?;
    Ok(())
}

fn allow_write_on(ctx: &mut Context, fd: RawFd) -> Result<()> {
    allow_fd_syscall(ctx, fd, libc::SYS_write)?;
    allow_fd_syscall(ctx, fd, libc::SYS_writev)?;
    allow_fd_syscall(ctx, fd, libc::SYS_pwritev)?;
    allow_fd_syscall(ctx, fd, libc::SYS_pwritev2)?;
    allow_fd_syscall(ctx, fd, libc::SYS_pwrite64)?;
    allow_fd_syscall(ctx, fd, libc::SYS_fsync)?;
    allow_fd_syscall(ctx, fd, libc::SYS_fdatasync)?;
    Ok(())
}

#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn allow_fd_syscall(ctx: &mut Context, fd: RawFd, syscall: c_long)
        -> Result<()> {
    ctx.add_rule(
        Rule::new(
            syscall as usize,
            Compare::arg(0)
                .using(Op::Eq)
                .with(fd as u64)
                .build()
                .unwrap(),
            Action::Allow
        )
    )?;
    Ok(())
}

pub trait LockableIo<'a> {
    type LockType;
    fn movable_lock(&'a mut self) -> Self::LockType;
}

impl<'a> LockableIo<'a> for io::Stdin {
    type LockType = io::StdinLock<'a>;
    fn movable_lock(&'a mut self) -> Self::LockType {
        Self::lock(self)
    }
}

impl<'a> LockableIo<'a> for io::Stdout {
    type LockType = io::StdoutLock<'a>;
    fn movable_lock(&'a mut self) -> Self::LockType {
        Self::lock(self)
    }
}

pub type MovableIoLock<'a, T> = OwningHandle<
    Box<RefCell<T>>,
    Box<<T as LockableIo<'a>>::LockType>
>;

pub fn movable_io_lock<'a, T>(io: T)
        -> MovableIoLock<'a, T> where T: 'a + LockableIo<'a> {
    OwningHandle::new_with_fn(
        Box::new(RefCell::new(io)),
        |cellptr| { unsafe {
            let cellref = cellptr.as_ref().unwrap();
            let ioref = cellref.as_ptr().as_mut().unwrap();
            Box::new(ioref.movable_lock())
        }}
    )
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct IRange<Idx>(pub Idx, pub Idx);

impl<Idx> IRange<Idx> where Idx: Ord {
    #[allow(clippy::needless_pass_by_value)]
    pub fn contains_point(&self, x: Idx) -> bool {
        x >= self.0 && x <= self.1
    }

    pub fn contains_range(&self, IRange(l,r): Self) -> bool {
        self.contains_point(l) && self.contains_point(r)
    }

    pub fn intersects(&self, IRange(l,r): Self) -> bool {
        self.contains_point(l) || self.contains_point(r)
    }
}

macro_rules! gen_irange_len {
    ( $t:ty ) => (
        impl IRange<$t> {
            pub fn len(&self) -> $t {
                assert!(self.0 <= self.1);
                self.1.checked_sub(self.0).unwrap().checked_add(1).unwrap()
            }
        }
    );

    ( $t:ty, $( $ts:ty ),+ ) => (
        gen_irange_len!($t);
        gen_irange_len!( $( $ts ),+ );
    )
}

gen_irange_len!(usize, u64, u32, u16, u8, isize, i64, i32, i16, i8);

pub fn validate_stream_packet(
    packet_buff: &[u8],
    addrs: Option<(Ipv6Addr,Ipv6Addr)>
) -> bool {
    let packet = Icmpv6Packet::new(packet_buff)
        .expect("a valid length icmpv6 packet");

    if packet.get_icmpv6_type() != Icmpv6Types::EchoRequest
            || packet.get_icmpv6_code() != Icmpv6Codes::NoCode {
        debug!("invalid icmpv6 type or code field");
        return false;
    }

    if let Some((src,dst)) = addrs {
        if packet.get_checksum()
                != icmpv6::checksum(&packet, &src, &dst) {
            debug!("invalid icmpv6 checksum");
            return false;
        }
    }

    let payload = packet.payload();

    let header_constraint = ::std::cmp::min(
        STREAM_CLIENT_HEADER_SIZE,
        STREAM_SERVER_HEADER_SIZE
    );
    if payload.len() < header_constraint {
        debug!("invalid packet length");
        return false;
    }

    let checksum = BE::read_u16(&payload[0..2]);

    if checksum != ping6_data_checksum(&payload[2..]) {
        debug!("invalid protocol checksum");
        return false;
    }

    let x = payload[3];
    if StreamPacketFlags::from_bits(x).is_none() {
        debug!("invalid protocol flags");
        return false;
    }

    if payload[2] != !0 {
        debug!("invalid reserved field value");
        return false;
    }

    true
}

pub struct DerefWrapper<T>(pub T);

impl<T> Deref for DerefWrapper<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for DerefWrapper<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}
