#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]
pub mod arena;
pub mod decode;
pub mod files;
pub mod io;
mod lyrics;
pub mod network;
mod pluginTable;
mod qualities;
#[cfg(test)]
mod qualityLevelTests;
#[cfg(test)]
mod qualityTests;
pub use pluginTable::readPluginApi;
pub mod types;
pub use arena::{OwnedRequest, OwnedResult};
pub use decode::{decodeRequest, decodeResult};
pub use types::*;
#[cfg(test)]
mod lyricsTests;
#[cfg(test)]
mod tests;
