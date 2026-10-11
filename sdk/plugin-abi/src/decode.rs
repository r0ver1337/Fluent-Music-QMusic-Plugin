use crate::types::*;
use fluent_plugin_api::*;

std::thread_local! { static copyBudget: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) }; }
struct Budget(Option<u64>);
impl Budget {
    fn start() -> Self {
        Self(copyBudget.replace(Some(MAX_MESSAGE_BYTES as u64)))
    }
}
impl Drop for Budget {
    fn drop(&mut self) {
        copyBudget.set(self.0);
    }
}
pub(crate) fn consume(length: u64) -> Result<(), ApiError> {
    if let Some(remaining) = copyBudget.get() {
        copyBudget.set(Some(remaining.checked_sub(length).ok_or_else(invalid)?));
    }
    Ok(())
}

pub(crate) fn invalid() -> ApiError {
    ApiError::new("ABI", "插件返回的 C ABI 数据无效")
}
/// # Safety
/// The view must reference readable bytes in this process until copying completes.
pub unsafe fn text(v: fmStringView) -> Result<String, ApiError> {
    consume(v.length)?;
    if v.length > MAX_MESSAGE_BYTES as u64 || (v.length > 0 && v.data.is_null()) {
        return Err(invalid());
    }
    if v.length == 0 {
        return Ok(String::new());
    }
    let bytes = unsafe { std::slice::from_raw_parts(v.data, v.length as usize) };
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| invalid())
}
/// # Safety
/// Each strided record must contain a readable, initialized `T` in this process.
pub unsafe fn array<T: Copy>(v: fmSlice) -> Result<Vec<T>, ApiError> {
    consume(v.count.saturating_mul(size_of::<T>() as u64))?;
    if v.count > maxElements
        || v.count.saturating_mul(v.stride) > MAX_MESSAGE_BYTES as u64
        || (v.count > 0
            && (v.stride < size_of::<T>() as u64
                || !v.stride.is_multiple_of(align_of::<T>() as u64)))
        || (v.count > 0 && (v.data.is_null() || !(v.data as usize).is_multiple_of(align_of::<T>())))
    {
        return Err(invalid());
    }
    if v.count == 0 {
        return Ok(vec![]);
    }
    Ok((0..v.count)
        .map(|i| unsafe {
            v.data
                .cast::<u8>()
                .add((i * v.stride) as usize)
                .cast::<T>()
                .read()
        })
        .collect())
}
pub(crate) unsafe fn body<T: Copy>(p: *const std::ffi::c_void) -> Result<T, ApiError> {
    if p.is_null() || !(p as usize).is_multiple_of(align_of::<T>()) {
        return Err(invalid());
    }
    let size = unsafe { p.cast::<u32>().read() };
    if size < size_of::<T>() as u32 {
        return Err(invalid());
    }
    Ok(unsafe { p.cast::<T>().read() })
}
unsafe fn optional(v: fmStringView) -> Result<Option<String>, ApiError> {
    let s = unsafe { text(v)? };
    Ok((!s.is_empty()).then_some(s))
}
unsafe fn account(v: fmAccount) -> Result<Account, ApiError> {
    Ok(Account {
        display_name: unsafe { text(v.displayName)? },
        avatar_url: unsafe { optional(v.avatarUrl)? },
    })
}
unsafe fn track(v: fmTrack) -> Result<Track, ApiError> {
    let artists = unsafe { array::<fmStringView>(v.artists)? }
        .into_iter()
        .map(|s| unsafe { text(s) })
        .collect::<Result<_, _>>()?;
    Ok(Track {
        id: unsafe { text(v.id)? },
        provider_id: unsafe { text(v.providerId)? },
        mid: unsafe { optional(v.mid)? },
        title: unsafe { text(v.title)? },
        artists,
        album: unsafe { text(v.album)? },
        duration_ms: (v.hasDuration != 0).then_some(v.durationMs),
        cover_url: unsafe { optional(v.coverUrl)? },
        thumbnail_url: unsafe { optional(v.thumbnailUrl)? },
    })
}
unsafe fn playlist(v: fmPlaylist) -> Result<Playlist, ApiError> {
    Ok(Playlist {
        id: unsafe { text(v.id)? },
        title: unsafe { text(v.title)? },
        track_count: (v.hasTrackCount != 0).then_some(v.trackCount),
        cover_url: unsafe { optional(v.coverUrl)? },
        subtitle: unsafe { optional(v.subtitle)? },
    })
}
unsafe fn tracks(v: fmSlice) -> Result<Vec<Track>, ApiError> {
    unsafe { array::<fmTrack>(v)? }
        .into_iter()
        .map(|v| unsafe { track(v) })
        .collect()
}
unsafe fn playlists(v: fmSlice) -> Result<Vec<Playlist>, ApiError> {
    unsafe { array::<fmPlaylist>(v)? }
        .into_iter()
        .map(|v| unsafe { playlist(v) })
        .collect()
}
unsafe fn error(v: fmError) -> Result<ApiError, ApiError> {
    Ok(ApiError {
        code: unsafe { text(v.code)? },
        message: unsafe { text(v.message)? },
    })
}
fn source(v: u32) -> Result<RecommendationSource, ApiError> {
    Ok(match v {
        0 => RecommendationSource::Unknown,
        1 => RecommendationSource::Public,
        2 => RecommendationSource::Account,
        3 => RecommendationSource::Homepage,
        _ => return Err(invalid()),
    })
}

/// # Safety
/// The request and all referenced views must remain readable for this call.
pub unsafe fn decodeRequest(p: *const fmRequest) -> Result<Operation, ApiError> {
    let _budget = Budget::start();
    let r: fmRequest = unsafe { body(p.cast())? };
    Ok(match r.method {
        0 => Operation::Describe,
        1 => Operation::LoginBegin,
        2 => Operation::LoginPoll {
            challenge_id: unsafe { text(r.id)? },
        },
        3 => Operation::Logout,
        4 => Operation::Account,
        5 => Operation::Search {
            query: unsafe { text(r.query)? },
            page: r.page,
        },
        6 => Operation::Playlists,
        7 => Operation::Playlist {
            id: unsafe { text(r.id)? },
            page: r.page,
            cursor: unsafe { optional(r.cursor)? },
        },
        8 => Operation::Recommendations,
        9 => Operation::Homepage,
        10 => Operation::PlaylistRecommendations,
        11 => Operation::Resolve {
            track: unsafe { track(body(r.track.cast())?)? },
        },
        12 => Operation::Lyrics {
            track: unsafe { track(body(r.track.cast())?)? },
        },
        13 => Operation::TrackQualities {
            track: if r.track.is_null() {
                None
            } else {
                Some(unsafe { track(body(r.track.cast())?)? })
            },
        },
        14 => Operation::ResolveWithQuality {
            track: unsafe { track(body(r.track.cast())?)? },
            qualityId: unsafe { text(r.id)? },
        },
        _ => return Err(invalid()),
    })
}
/// # Safety
/// The result and its complete object graph must remain readable for this call.
/// Decode untrusted native pointers only inside the isolated worker.
pub unsafe fn decodeResult(p: *const fmResultView) -> Result<ResultData, ApiError> {
    let _budget = Budget::start();
    let r: fmResultView = unsafe { body(p.cast())? };
    Ok(match r.kind {
        0 => return Err(unsafe { error(body(r.data)?)? }),
        1 => {
            let v: fmPluginInfo = unsafe { body(r.data)? };
            let all = [
                Capability::Login,
                Capability::Account,
                Capability::Search,
                Capability::Playlists,
                Capability::Playlist,
                Capability::Recommendations,
                Capability::Homepage,
                Capability::PlaylistRecommendations,
                Capability::Resolve,
                Capability::Lyrics,
                Capability::TrackQualities,
            ];
            let capabilities = all
                .into_iter()
                .enumerate()
                .filter(|(i, _)| v.capabilities & (1 << i) != 0)
                .map(|(_, c)| c)
                .collect();
            ResultData::Description(PluginInfo {
                id: unsafe { text(v.id)? },
                name: unsafe { text(v.name)? },
                version: unsafe { text(v.version)? },
                api_version: API_VERSION,
                capabilities,
                extensions: vec![],
            })
        }
        2 => ResultData::Account(unsafe { account(body(r.data)?)? }),
        3 => {
            if r.data.is_null() {
                return Err(invalid());
            }
            let slice = unsafe { r.data.cast::<fmSlice>().read() };
            ResultData::Playlists(unsafe { playlists(slice)? })
        }
        4 => {
            let v: fmTrackPage = unsafe { body(r.data)? };
            ResultData::Tracks(Page {
                items: unsafe { tracks(v.items)? },
                page: v.page,
                has_more: v.hasMore != 0,
                cursor: unsafe { optional(v.cursor)? },
            })
        }
        5 => {
            let v: fmFeed = unsafe { body(r.data)? };
            let issues = unsafe { array::<fmIssue>(v.issues)? }
                .into_iter()
                .map(|i| {
                    Ok(RecommendationIssue {
                        stage: unsafe { text(i.stage)? },
                        error: unsafe { error(i.error)? },
                    })
                })
                .collect::<Result<_, ApiError>>()?;
            ResultData::RecommendationFeed(RecommendationFeed {
                playlists: unsafe { playlists(v.playlists)? },
                tracks: unsafe { tracks(v.tracks)? },
                personalized: v.personalized != 0,
                notice: unsafe { optional(v.notice)? },
                playlist_source: source(v.playlistSource)?,
                track_source: source(v.trackSource)?,
                issues,
            })
        }
        6 => {
            let v: fmChallenge = unsafe { body(r.data)? };
            ResultData::LoginChallenge(LoginChallenge {
                challenge_id: unsafe { text(v.challengeId)? },
                qr_text: unsafe { text(v.qrText)? },
                expires_in_seconds: v.expiresInSeconds,
            })
        }
        7 => {
            let v: fmLoginState = unsafe { body(r.data)? };
            ResultData::LoginState(match v.state {
                0 => LoginState::Waiting,
                1 => LoginState::Scanned,
                2 => LoginState::Authorized(unsafe { account(v.account)? }),
                3 => LoginState::Expired,
                _ => return Err(invalid()),
            })
        }
        8 => {
            let v: fmMedia = unsafe { body(r.data)? };
            ResultData::Media(match v.kind {
                0 => {
                    let headers = unsafe { array::<fmHeader>(v.headers)? }
                        .into_iter()
                        .map(|h| Ok((unsafe { text(h.name)? }, unsafe { text(h.value)? })))
                        .collect::<Result<_, ApiError>>()?;
                    MediaSource::Http {
                        url: unsafe { text(v.url)? },
                        headers,
                    }
                }
                1 if v.mediaHandle != 0 => MediaSource::ManagedFile { id: v.mediaHandle },
                _ => return Err(invalid()),
            })
        }
        9 => ResultData::Ok,
        10 => ResultData::Lyrics(unsafe { crate::lyrics::decodeLyrics(r.data)? }),
        11 => ResultData::TrackQualities(unsafe { crate::qualities::decodeQualityList(r.data)? }),
        _ => return Err(invalid()),
    })
}
