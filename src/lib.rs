mod artwork;
#[cfg(not(test))]
use fluent_plugin_abi::network::http;
#[cfg(test)]
use reqwest::blocking as http;
mod broker_media;
pub mod compatibility;
mod discovery;
mod ffi;
mod lyrics;
mod media;
mod plugin_config;
mod protocol;
mod provider;
mod qmc;
pub mod qrc;
mod recommendation;
mod session_store;
pub use compatibility::legacy_api::*;
pub use ffi::fmPluginGetApi;
pub use provider::{
    capture_lyrics, diagnose_recommendation_account, diagnose_recommendations,
    diagnose_saved_session, replay_lyrics,
};
