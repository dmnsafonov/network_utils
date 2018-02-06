extern crate capabilities;
#[macro_use] extern crate error_chain;
#[macro_use] extern crate log;
extern crate nix;
extern crate owning_ref;
extern crate seahash;
extern crate seccomp;

extern crate linux_network;

error_chain!(
    errors {
        Priv {
            description("privilege operation error (is cap_net_raw+p not set \
                on the executable?)")
        }
    }

    foreign_links {
        IoError(std::io::Error);
        NixError(nix::Error);
        Seccomp(seccomp::SeccompError);
    }

    links {
        LinuxNetwork (
            linux_network::errors::Error,
            linux_network::errors::ErrorKind
        );
    }
);

use std::cell::RefCell;
use std::io;
use std::net::*;
use std::os::unix::prelude::*;
use std::sync::atomic::*;

use capabilities::*;
use libc::{c_int, c_long, IPPROTO_ICMPV6};
use nix::libc;
use nix::sys::signal::*;
use owning_ref::OwningHandle;
use seccomp::*;

use linux_network::*;

pub enum Resolve {
    Yes,
    No
}

pub fn make_socket_addr<T>(addr_str: T, resolve: Resolve) -> Result<SocketAddrV6>
        where T: AsRef<str> {
    let sockaddr_in = make_sockaddr_in6_v6_dgram(
        addr_str,
        None,
        IPPROTO_ICMPV6,
        0,
        match resolve {
            Resolve::Yes => AddrInfoFlagSet::new(),
            Resolve::No => AddrInfoFlags::NumericHost.into()
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
    let err = || ErrorKind::Priv;
    let mut caps = Capabilities::from_current_proc()
        .chain_err(&err)?;
    if !caps.update(&[Capability::CAP_NET_RAW], Flag::Effective, true) {
        bail!(err());
    }
    caps.apply().chain_err(err)?;
    debug!("gained CAP_NET_RAW");
    Ok(())
}

pub fn drop_caps() -> Result<()> {
    Capabilities::new()?
        .apply()
        .chain_err(|| ErrorKind::Priv)?;
    debug!("dropped all capabilities");
    Ok(())
}

pub fn ping6_data_checksum<T>(payload: T) -> u16 where T: AsRef<[u8]> {
    use std::hash::Hasher;
    use seahash::*;
    let b = payload.as_ref();
    let mut hasher = SeaHasher::new();
    hasher.write(&u16_to_bytes_be(b.len() as u16));
    hasher.write(b);
    (hasher.finish() & 0xffff) as u16
}

static SIGNAL_FLAG: AtomicBool = ATOMIC_BOOL_INIT;

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

pub fn u16_to_bytes_be(x: u16) -> [u8; 2] {
    [
        ((x & 0xff00) >> 8) as u8,
        (x & 0xff) as u8
    ]
}

#[derive(Clone, Copy, Debug)]
pub enum StdoutUse {
    Yes,
    No
}

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
        io::Stdin::lock(self)
    }
}

impl<'a> LockableIo<'a> for io::Stdout {
    type LockType = io::StdoutLock<'a>;
    fn movable_lock(&'a mut self) -> Self::LockType {
        io::Stdout::lock(self)
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
