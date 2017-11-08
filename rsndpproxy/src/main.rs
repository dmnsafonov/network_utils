#![recursion_limit="128"]

extern crate capabilities;
#[macro_use] extern crate clap;
extern crate env_logger;
#[macro_use] extern crate error_chain;
extern crate interfaces;
extern crate ipnetwork;
extern crate libc;
#[macro_use] extern crate log;
extern crate nix;
extern crate pnet_packet;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate syslog;
extern crate toml;
extern crate users;

extern crate linux_network;

mod config;
mod errors;
mod server;
mod util;

use std::cell::*;
use std::collections::HashMap;
use std::fs::*;
use std::process::exit;
use std::os::unix::prelude::*;
use std::rc::*;

use nix::Errno;
use nix::sys::signal::*;
use nix::sys::signalfd::*;
use nix::unistd::*;
use users::*;

use linux_network::*;
use linux_network::Permissions;
use config::*;
use errors::{ErrorKind, Result, ResultExt};
use server::*;
use util::*;

quick_main!(early_main);
fn early_main() -> Result<()> {
    let config = config::read_config()?;

    if config.daemonize {
        daemonize()?;
    }

    setup_logging(&config)?;

    log_if_err(the_main(&config));
    Ok(())
}

fn daemonize() -> Result<()> {
    if fork()?.is_parent() {
        exit(0);
    }
    setsid()?;

    chdir("/")?;

    let devnull_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null").unwrap();
    let devnull = devnull_file.as_raw_fd();
    dup2(devnull, std::io::stdin().as_raw_fd())?;
    dup2(devnull, std::io::stdout().as_raw_fd())?;
    dup2(devnull, std::io::stderr().as_raw_fd())?;

    Ok(())
}

fn setup_logging(config: &Config) -> Result<()> {
    if config.daemonize {
        let log_level = match config.verbose_logging {
            true => log::LogLevelFilter::Debug,
            false => log::LogLevelFilter::Info
        };
        syslog::init(syslog::Facility::LOG_DAEMON,
            log_level, Some(crate_name!()))?;
    } else {
        if config.verbose_logging {
            eprintln!("Using \"--verbose\" option has no effect without \
                \"--daemonize.\"  Use the RUST_LOG environment variable \
                instead.");
            exit(1);
        }

        env_logger::init()?;
    }
    Ok(())
}

fn the_main(config: &Config) -> Result<()> {
    info!("{} version {} started", crate_name!(), crate_version!());

    debug!("verbose logging on");
    debug!("configuration read from {}",
        config.config_file.to_string_lossy());
    debug!("received configuration:");
    for i in toml::to_string(&config)?.lines() {
        debug!("\t{}", i);
    }
    debug!("daemonize is {}", if config.daemonize {"on"} else {"off"});

    if get_effective_uid() != 0 {
        error!("need to be started as root");
        exit(1);
    }

    if config.daemonize {
        umask(UmaskPermissionSet::new())?;
        create_pid_file(&config.pid_file)?;
    }

    drop_privileges(&config.su)?;

    serve_requests(&config)?;

    info!("{} stopping", crate_name!());
    Ok(())
}

fn serve_requests(config: &Config) -> Result<()> {
    let signals = setup_signalfd()?;
    let epoll = EPoll::new()?;
    epoll.borrow_mut().add(Rc::clone(&signals), EPOLLIN)?;

    let mut servers = HashMap::with_capacity(config.interfaces.len());
    for i in &config.interfaces {
        let s = Server::new(i, Rc::clone(&epoll))?;
        servers.insert(s.as_raw_fd(), s);
    }

    for ev in &*epoll.borrow() {
        let fd = ev.data() as RawFd;

        if fd == signals.borrow().as_raw_fd() {
            break; // TODO
        }

        let server = servers.get_mut(&fd).unwrap();

        server.serve(ev.events());
    }

    Ok(())
}

fn create_pid_file<T>(pid_filename: T) -> Result<()>
        where T: AsRef<std::ffi::OsStr> {
    use std::io::Write;

    let pid_filename = pid_filename.as_ref();
    let pid_filename_str = pid_filename.to_string_lossy().into_owned();
    let err_arg = || ErrorKind::FileIo(pid_filename_str.clone());
    let mut pid_file = OpenOptions::new()
        .write(true)
        .create(true)
        .mode(PermissionSet::new()
            .set(Permissions::UserRead)
            .set(Permissions::UserWrite)
            .set(Permissions::GroupRead)
            .set(Permissions::OtherRead)
            .get())
        .custom_flags(FileOpenFlagSet::new()
            .set(FileOpenFlags::CloseOnExec)
            .set(FileOpenFlags::NoFollow)
            .get())
        .open(pid_filename)
        .chain_err(&err_arg)?;

    lock_file(&mut pid_file, &pid_filename_str)?;

    writeln!(pid_file, "{}", getpid())
        .chain_err(err_arg)?;

    pid_file.into_raw_fd();

    Ok(())
}

fn lock_file<T>(file: &mut File, filename: T) -> Result<()>
        where T: AsRef<str> {
    if let Err(he) = fcntl_lock_fd(file) {
        match linux_network::errors::error_to_errno(&he)
            .map(Errno::from_i32) {
                Some(Errno::EACCES) | Some(Errno::EAGAIN) => {
                    bail!(ErrorKind::AlreadyRunning(
                        filename.as_ref().to_string()));
                },
                _ => bail!(he)
        }
    }
    Ok(())
}

fn drop_privileges(su: &Option<SuTarget>) -> Result<()> {
    use capabilities::*;

    let bits = SecBitSet::new()
        .set(SecBit::NoSetuidFixup)
        .set(SecBit::NoSetuidFixupLocked)
        .set(SecBit::NoRoot)
        .set(SecBit::NoRootLocked)
        .set(SecBit::NoCapAmbientRaise)
        .set(SecBit::NoCapAmbientRaiseLocked);
    set_securebits(bits)?;

    drop_supplementary_groups()?;
    debug!("dropped supplementary groups");
    if let Some(ref su) = *su {
        let bits = get_securebits()?
            .set(SecBit::KeepCaps);
        set_securebits(bits)?;

        switch::set_current_gid(su.gid)
            .chain_err(|| ErrorKind::PrivDrop)?;
        switch::set_current_uid(su.uid)
            .chain_err(|| ErrorKind::PrivDrop)?;
        debug!("dropped uid and gid 0");
    } else {
        warn!("consider changing user with \"su = user:group\" option");
    }

    let bits = bits
        .clear(SecBit::KeepCaps)
        .set(SecBit::KeepCapsLocked);
    set_securebits(bits)?;
    debug!("securebits set to 0b{:b}", bits.get());

    set_no_new_privs()?;
    debug!("PR_SET_NO_NEW_PRIVS set");

    let mut caps = Capabilities::new()
        .chain_err(|| ErrorKind::PrivDrop)?;
    let req_caps = [
        Capability::CAP_NET_ADMIN,
        Capability::CAP_NET_BROADCAST,
        Capability::CAP_NET_RAW
    ];

    if !caps.update(&req_caps, Flag::Permitted, true) {
        bail!(ErrorKind::PrivDrop);
    }
    caps.apply()
        .chain_err(|| ErrorKind::PrivDrop)?;

    if !caps.update(&req_caps, Flag::Effective, true) {
        bail!(ErrorKind::PrivDrop);
    }
    caps.apply()
        .chain_err(|| ErrorKind::PrivDrop)?;
    debug!("dropped linux capabilities");

    // TODO: chroot, namespaces

    Ok(())
}

fn setup_signalfd() -> Result<Rc<RefCell<SignalFd>>> {
    let mut signals = SigSet::empty();
    signals.add(Signal::SIGHUP);
    signals.add(Signal::SIGINT);
    signals.add(Signal::SIGTERM);
    signals.add(Signal::SIGQUIT);
    signals.thread_block()?;
    debug!("blocked signals");

    signals.remove(Signal::SIGHUP);
    let ret = SignalFd::with_flags(&signals, SFD_NONBLOCK)?;
    debug!("set up signalfd");

    Ok(Rc::new(RefCell::new(ret)))
}
