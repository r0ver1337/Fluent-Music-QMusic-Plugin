//! QMC v2 playback-format adapter, ported from music-geshizhuanhuan-main.
//! Only accepts an EKey supplied with an authorized server playback response.
//! See ../LICENSE-QMC for the MIT attribution. No key discovery or QMC v1 fallback.
use crate::protocol::fail;
use base64::{Engine, engine::general_purpose::STANDARD};
use fluent_plugin_api::ApiError;
use std::{
    io::{Read, Seek, SeekFrom, Write},
    time::Instant,
};

const EKEY_V2_PREFIX: &str = "UVFNdXNpYyBFbmNWMixLZXk6";
const EKEY_V2_KEY1: &[u8; 16] = b"386ZJY!@#*$%^&)(";
const EKEY_V2_KEY2: &[u8; 16] = b"**#!(#$%&^a1cZ,T";
// Reference simple_key_8(), evaluated with its explicit f32 semantics.
const SIMPLE_KEY: [u8; 8] = [105, 86, 70, 56, 43, 32, 21, 11];
const MAX_EKEY_BYTES: usize = 16 * 1024;
const SEGMENT: usize = 0x1400;
const BOUNDARY: u64 = 0x7fff;

fn key_error() -> ApiError {
    fail(
        "MEDIA_KEY",
        "QQ 返回的音频 EKey 无效或暂不支持；不会尝试获取其它密钥",
    )
}

mod conversion;
mod format;
mod keys;
mod stream;
#[cfg(test)]
pub(crate) use conversion::convert;
pub use conversion::convert_until;
pub use format::container;
use format::footer_size;
use keys::derive_master;
#[cfg(test)]
use keys::tea_cbc;
use stream::Stream;
#[cfg(test)]
mod tests;
