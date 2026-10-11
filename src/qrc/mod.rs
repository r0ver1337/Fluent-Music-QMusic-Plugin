mod decrypt;
mod des;
mod legacy_api;
mod legacy_xml;
mod parser;
mod tables;
mod xml;

use fluent_plugin_api::ApiError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use decrypt::{decrypt_qrc_api, decrypt_qrc_hex};
pub use legacy_api::*;
pub use parser::parse_qrc;

const MAX_ENCRYPTED_BYTES: usize = 512 * 1024;
const MAX_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_LINES: usize = 10_000;
const MAX_WORDS: usize = 100_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrcLyric {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub offset_ms: i64,
    pub metadata: BTreeMap<String, String>,
    pub lines: Vec<QrcLine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrcLine {
    pub start_ms: i64,
    pub duration_ms: u64,
    pub text: String,
    pub words: Vec<QrcWord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrcWord {
    pub start_ms: u64,
    pub duration_ms: u64,
    pub text: String,
    pub start_utf16: usize,
    pub end_utf16: usize,
}

pub fn decode_qrc_api(value: &str) -> Result<QrcLyric, ApiError> {
    parse_qrc(&decrypt_qrc_api(value)?)
}

pub fn decode_qrc_hex(value: &str) -> Result<QrcLyric, ApiError> {
    parse_qrc(&decrypt_qrc_hex(value)?)
}

fn format_error() -> ApiError {
    ApiError::new("LYRICS_FORMAT", "QRC 歌词结构或时间数据无效")
}

fn limit_error() -> ApiError {
    ApiError::new("LYRICS_LIMIT", "QRC 歌词超过本地处理限制")
}
