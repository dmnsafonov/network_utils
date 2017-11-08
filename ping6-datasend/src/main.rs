#[macro_use] extern crate clap;
extern crate env_logger;
#[macro_use] extern crate log;

extern crate linux_network;

use clap::*;
use linux_network::*;

fn main() {
    env_logger::init();

    let matches = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(Arg::with_name("destination")
            .required(true)
            .value_name("DESTINATION")
            .index(1)
            .help("Messages destination")
        )
        .arg(Arg::with_name("messages")
            .required(true)
            .value_name("MESSAGES")
            .multiple(true)
            .index(2)
            .help("The messages to send, one argument for a packet")
        ).get_matches();


}
