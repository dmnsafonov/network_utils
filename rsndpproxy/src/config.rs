use ::std::ffi::OsString;
use ::std::str::FromStr;

use ::clap::{App, Arg};
use ::ipnetwork::Ipv6Network;
use ::libc::{uid_t, gid_t};
use ::serde::*;
use ::serde::de::Visitor;

use super::errors::{Error, ErrorKind, Result, ResultExt};

const DEFAULT_CONFIG_PATH: &str = "/etc/rsndppd.conf";
const DEFAULT_PID_PATH: &str = "/run/rsndppd.pid";

#[allow(non_snake_case)]
fn DEFAULT_MAX_QUEUED() -> usize { 42 }

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(skip)] pub config_file: OsString,
    #[serde(skip)] pub daemonize: bool,
    #[serde(skip)] pub pid_file: OsString,
    #[serde(skip)] pub verbose_logging: bool,
    pub su: Option<SuTarget>,
    #[serde(rename = "interface")] pub interfaces: Vec<InterfaceConfig>
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InterfaceConfig {
    pub name: String,
    #[serde(default = "DEFAULT_MAX_QUEUED")] pub max_queued: usize,
    #[serde(rename = "prefix")] pub prefixes: Vec<PrefixConfig>
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PrefixConfig {
    pub prefix: Ipv6Prefix,
    #[serde(default)] pub router: bool
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct Ipv6Prefix(Ipv6Network);

impl Serialize for Ipv6Prefix {
    fn serialize<S>(&self, serializer: S)
            -> ::std::result::Result<S::Ok, S::Error>
            where S: Serializer {
        serializer.serialize_str(&format!("{}", self.0))
    }
}

impl<'de> Deserialize<'de> for Ipv6Prefix {
    fn deserialize<D>(deserializer: D)
            -> ::std::result::Result<Ipv6Prefix, D::Error>
            where D: Deserializer<'de> {
        deserializer.deserialize_str(Ipv6PrefixVisitor)
    }
}

struct Ipv6PrefixVisitor;
impl<'de> Visitor<'de> for Ipv6PrefixVisitor {
    type Value = Ipv6Prefix;

    fn expecting(&self, formatter: &mut ::std::fmt::Formatter)
            -> ::std::fmt::Result {
        formatter.write_str("an IPv6 prefix")
    }

    fn visit_str<E>(self, value: &str)
            -> ::std::result::Result<Self::Value, E>
            where E: ::serde::de::Error {
        Ipv6Network::from_str(value)
            .map(Ipv6Prefix)
            .map_err(|e| E::custom(Error::from(e)))
    }
}

#[derive(Clone, Debug)]
pub struct SuTarget {
    pub name: String,
    pub uid: uid_t,
    pub gid: gid_t
}

impl Serialize for SuTarget {
    fn serialize<S>(&self, serializer: S)
            -> ::std::result::Result<S::Ok, S::Error>
            where S: Serializer {
        serializer.serialize_str(&self.name)
    }
}

impl<'de> Deserialize<'de> for SuTarget {
    fn deserialize<D>(deserializer: D)
            -> ::std::result::Result<SuTarget, D::Error>
            where D: Deserializer<'de> {
        deserializer.deserialize_str(SuTargetVisitor)
    }
}

struct SuTargetVisitor;
impl<'de> Visitor<'de> for SuTargetVisitor {
    type Value = SuTarget;

    fn expecting(&self, formatter: &mut ::std::fmt::Formatter)
            -> ::std::fmt::Result {
        formatter.write_str("a username, group name pair")
    }

    fn visit_str<E>(self, value: &str)
            -> ::std::result::Result<Self::Value, E>
            where E: ::serde::de::Error {
        let mut i = value.split(':');
        let user_str = match i.next() {
            Some(x) => x,
            None => return Err(E::custom(
                format!("no username in string {}", value)))
        };

        let group_str = match i.next() {
            Some(x) => if x.is_empty() {user_str} else {x},
            None => user_str
        };

        if i.next().is_some() {
            return Err(E::custom(
                format!("not a valid username, group name pair: {}", value)));
        }

        let user = match ::users::get_user_by_name(user_str) {
            Some(x) => x,
            None => return Err(E::custom(format!("no user {}", user_str)))
        };

        let group = match ::users::get_group_by_name(group_str) {
            Some(x) => x,
            None => return Err(E::custom(format!("no group {}", group_str)))
        };

        Ok(SuTarget {
            name: value.into(),
            uid: user.uid(),
            gid: group.gid()
        })
    }
}

pub fn read_config() -> Result<Config> {
    use std::io::Read;

    let matches = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(Arg::with_name("config")
            .short("c")
            .long("config")
            .takes_value(true)
            .value_name("FILE")
            .default_value(DEFAULT_CONFIG_PATH)
            .help("Sets configuration file name")
        ).arg(Arg::with_name("daemonize")
            .short("d")
            .long("daemonize")
            .help("Forks and sets log to syslog instead of the console")
        ).arg(Arg::with_name("pid")
            .short("p")
            .long("pid-file")
            .takes_value(true)
            .value_name("FILE")
            .default_value(DEFAULT_PID_PATH)
            .help("Sets pid file name")
        ).arg(Arg::with_name("verbose")
            .short("v")
            .long("verbose")
            .help("Enables extremely verbose logging when daemonizing.  \
                Use RUST_LOG for the console logging")
        ).get_matches();

    let config_filename = matches.value_of_os("config").unwrap();
    let config_filename_str = config_filename.to_string_lossy().into_owned();
    let mut config_file = ::std::fs::File::open(config_filename)
        .chain_err(|| ErrorKind::FileIo(config_filename_str.clone()))?;
    let mut config_str = String::new();
    config_file.read_to_string(&mut config_str)
        .chain_err(|| ErrorKind::FileIo(config_filename_str.clone()))?;

    let mut config: Config = ::toml::from_str(&config_str)?;
    config.config_file = config_filename.into();
    config.daemonize = matches.is_present("daemonize");
    config.pid_file = matches.value_of_os("pid").unwrap().into();
    config.verbose_logging = matches.is_present("verbose");

    Ok(config)
}
