use ::std::ffi::OsString;
use ::std::str::FromStr;
use ::std::sync::Arc;

use ::clap::{App, Arg};
use ::ip_network::Ipv6Network;
use ::libc::{uid_t, gid_t};
use ::serde::*;
use ::serde::de::Visitor;

use super::errors::{Error, Result};

const DEFAULT_CONFIG_PATH: &str = "/etc/rsndpproxy.conf";
const DEFAULT_PID_PATH: &str = "/run/rsndpproxy.pid";

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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InterfaceConfig {
    pub name: String,
    #[serde(default = "DEFAULT_MAX_QUEUED")] pub max_queued: usize,
    #[serde(rename = "prefix")] pub prefixes: Vec<Arc<PrefixConfig>>
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PrefixConfig {
    #[serde(serialize_with="serialize_ipnetwork")]
    #[serde(deserialize_with="deserialize_ipnetwork")]
    pub prefix: Ipv6Network,
    #[serde(rename = "router")]
    #[serde(default)]
    pub router_flag: Router,
    #[serde(rename = "reply-unconditionally")]
    #[serde(default)]
    pub reply_unconditionally: bool,
    #[serde(rename = "override")]
    #[serde(default)]
    pub override_flag: Override
}

gen_boolean_enum!(pub serde Override);
gen_boolean_enum!(pub serde Router);

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

fn serialize_ipnetwork<S>(netw: &Ipv6Network, serializer: S)
        -> ::std::result::Result<S::Ok, S::Error> where S: Serializer {
    serializer.serialize_str(&format!("{}", netw))
}

fn deserialize_ipnetwork<'de, D>(deserializer: D)
        -> ::std::result::Result<Ipv6Network, D::Error>
        where D: Deserializer<'de> {
    deserializer.deserialize_str(IpNetworkVisitor)
}

struct IpNetworkVisitor;
impl<'de> Visitor<'de> for IpNetworkVisitor {
    type Value = Ipv6Network;

    fn expecting(&self, formatter: &mut ::std::fmt::Formatter)
            -> ::std::fmt::Result {
        formatter.write_str("a IPv6 prefix")
    }

    fn visit_str<E>(self, value: &str)
            -> ::std::result::Result<Self::Value, E>
            where E: ::serde::de::Error {
        Ipv6Network::from_str(value)
            .map_err(|e|
                E::custom(format!("not a valid IPv6 network prefix: {}", e))
            )
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
        .map_err(|e| Error::FileIo {
            name: config_filename_str.clone(),
            cause: e
        })?;
    let mut config_str = String::new();
    config_file.read_to_string(&mut config_str)
        .map_err(|e| Error::FileIo {
            name: config_filename_str.clone(),
            cause: e
        })?;

    let mut config: Config = ::toml::from_str(&config_str)?;
    config.config_file = config_filename.into();
    config.daemonize = matches.is_present("daemonize");
    config.pid_file = matches.value_of_os("pid").unwrap().into();
    config.verbose_logging = matches.is_present("verbose");

    Ok(config)
}
