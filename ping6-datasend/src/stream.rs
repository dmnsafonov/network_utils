use ::config::*;
use ::errors::Result;

use ::util::InitState;

pub fn stream_mode((config, src, dst, mut sock): InitState) -> Result<()> {
    let _stream_conf = extract!(ModeConfig::Stream(_), config.mode).unwrap();

    unimplemented!()
}
