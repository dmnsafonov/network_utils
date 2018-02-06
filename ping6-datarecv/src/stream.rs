use ::config::*;
use ::errors::Result;
use ::util::InitState;

pub fn stream_mode((config, bound_addr, mut sock): InitState) -> Result<()> {
    let stream_conf = extract!(ModeConfig::Stream(_), config.mode).unwrap();

    unimplemented!()
}
