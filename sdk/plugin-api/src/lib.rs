//! Stable, language-independent provider protocol. No service-specific fields or credentials.
#![allow(non_snake_case)]
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod lyrics;
pub use lyrics::{LyricLine, LyricTimeline, LyricWord, Lyrics};
mod qualities;
pub use qualities::{QualityLevel, QualityList, TrackQuality, maxQualityEntries};
#[cfg(test)]
mod lyricsTests;

pub const API_VERSION: u32 = 2;
pub const MAX_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Login,
    Account,
    Search,
    Playlists,
    Playlist,
    Recommendations,
    Homepage,
    PlaylistRecommendations,
    Resolve,
    Lyrics,
    TrackQualities,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub capabilities: Vec<Capability>,
    /// Optional capabilities retained separately for metadata compatibility.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<Capability>,
}
impl PluginInfo {
    pub fn supports(&self, capability: &Capability) -> bool {
        self.capabilities.contains(capability) || self.extensions.contains(capability)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub api_version: u32,
    pub id: String,
    /// Executable relative to this manifest; no shell, PATH search, or traversal.
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Request {
    pub api_version: u32,
    pub id: u64,
    #[serde(flatten)]
    pub operation: Operation,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Operation {
    Describe,
    LoginBegin,
    LoginPoll {
        challenge_id: String,
    },
    Logout,
    Account,
    Search {
        query: String,
        page: u32,
    },
    Playlists,
    Playlist {
        id: String,
        page: u32,
        cursor: Option<String>,
    },
    Recommendations,
    Homepage,
    PlaylistRecommendations,
    Resolve {
        track: Track,
    },
    Lyrics {
        track: Track,
    },
    TrackQualities {
        track: Option<Track>,
    },
    ResolveWithQuality {
        track: Track,
        #[serde(rename = "qualityId")]
        qualityId: String,
    },
}

impl Operation {
    pub fn capability(&self) -> Option<Capability> {
        Some(match self {
            Self::Describe | Self::Logout => return None,
            Self::LoginBegin | Self::LoginPoll { .. } => Capability::Login,
            Self::Account => Capability::Account,
            Self::Search { .. } => Capability::Search,
            Self::Playlists => Capability::Playlists,
            Self::Playlist { .. } => Capability::Playlist,
            Self::Recommendations => Capability::Recommendations,
            Self::Homepage => Capability::Homepage,
            Self::PlaylistRecommendations => Capability::PlaylistRecommendations,
            Self::Resolve { .. } => Capability::Resolve,
            Self::Lyrics { .. } => Capability::Lyrics,
            Self::TrackQualities { .. } | Self::ResolveWithQuality { .. } => {
                Capability::TrackQualities
            }
        })
    }
    pub fn accepts(&self, result: &ResultData) -> bool {
        matches!(
            (self, result),
            (Self::Describe, ResultData::Description(_))
                | (Self::LoginBegin, ResultData::LoginChallenge(_))
                | (Self::LoginPoll { .. }, ResultData::LoginState(_))
                | (Self::Logout, ResultData::Ok)
                | (Self::Account, ResultData::Account(_))
                | (Self::Playlists, ResultData::Playlists(_))
                | (
                    Self::Search { .. } | Self::Playlist { .. } | Self::Recommendations,
                    ResultData::Tracks(_)
                )
                | (Self::Resolve { .. }, ResultData::Media(_))
                | (Self::Lyrics { .. }, ResultData::Lyrics(_))
                | (Self::TrackQualities { .. }, ResultData::TrackQualities(_))
                | (Self::ResolveWithQuality { .. }, ResultData::Media(_))
                | (
                    Self::Homepage | Self::PlaylistRecommendations,
                    ResultData::RecommendationFeed(_)
                )
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub artists: Vec<String>,
    #[serde(default)]
    pub album: String,
    pub duration_ms: Option<u64>,
    pub provider_id: String,
    pub mid: Option<String>,
    /// Public HTTPS album artwork, independent of playback authorization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    /// Optional smaller version of the same cover, preferred in smooth mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    pub track_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
}

/// Optional discovery extension. Browsing is independent of playback authorization.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationSource {
    #[default]
    Unknown,
    Public,
    /// A recommendation request actually made using the current account, not a personalization guarantee.
    Account,
    Homepage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecommendationIssue {
    /// Fixed diagnostic stage, never a URL, user identifier or remote response text.
    pub stage: String,
    pub error: ApiError,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RecommendationFeed {
    #[serde(default)]
    pub playlists: Vec<Playlist>,
    #[serde(default)]
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub personalized: bool,
    #[serde(default)]
    pub notice: Option<String>,
    #[serde(default)]
    pub playlist_source: RecommendationSource,
    #[serde(default)]
    pub track_source: RecommendationSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<RecommendationIssue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub has_more: bool,
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub display_name: String,
    /// Optional public HTTPS image. Never contains login credentials or requires auth headers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LoginChallenge {
    pub challenge_id: String,
    pub qr_text: String,
    pub expires_in_seconds: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "state", content = "account", rename_all = "snake_case")]
pub enum LoginState {
    Waiting,
    Scanned,
    Authorized(Account),
    Expired,
}

/// Sensitive short-lived locators; deliberately no Debug implementation.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaSource {
    ManagedFile {
        id: u64,
    },
    File {
        path: String,
    },
    Http {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ResultData {
    Description(PluginInfo),
    LoginChallenge(LoginChallenge),
    LoginState(LoginState),
    Account(Account),
    Playlists(Vec<Playlist>),
    Tracks(Page<Track>),
    RecommendationFeed(RecommendationFeed),
    Media(MediaSource),
    Lyrics(Lyrics),
    TrackQualities(QualityList),
    Ok,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    /// Safe user-facing message; never raw HTTP errors or signed URLs.
    pub message: String,
}
impl ApiError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ApiError {}

#[derive(Serialize, Deserialize)]
pub struct Response {
    pub api_version: u32,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ResultData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}
impl Response {
    pub fn new(id: u64, result: Result<ResultData, ApiError>) -> Self {
        let (result, error) = match result {
            Ok(r) => (Some(r), None),
            Err(e) => (None, Some(e)),
        };
        Self {
            api_version: API_VERSION,
            id,
            result,
            error,
        }
    }
}

mod transport;
pub use transport::{readFrame as read_frame, serve};
#[cfg(test)]
mod tests;
