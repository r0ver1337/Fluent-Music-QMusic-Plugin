use crate::types::*;
use fluent_plugin_api::*;
use std::{any::Any, ffi::c_void};

#[derive(Default)]
pub struct Arena {
    Values: Vec<Box<dyn Any>>,
}
impl Arena {
    pub fn keep<T: 'static>(&mut self, value: T) -> *const T {
        let value = Box::new(value);
        let pointer = &*value as *const T;
        self.Values.push(value);
        pointer
    }
    pub fn text(&mut self, text: &str) -> fmStringView {
        let bytes = text.as_bytes().to_vec();
        let view = fmStringView {
            data: bytes.as_ptr(),
            length: bytes.len() as u64,
        };
        self.Values.push(Box::new(bytes));
        view
    }
    pub fn optional(&mut self, text: &Option<String>) -> fmStringView {
        self.text(text.as_deref().unwrap_or_default())
    }
    pub fn array<T: 'static>(&mut self, items: Vec<T>) -> fmSlice {
        let view = fmSlice {
            data: items.as_ptr().cast(),
            count: items.len() as u64,
            stride: size_of::<T>() as u64,
        };
        self.Values.push(Box::new(items));
        view
    }
    pub fn account(&mut self, a: &Account) -> fmAccount {
        fmAccount {
            displayName: self.text(&a.display_name),
            avatarUrl: self.optional(&a.avatar_url),
            ..fmAccount::new()
        }
    }
    pub fn track(&mut self, t: &Track) -> fmTrack {
        let artists = t.artists.iter().map(|s| self.text(s)).collect();
        fmTrack {
            id: self.text(&t.id),
            providerId: self.text(&t.provider_id),
            mid: self.optional(&t.mid),
            title: self.text(&t.title),
            artists: self.array(artists),
            album: self.text(&t.album),
            hasDuration: t.duration_ms.is_some() as u32,
            durationMs: t.duration_ms.unwrap_or_default(),
            coverUrl: self.optional(&t.cover_url),
            thumbnailUrl: self.optional(&t.thumbnail_url),
            ..fmTrack::new()
        }
    }
    fn playlist(&mut self, p: &Playlist) -> fmPlaylist {
        fmPlaylist {
            id: self.text(&p.id),
            title: self.text(&p.title),
            hasTrackCount: p.track_count.is_some() as u32,
            trackCount: p.track_count.unwrap_or_default(),
            coverUrl: self.optional(&p.cover_url),
            subtitle: self.optional(&p.subtitle),
            ..fmPlaylist::new()
        }
    }
    fn error(&mut self, e: &ApiError) -> fmError {
        fmError {
            code: self.text(&e.code),
            message: self.text(&e.message),
            ..fmError::new()
        }
    }
    fn tracks(&mut self, tracks: &[Track]) -> fmSlice {
        let items = tracks.iter().map(|t| self.track(t)).collect();
        self.array(items)
    }
    fn playlists(&mut self, playlists: &[Playlist]) -> fmSlice {
        let items = playlists.iter().map(|p| self.playlist(p)).collect();
        self.array(items)
    }
}
#[allow(dead_code)]
pub struct OwnedResult {
    pub View: fmResultView,
    Arena: Arena,
}
impl OwnedResult {
    pub fn new(result: Result<ResultData, ApiError>) -> Self {
        let mut arena = Arena::default();
        let (kind, data): (u32, *const c_void) = match result {
            Err(e) => {
                let v = arena.error(&e);
                (0, arena.keep(v).cast())
            }
            Ok(ResultData::Description(info)) => {
                let capabilities = info
                    .capabilities
                    .iter()
                    .chain(&info.extensions)
                    .fold(0, |mask, c| mask | (1 << capabilityIndex(c)));
                let v = fmPluginInfo {
                    id: arena.text(&info.id),
                    name: arena.text(&info.name),
                    version: arena.text(&info.version),
                    capabilities,
                    ..fmPluginInfo::new()
                };
                (1, arena.keep(v).cast())
            }
            Ok(ResultData::Account(account)) => {
                let v = arena.account(&account);
                (2, arena.keep(v).cast())
            }
            Ok(ResultData::Playlists(items)) => {
                let v = arena.playlists(&items);
                (3, arena.keep(v).cast())
            }
            Ok(ResultData::Tracks(page)) => {
                let v = fmTrackPage {
                    items: arena.tracks(&page.items),
                    page: page.page,
                    hasMore: page.has_more as u32,
                    cursor: arena.optional(&page.cursor),
                    ..fmTrackPage::new()
                };
                (4, arena.keep(v).cast())
            }
            Ok(ResultData::RecommendationFeed(feed)) => {
                let issues = feed
                    .issues
                    .iter()
                    .map(|i| fmIssue {
                        stage: arena.text(&i.stage),
                        error: arena.error(&i.error),
                        ..fmIssue::new()
                    })
                    .collect();
                let v = fmFeed {
                    playlists: arena.playlists(&feed.playlists),
                    tracks: arena.tracks(&feed.tracks),
                    personalized: feed.personalized as u32,
                    notice: arena.optional(&feed.notice),
                    playlistSource: sourceIndex(feed.playlist_source),
                    trackSource: sourceIndex(feed.track_source),
                    issues: arena.array(issues),
                    ..fmFeed::new()
                };
                (5, arena.keep(v).cast())
            }
            Ok(ResultData::LoginChallenge(c)) => {
                let v = fmChallenge {
                    challengeId: arena.text(&c.challenge_id),
                    qrText: arena.text(&c.qr_text),
                    expiresInSeconds: c.expires_in_seconds,
                    ..fmChallenge::new()
                };
                (6, arena.keep(v).cast())
            }
            Ok(ResultData::LoginState(state)) => {
                let (state, account) = match state {
                    LoginState::Waiting => (0, fmAccount::new()),
                    LoginState::Scanned => (1, fmAccount::new()),
                    LoginState::Authorized(a) => (2, arena.account(&a)),
                    LoginState::Expired => (3, fmAccount::new()),
                };
                (
                    7,
                    arena
                        .keep(fmLoginState {
                            state,
                            account,
                            ..fmLoginState::new()
                        })
                        .cast(),
                )
            }
            Ok(ResultData::Media(source)) => {
                let v = match source {
                    MediaSource::Http { url, headers } => {
                        let headers = headers
                            .iter()
                            .map(|(k, v)| fmHeader {
                                name: arena.text(k),
                                value: arena.text(v),
                                ..fmHeader::new()
                            })
                            .collect();
                        fmMedia {
                            kind: 0,
                            url: arena.text(&url),
                            headers: arena.array(headers),
                            ..fmMedia::new()
                        }
                    }
                    MediaSource::ManagedFile { id } => fmMedia {
                        kind: 1,
                        mediaHandle: id,
                        ..fmMedia::new()
                    },
                    MediaSource::File { .. } => {
                        return Self::new(Err(ApiError::new(
                            "MEDIA",
                            "C ABI 插件必须返回宿主媒体句柄",
                        )));
                    }
                };
                (8, arena.keep(v).cast())
            }
            Ok(ResultData::Ok) => (9, std::ptr::null()),
            Ok(ResultData::Lyrics(lyrics)) => {
                if let Err(error) = lyrics.validate() {
                    return Self::new(Err(error));
                }
                let value = arena.lyrics(&lyrics);
                (10, arena.keep(value).cast())
            }
            Ok(ResultData::TrackQualities(list)) => {
                if let Err(error) = list.validate() {
                    return Self::new(Err(error));
                }
                let value = arena.qualityList(&list);
                (11, arena.keep(value).cast())
            }
        };
        Self {
            View: fmResultView {
                kind,
                data,
                ..fmResultView::new()
            },
            Arena: arena,
        }
    }
    pub fn intoHandle(self) -> fmHandle {
        Box::into_raw(Box::new(self)).cast()
    }
}
#[allow(dead_code)]
pub struct OwnedRequest {
    pub View: fmRequest,
    Arena: Arena,
}
impl OwnedRequest {
    pub fn new(operation: &Operation) -> Self {
        let mut arena = Arena::default();
        let mut view = fmRequest::new();
        view.method = match operation {
            Operation::Describe => 0,
            Operation::LoginBegin => 1,
            Operation::LoginPoll { challenge_id } => {
                view.id = arena.text(challenge_id);
                2
            }
            Operation::Logout => 3,
            Operation::Account => 4,
            Operation::Search { query, page } => {
                view.query = arena.text(query);
                view.page = *page;
                5
            }
            Operation::Playlists => 6,
            Operation::Playlist { id, page, cursor } => {
                view.id = arena.text(id);
                view.page = *page;
                view.cursor = arena.optional(cursor);
                7
            }
            Operation::Recommendations => 8,
            Operation::Homepage => 9,
            Operation::PlaylistRecommendations => 10,
            Operation::Resolve { track } => {
                let track = arena.track(track);
                view.track = arena.keep(track);
                11
            }
            Operation::Lyrics { track } => {
                let track = arena.track(track);
                view.track = arena.keep(track);
                12
            }
            Operation::TrackQualities { track } => {
                if let Some(track) = track {
                    let track = arena.track(track);
                    view.track = arena.keep(track);
                }
                13
            }
            Operation::ResolveWithQuality { track, qualityId } => {
                let track = arena.track(track);
                view.track = arena.keep(track);
                view.id = arena.text(qualityId);
                14
            }
        };
        Self {
            View: view,
            Arena: arena,
        }
    }
}
pub fn capabilityIndex(c: &Capability) -> u32 {
    match c {
        Capability::Login => 0,
        Capability::Account => 1,
        Capability::Search => 2,
        Capability::Playlists => 3,
        Capability::Playlist => 4,
        Capability::Recommendations => 5,
        Capability::Homepage => 6,
        Capability::PlaylistRecommendations => 7,
        Capability::Resolve => 8,
        Capability::Lyrics => 9,
        Capability::TrackQualities => 10,
    }
}
pub fn sourceIndex(s: RecommendationSource) -> u32 {
    match s {
        RecommendationSource::Unknown => 0,
        RecommendationSource::Public => 1,
        RecommendationSource::Account => 2,
        RecommendationSource::Homepage => 3,
    }
}
