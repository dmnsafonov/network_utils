use std::ffi::*;

use clap::*;

pub struct Config {
    pub source: String,
    pub destination: String,
    pub bind_interface: Option<String>,
    pub mode: ModeConfig
}

#[derive(Clone, EnumKind)]
#[enum_kind_name(ModeConfigKind)]
pub enum ModeConfig {
    Datagram(DatagramConfig),
    Stream(StreamConfig)
}

#[derive(Clone)]
pub struct DatagramConfig {
    pub raw: bool,
    pub inline_messages: Vec<OsString>
}

#[derive(Clone)]
pub struct StreamConfig;

pub fn get_config() -> Config {
    let matches = get_args();

    let messages = match matches.values_of_os("messages") {
        Some(messages) => messages.map(OsStr::to_os_string).collect(),
        None => Vec::new()
    };

    Config {
        source: matches.value_of("source").unwrap().to_string(),
        destination: matches.value_of("destination").unwrap().to_string(),
        bind_interface: matches.value_of("bind-to-interface")
            .map(str::to_string),
        mode: if matches.is_present("stream") {
                ModeConfig::Stream(StreamConfig)
            } else {
                ModeConfig::Datagram(DatagramConfig {
                    raw: matches.is_present("raw"),
                    inline_messages: messages
                })
            }
    }
}

pub fn get_args<'a>() -> ArgMatches<'a> {
    App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(Arg::with_name("raw")
            .long("raw")
            .short("r")
            .help("Forms raw packets without payload identification")
            .conflicts_with("stream")
        ).arg(Arg::with_name("source")
            .required(true)
            .value_name("SOURCE_ADDRESS")
            .index(1)
            .help("Source address to use")
        ).arg(Arg::with_name("destination")
            .required(true)
            .value_name("DESTINATION")
            .index(2)
            .help("Messages destination")
        ).arg(Arg::with_name("messages")
            .required(true)
            .conflicts_with("use-stdin")
            .value_name("MESSAGES")
            .multiple(true)
            .index(3)
            .help("The messages to send, one argument for a packet")
        ).arg(Arg::with_name("bind-to-interface")
            .short("I")
            .long("bind-to-interface")
            .takes_value(true)
            .value_name("INTERFACE")
            .help("Binds to an interface")
        ).arg(Arg::with_name("use-stdin")
            .required(true)
            .conflicts_with("messages")
            .long("use-stdin")
            .short("c")
            .help("Instead of messages on the command-line, read from stdin \
                (prepend each message with 16-bit BE length)")
        ).arg(Arg::with_name("stream")
            .long("stream")
            .short("s")
            .help("Sets stream mode on: messages are to be read as \
                a continuous stream from stdin")
            .requires("use-stdin")
        ).get_matches()
}
