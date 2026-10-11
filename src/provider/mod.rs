use crate::protocol::*;
use crate::{discovery, media, session_store::SessionStore};
use fluent_plugin_api::*;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

const QR: &str = "fcg_music_custom_sdk_get_qr_code.fcg";
const POLL: &str = "fcg_music_custom_qrcode_auth_poll.fcg";
const TOKEN: &str = "fcg_music_oauth_get_accesstoken.fcg";
const ACCOUNT: &str = "fcg_music_custom_get_account_info.fcg";
const PLAYLISTS: &str = "fcg_music_custom_get_songlist_self.fcg";
const DETAIL: &str = "fcg_music_custom_get_songlist_detail.fcg";
const SEARCH: &str = "fcg_music_custom_search.fcg";
const RECOMMEND: &str = "fcg_music_custom_individual_radio.fcg";
const HOMEPAGE_PERMISSION_COOLDOWN: Duration = Duration::from_secs(60);
const RESOLVE: &str = "fcg_music_custom_get_song_info_batch.fcg";
mod authentication;
mod diagnostics;
mod lyrics;
mod lyrics_diagnostic;
mod metadata;
mod playlist;
mod qualities;
mod recommendations;
mod request;
mod signed_homepage;
mod state;
#[cfg(test)]
mod tests;
mod tracks;
#[cfg(test)]
use authentication::{account_avatar_url, account_profile};
#[cfg(test)]
use diagnostics::saved_session_status;
pub use diagnostics::{
    diagnose_recommendation_account, diagnose_recommendations, diagnose_saved_session,
};
pub use lyrics_diagnostic::{capture_lyrics, replay_lyrics};
use metadata::library_playlists;
pub(crate) use metadata::{clean, playlist, track, validate_id};
use recommendations::finish_feed;

struct Pending {
    id: String,
    code: String,
    deadline: Instant,
    next_poll: Instant,
    state: LoginState,
}
pub struct Provider {
    client: Option<Client>,
    session: Option<Session>,
    pending: Option<Pending>,
    tokens: HashMap<String, String>,
    updates: HashMap<String, u64>,
    media: media::Adapter,
    public: Option<discovery::PublicClient>,
    public_playlists: HashSet<String>,
    store: Option<SessionStore>,
    storage_issue: Option<ApiError>,
    restored: bool,
    generation: Option<String>,
    persistence_dirty: bool,
    // Application-level denial only; never persisted with or cleared as user credentials.
    homepage_denial: Option<(Instant, ApiError)>,
    #[cfg(test)]
    signed_homepage_enabled: bool,
    recommendation_client: Option<crate::recommendation::RecommendationClient>,
}
impl Provider {
    pub fn new() -> Self {
        Self {
            client: None,
            session: None,
            pending: None,
            tokens: HashMap::new(),
            updates: HashMap::new(),
            media: media::Adapter::default(),
            public: None,
            public_playlists: HashSet::new(),
            store: None,
            storage_issue: None,
            restored: false,
            generation: None,
            persistence_dirty: false,
            homepage_denial: None,
            #[cfg(test)]
            signed_homepage_enabled: true,
            recommendation_client: None,
        }
    }
    pub fn persistent() -> Self {
        let mut provider = Self::new();
        match SessionStore::from_environment() {
            Ok(store) => provider.store = store,
            Err(error) => provider.storage_issue = Some(error),
        }
        provider
    }
    fn uses_signed_homepage(&self) -> bool {
        #[cfg(test)]
        {
            self.signed_homepage_enabled
        }
        #[cfg(not(test))]
        {
            true
        }
    }
    pub fn handle(&mut self, op: Operation) -> Result<ResultData, ApiError> {
        if matches!(op, Operation::Describe) {
            return Ok(ResultData::Description(PluginInfo {
                id: "qqmusic".into(),
                name: "QQ音乐".into(),
                version: "0.1.0".into(),
                api_version: API_VERSION,
                capabilities: vec![
                    Capability::Login,
                    Capability::Account,
                    Capability::Search,
                    Capability::Playlists,
                    Capability::Playlist,
                    Capability::Recommendations,
                    Capability::Resolve,
                ],
                extensions: vec![
                    Capability::Homepage,
                    Capability::PlaylistRecommendations,
                    Capability::Lyrics,
                    Capability::TrackQualities,
                ],
            }));
        }
        if matches!(op, Operation::TrackQualities { track: None }) {
            return Ok(ResultData::TrackQualities(media::qualities::catalogue()));
        }
        if matches!(op, Operation::Logout) {
            self.clear_private_state();
            self.restored = true; // Never reload the saved session after an explicit logout.
            if let Some(store) = &self.store {
                if let Err(error) = store.clear() {
                    self.storage_issue = Some(error.clone());
                    return Err(error);
                }
                self.storage_issue = None;
            } else if let Some(error) = &self.storage_issue {
                return Err(error.clone());
            }
            return Ok(ResultData::Ok);
        }
        if !matches!(op, Operation::LoginBegin | Operation::LoginPoll { .. }) {
            self.restore_once();
        }
        // Public browsing requires no credentials; a saved authorized session is optional.
        match &op {
            Operation::Lyrics { track } => return self.get_lyrics(track),
            Operation::Homepage | Operation::PlaylistRecommendations => {
                return (if self.uses_signed_homepage() {
                    self.signed_recommendation_feed()
                } else {
                    self.recommendation_feed()
                })
                .map(ResultData::RecommendationFeed);
            }
            Operation::Recommendations if self.uses_signed_homepage() => {
                let feed = self.signed_recommendation_feed()?;
                return Ok(ResultData::Tracks(Page {
                    items: feed.tracks,
                    page: 0,
                    has_more: false,
                    cursor: None,
                }));
            }
            Operation::Recommendations if self.session.is_none() => {
                let feed = self.public_feed()?;
                return Ok(ResultData::Tracks(Page {
                    items: feed.tracks,
                    page: 0,
                    has_more: false,
                    cursor: None,
                }));
            }
            Operation::Playlist { id, page, .. }
                if id != "0" && (self.session.is_none() || self.public_playlists.contains(id)) =>
            {
                return self
                    .public_client()?
                    .playlist(id, *page)
                    .map(ResultData::Tracks);
            }
            _ => {}
        }
        if matches!(
            op,
            Operation::Resolve { .. }
                | Operation::TrackQualities { .. }
                | Operation::ResolveWithQuality { .. }
                | Operation::Playlists
                | Operation::Playlist { .. }
                | Operation::Account
        ) && self.session.is_none()
        {
            if let Some(error) = &self.storage_issue {
                return Err(error.clone());
            }
            return Err(fail(
                "LOGIN_REQUIRED",
                "浏览推荐无需登录；播放和账号歌单请先扫码授权",
            ));
        }
        if self.client.is_none() {
            self.client = Some(Client::new(Config::bundled())?);
        }
        match op {
            Operation::LoginBegin => self.login_begin(),
            Operation::LoginPoll { challenge_id } => self.login_poll(&challenge_id),
            Operation::Account => {
                self.ensure_session()?;
                Ok(ResultData::Account(self.account()?))
            }
            Operation::Playlists => {
                self.ensure_session()?;
                let data = self.request(PLAYLISTS, &[])?;
                Ok(ResultData::Playlists(library_playlists(&data)?))
            }
            Operation::Playlist { id, page, cursor } => self.playlist_details(id, page, cursor),
            Operation::Search { query, page } => {
                self.ensure_session()?;
                if query.trim().is_empty() || query.len() > 1024 || page > 100000 {
                    return Err(fail("PARAMETERS", "搜索词为空、过长或页码无效"));
                }
                let data = self.request(
                    SEARCH,
                    &[
                        ("w", query),
                        ("p", (page + 1).to_string()),
                        ("t", "0".into()),
                        ("num", "50".into()),
                        ("source", "0".into()),
                        ("exclude_types", "".into()),
                        ("file_format", "0".into()),
                        ("max_file_type", "50".into()),
                        ("support_dolby", "0".into()),
                    ],
                )?;
                self.track_page(&data, "list", page)
            }
            Operation::Recommendations => {
                self.ensure_session()?;
                let scene = self.client.as_ref().unwrap().config.scene;
                let data = self.request(RECOMMEND, &[("scene", scene.to_string())])?;
                self.track_page(&data, "songlist", 0)
            }
            Operation::Resolve { track } => self.resolve(track),
            Operation::TrackQualities { track: Some(track) } => self.get_track_qualities(&track),
            Operation::ResolveWithQuality {
                track,
                qualityId: quality_id,
            } => self.resolve_with_quality(track, &quality_id),
            Operation::TrackQualities { track: None } => unreachable!(),
            Operation::Describe
            | Operation::Logout
            | Operation::Homepage
            | Operation::PlaylistRecommendations => unreachable!(),
            Operation::Lyrics { .. } => unreachable!(),
        }
    }
}
