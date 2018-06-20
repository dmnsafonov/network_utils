#![recursion_limit="128"]

#[macro_use] extern crate bitflags;
#[macro_use] extern crate boolean_enums;
extern crate capabilities;
#[macro_use] extern crate clap;
extern crate env_logger;
#[macro_use] extern crate failure;
#[macro_use] extern crate futures;
extern crate interfaces;
extern crate ip_network;
#[macro_use] extern crate log;
extern crate nix;
extern crate pnet_packet;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate syslog;
extern crate tokio;
extern crate tokio_timer;
extern crate tokio_signal;
extern crate toml;
extern crate users;

extern crate linux_network;
extern crate send_box;

mod broadcast;
mod config;
mod constants;
mod errors;
mod packet;
mod server;
mod util;

use std::fs::*;
use std::io;
use std::process::exit;
use std::os::unix::prelude::*;
use std::sync::Arc;

use failure::ResultExt;
use futures::future::*;
use futures::prelude::*;
use log::LogLevel::*;
use nix::Errno;
use nix::libc;
use nix::unistd::*;
use tokio_signal::unix as signal;
use users::*;

use linux_network::*;
use linux_network::Permissions;

use broadcast::*;
use config::*;
use errors::{Error, Result};
use server::*;
use util::*;

fn main() {
    if let Err(e) = early_main() {
        let mut first = true;;
        for i in e.causes() {
            if !first {
                eprint!(": ");
            }
            eprint!("{}", i);
            first = false;
        }
        eprintln!("");
    }
}

fn early_main() -> Result<()> {
    let config = config::read_config()?;

    if config.daemonize {
        daemonize()?;
    }

    setup_logging(&config)?;

    log_if_err(the_main(config));
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

fn the_main(config: Config) -> Result<()> {
    info!("{} version {} started", crate_name!(), crate_version!());

    let config = Arc::new(config);

    if log_enabled!(Debug) {
        debug!("verbose logging on");
        debug!("configuration read from {}",
            config.config_file.to_string_lossy());
        debug!("received configuration:");
        for i in toml::to_string(&*config)?.lines() {
            debug!("\t{}", i);
        }
        debug!("daemonize is {}", if config.daemonize {"on"} else {"off"});
    }

    if get_effective_uid() != 0 {
        error!("need to be started as root");
        exit(1);
    }

    if config.daemonize {
        umask(UmaskPermissions::empty()).context("setting umask failed")?;
        create_pid_file(&config.pid_file)?;
    }

    drop_privileges(&config.su)?;

    if config.interfaces.len() == 0 {
        bail!("You must configure at least one interface.");
    }

    let config_clone = config.clone();
    tokio::run(poll_fn(move || {
        setup_server(config_clone.clone()).map_err(|e| log_err(e))?;
        Ok(Async::Ready(()))
    }));

    info!("{} stopping", crate_name!());
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QuitKind {
    Fast, Normal
}

fn setup_server(config: Arc<Config>) -> Result<()> {
    let (quit_rx, quit_tx) = broadcaster(config.interfaces.len());

    handle_signals(quit_tx);
    handle_requests(&config.interfaces, quit_rx);

    Ok(())
}

fn handle_signals(mut sender: Sender<QuitKind>) {
    let mut interrupted = signal::Signal::new(signal::SIGINT).flatten_stream();
    let mut terminated = signal::Signal::new(signal::SIGTERM).flatten_stream();

    let mut to_send = None;
    let mut fast_quit_sent = false;
    tokio::spawn(
        poll_fn(move || {
            let mut active = true;
            while active {
                active = false;

                if let Some(q) = to_send {
                    if sender.start_send(q)?.is_ready() {
                        debug!("sent a quit broadcast to workers");
                        to_send.take();
                        active = true;
                    }
                }

                if !fast_quit_sent {
                    if let Async::Ready(s) = interrupted.poll()
                            .map_err(Error::SignalIOError)? {
                        assert_eq!(s.unwrap(), signal::SIGINT);
                        debug!("received SIGINT");
                        to_send = Some(QuitKind::Normal);
                        active = true;
                    }
                    if let Async::Ready(s) = terminated.poll()
                            .map_err(Error::SignalIOError)? {
                        assert_eq!(s.unwrap(), signal::SIGTERM);
                        debug!("received SIGTERM");
                        to_send = Some(QuitKind::Fast);
                        fast_quit_sent = true;
                        active = true;
                    }
                }

                if !sender.are_receivers_present() {
                    debug!("no signal broadcast readers, \
                        the broadcast worker quitting");
                    return Ok(Async::Ready(()));
                }
            }

            Ok(Async::NotReady)
        }).map_err(log_err)
    );
}

fn handle_requests(interfaces: &[InterfaceConfig], quit: Receiver<QuitKind>) {
    let mut quit = Some(quit);
    for i in 0 .. interfaces.len() {
        let j = interfaces[i].clone();
        let quit_to_move =
            if i == interfaces.len() - 1 {
                quit.take().unwrap()
            } else {
                quit.clone().unwrap()
            };
        tokio::spawn(
            result(
                Server::new(
                    &j,
                    quit_to_move
                ).map_err(|e| log_err(e))
            ).flatten()
        );
        debug!("server for interface {} started", interfaces[i].name);
    }
}

fn create_pid_file<T>(pid_filename: T) -> Result<()>
        where T: AsRef<std::ffi::OsStr> {
    use std::io::Write;

    let pid_filename = pid_filename.as_ref();
    let pid_filename_str = pid_filename.to_string_lossy().into_owned();
    let err_arg = |e| Error::FileIo {
        name: pid_filename_str.clone(),
        cause: e
    };
    let mut pid_file = OpenOptions::new()
        .write(true)
        .create(true)
        .mode(
            (Permissions::UserRead
            | Permissions::UserWrite
            | Permissions::GroupRead
            | Permissions::OtherRead
            ).bits()
        ).custom_flags(
            (FileOpenFlags::CloseOnExec
            | FileOpenFlags::NoFollow
            ).bits()
        ).open(pid_filename)
        .map_err(&err_arg)?;

    lock_file(&mut pid_file, &pid_filename_str)?;

    writeln!(pid_file, "{}", getpid())
        .map_err(err_arg)?;

    pid_file.into_raw_fd();

    Ok(())
}

fn lock_file<T>(file: &mut File, filename: T) -> Result<()>
        where T: AsRef<str> {
    if let Err(he) = fcntl_lock_fd(file) {
        let errno = linux_network::errors::error_to_errno(
            he.downcast_ref::<linux_network::errors::Error>().unwrap()
        ).map(Errno::from_i32);
        match errno {
            Some(Errno::EACCES) | Some(Errno::EAGAIN) => {
                bail!(Error::AlreadyRunning {
                    filename: filename.as_ref().to_string()
                });
            },
            _ => bail!(he)
        }
    }
    Ok(())
}

fn drop_privileges(su: &Option<SuTarget>) -> Result<()> {
    use capabilities::*;

    let mut bits =
        SecBits::NoSetuidFixup
        | SecBits::NoSetuidFixupLocked
        | SecBits::NoRoot
        | SecBits::NoRootLocked
        | SecBits::NoCapAmbientRaise
        | SecBits::NoCapAmbientRaiseLocked;
    set_securebits(bits)
        .map_err(|e| Error::SecurebitsError(
            ::failure::Error::from(e).compat())
        )?;

    drop_supplementary_groups().context("cannot drop supplementary groups")?;
    debug!("dropped supplementary groups");
    if let Some(ref su) = *su {
        let bits = get_securebits()
                .map_err(|e| Error::SecurebitsError(
                    ::failure::Error::from(e).compat())
                )?
            | SecBits::KeepCaps;
        set_securebits(bits)
            .map_err(|e| Error::SecurebitsError(
                ::failure::Error::from(e).compat())
            )?;

        switch::set_current_gid(su.gid).map_err(Error::PrivDrop)?;
        switch::set_current_uid(su.uid).map_err(Error::PrivDrop)?;
        debug!("dropped uid and gid 0");
    } else {
        warn!("consider changing user with \"su = user:group\" option");
    }

    bits.remove(SecBits::KeepCaps);
    bits.insert(SecBits::KeepCapsLocked);
    set_securebits(bits)
        .map_err(|e| Error::SecurebitsError(
            ::failure::Error::from(e).compat())
        )?;
    debug!("securebits set to {:?}", bits);

    set_no_new_privs().context("cannot set NO_NEW_PRIVS")?;
    debug!("PR_SET_NO_NEW_PRIVS set");

    let mut caps = Capabilities::new().map_err(Error::PrivDrop)?;
    let req_caps = [
        Capability::CAP_NET_ADMIN,
        Capability::CAP_NET_BROADCAST,
        Capability::CAP_NET_RAW
    ];

    if !caps.update(&req_caps, Flag::Permitted, true) {
        bail!(Error::PrivDrop(io::Error::new(
            io::ErrorKind::Other,
            "cannot update a capset"
        )));
    }
    caps.apply().map_err(Error::PrivDrop)?;

    if !caps.update(&req_caps, Flag::Effective, true) {
        bail!(Error::PrivDrop(io::Error::new(
            io::ErrorKind::Other,
            "cannot update a capset"
        )));
    }
    caps.apply().map_err(Error::PrivDrop)?;
    debug!("dropped linux capabilities");

    // TODO: chroot, namespaces

    Ok(())
}
