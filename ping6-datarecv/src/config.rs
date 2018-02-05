use std::ffi::*;

use clap::*;

pub struct Config {
    pub bind_address: Option<String>,
    pub bind_interface: Option<String>,
    pub mode: ModeConfig
}

#[derive(EnumKind)]
#[enum_kind_name(ModeConfigKind)]
pub enum ModeConfig {
    Datagram(DatagramConfig),
    Stream(StreamConfig)
}

pub struct DatagramConfig {
    pub raw: bool,
    pub binary: bool
}

pub struct StreamConfig {
    pub message: Option<OsString>
}

pub fn get_config() -> Config {
    let matches = get_args();
    Config {
        bind_address: matches.value_of("bind").map(str::to_string),
        bind_interface: matches.value_of("bind-to-interface")
            .map(str::to_string),
        mode: if matches.is_present("stream") {
                ModeConfig::Stream(StreamConfig {
                    message: matches.value_of_os("message")
                        .map(OsStr::to_os_string)
                })
            } else {
                ModeConfig::Datagram(DatagramConfig {
                    raw: matches.is_present("raw"),
                    binary: matches.is_present("binary")
                })
        }
    }
}

pub fn get_args<'a>() -> ArgMatches<'a> {
    App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(Arg::with_name("bind")
            .long("bind")
            .short("-b")
            .takes_value(true)
            .value_name("ADDRESS")
            .help("Binds to an address")
        ).arg(Arg::with_name("bind-to-interface")
            .long("bind-to-interface")
            .short("I")
            .takes_value(true)
            .value_name("INTERFACE")
            .help("Binds to an interface")
        ).arg(Arg::with_name("raw")
            .long("raw")
            .short("r")
            .help("Shows all received packets' payload")
            .conflicts_with("stream")
        ).arg(Arg::with_name("binary")
            .long("binary")
            .short("B")
            .help("Outputs only the messages' contents, preceded by \
                2-byte-BE length; otherwise messages are converted to \
                unicode, filtering out any non-unicode data")
            .conflicts_with("stream")
        ).arg(Arg::with_name("stream")
            .long("stream")
            .short("s")
            .help("Sets stream mode on: message contents are written as \
                a continuous stream to stdout")
        ).arg(Arg::with_name("message")
            .long("message")
            .short("m")
            .help("Send a short (fitting in a single packet) message \
                to the sender simulteneously with accepting connection \
                in stream mode")
            .requires("stream")
        ).get_matches()
}
