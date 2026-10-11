//! Public transport and bounded metadata parsers; never return credentials or playback keys.
use crate::protocol::{fail, unsigned};
use crate::provider::{clean, playlist, track, validate_id};
use fluent_plugin_api::{
    ApiError, Page, Playlist, RecommendationFeed, RecommendationIssue, RecommendationSource, Track,
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::Read,
    time::{Duration, Instant},
};

const PLAZA: &str = "https://c.y.qq.com/splcloud/fcgi-bin/fcg_get_diss_by_tag.fcg";
const MODULE: &str = "https://u.y.qq.com/cgi-bin/musicu.fcg";
const PAGE_SIZE: u32 = 50;
const LIMIT: u64 = 1024 * 1024;

mod client;
mod parsing;
pub use client::PublicClient;
use parsing::{check_code, detail_request, parse_detail, parse_plaza};
pub use parsing::{cover_url, opi_feed};
#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests;
