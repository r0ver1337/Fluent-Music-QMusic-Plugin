use fluent_plugin_api::ApiError;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

mod client;
mod config;
mod response;
mod session;
mod signing;
#[cfg(test)]
mod tests;
pub use client::Client;
pub use config::Config;
use response::homepage_success;
pub use response::{field, status, success, unsigned};
pub use session::{Session, decrypt, valid_music_id};
pub use signing::{encode, hash, signed};

pub const HOMEPAGE: &str = "fcg_music_custom_homepage_song_list.fcg";
pub fn fail(code: &str, message: &str) -> ApiError {
    ApiError::new(code, message)
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
