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
        .arg(Arg::with_name("bind")
            .long("bind")
            .short("b")
            .takes_value(true)
            .value_name("INTERFACE")
            .help("Bind to an interface")
        ).get_matches();


}
