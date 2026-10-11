#![allow(non_snake_case, non_upper_case_globals)]
use fluent_plugin_abi::{io::RemoteFile, *};
use fluent_plugin_api::*;
use std::io::Write;

struct Provider {
    Requests: u32,
}

unsafe extern "C" fn create(
    host: *const fmHostApi,
    _: *const fmCreateInfo,
    out: *mut fmHandle,
) -> fmStatus {
    if out.is_null() {
        return -1;
    }
    if unsafe { fluent_plugin_abi::io::installApi(host) }.is_err() {
        return -1;
    }
    unsafe {
        *out = Box::into_raw(Box::new(Provider { Requests: 0 })).cast();
    }
    0
}

unsafe extern "C" fn destroy(handle: fmHandle) -> fmStatus {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<Provider>()) });
    }
    0
}

fn track(second: bool) -> Track {
    Track {
        id: if second { "second" } else { "first" }.into(),
        title: if second {
            "句级歌词验证"
        } else {
            "真实音频时钟验证"
        }
        .into(),
        artists: vec!["离线验证夹具".into()],
        album: "Synthetic silence".into(),
        duration_ms: Some(30_000),
        provider_id: "desktop-lyrics-test".into(),
        mid: None,
        cover_url: None,
        thumbnail_url: None,
    }
}

fn longTrack() -> Track {
    let mut value = track(false);
    value.id = "long".into();
    value.title = "长句单行滚动验证".into();
    value
}

fn audio() -> Result<ResultData, ApiError> {
    let mut file =
        RemoteFile::mediaOutput().map_err(|error| ApiError::new("FILE", error.to_string()))?;
    let dataBytes = 8000u32 * 2 * 30;
    let mut header = Vec::new();
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&(dataBytes + 36).to_le_bytes());
    header.extend_from_slice(b"WAVEfmt ");
    header.extend_from_slice(&16u32.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&8000u32.to_le_bytes());
    header.extend_from_slice(&16000u32.to_le_bytes());
    header.extend_from_slice(&2u16.to_le_bytes());
    header.extend_from_slice(&16u16.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&dataBytes.to_le_bytes());
    let write = || -> std::io::Result<()> {
        file.write_all(&header)?;
        for _ in 0..dataBytes / 1600 {
            file.write_all(&[0u8; 1600])?;
        }
        Ok(())
    };
    let mut write = write;
    write().map_err(|error| ApiError::new("FILE", error.to_string()))?;
    let id = file
        .finishMedia()
        .map_err(|error| ApiError::new("FILE", error.to_string()))?;
    Ok(ResultData::Media(MediaSource::ManagedFile { id }))
}

fn lyrics(second: bool) -> Lyrics {
    let texts = if second {
        ["第二首使用真实句级歌词", "没有逐字时间就不伪造高亮"]
    } else {
        ["真实播放驱动桌面歌词", "译文始终保持居中显示"]
    };
    let translations = if second {
        [
            "Sentence-timed lyrics, without invented words.",
            "No fabricated character timing.",
        ]
    } else {
        [
            "Lyrics follow real audio playback.",
            "Translations stay centered.",
        ]
    };
    Lyrics {
        Original: Some(LyricTimeline {
            OffsetMs: 0,
            Lines: texts
                .iter()
                .enumerate()
                .map(|(index, text)| {
                    let start = index as i64 * 6000 + if second { 5000 } else { 0 };
                    LyricLine {
                        StartMs: Some(start),
                        DurationMs: Some(5000),
                        Text: (*text).into(),
                        Words: if second {
                            vec![]
                        } else {
                            text.chars()
                                .enumerate()
                                .map(|(index, character)| LyricWord {
                                    StartMs: start + index as i64 * 400,
                                    DurationMs: 400,
                                    Text: character.to_string(),
                                })
                                .collect()
                        },
                    }
                })
                .collect(),
        }),
        Translation: Some(LyricTimeline {
            OffsetMs: 0,
            Lines: translations
                .iter()
                .enumerate()
                .map(|(index, text)| LyricLine {
                    StartMs: Some(index as i64 * 6000 + if second { 5000 } else { 0 }),
                    DurationMs: None,
                    Text: (*text).into(),
                    Words: vec![],
                })
                .collect(),
        }),
        StartMs: None,
    }
}

fn longLyrics() -> Lyrics {
    let mut lyrics = lyrics(false);
    let primary = lyrics.Original.as_mut().unwrap();
    for (index, line) in primary.Lines.iter_mut().enumerate() {
        line.Text = if index == 0 {
            "这是一句需要跟随真实演唱进度向左滚动并且始终保持单行的长歌词"
        } else {
            "改变固定显示宽度不会改变字号或者换行暂停之后滚动位置也必须保持不变"
        }
        .repeat(3);
        let count = line.Text.chars().count() as i64;
        let start = line.StartMs.unwrap();
        line.Words = line
            .Text
            .chars()
            .enumerate()
            .map(|(index, character)| LyricWord {
                StartMs: start + index as i64 * 5000 / count,
                DurationMs: (5000 / count) as u64,
                Text: character.to_string(),
            })
            .collect();
    }
    for line in &mut lyrics.Translation.as_mut().unwrap().Lines {
        line.Text = "This deliberately long translation stays on one line, scrolls with the song, and keeps its position while paused. ".repeat(4);
    }
    lyrics
}

unsafe extern "C" fn call(
    handle: fmHandle,
    _: *const fmCallContext,
    request: *const fmRequest,
    out: *mut fmHandle,
) -> fmStatus {
    if handle.is_null() || out.is_null() {
        return -1;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<ResultData, ApiError> {
            let provider = unsafe { &mut *handle.cast::<Provider>() };
            match unsafe { decodeRequest(request)? } {
                Operation::Describe => Ok(ResultData::Description(PluginInfo {
                    id: "desktop-lyrics-test".into(),
                    name: "桌面歌词离线验证".into(),
                    version: "1".into(),
                    api_version: API_VERSION,
                    capabilities: vec![
                        Capability::Homepage,
                        Capability::Recommendations,
                        Capability::Resolve,
                    ],
                    extensions: vec![Capability::Lyrics],
                })),
                Operation::Homepage => Ok(ResultData::RecommendationFeed(RecommendationFeed {
                    tracks: vec![track(false), track(true), longTrack()],
                    ..Default::default()
                })),
                Operation::Recommendations => Ok(ResultData::Tracks(Page {
                    items: vec![track(false), track(true), longTrack()],
                    page: 0,
                    has_more: false,
                    cursor: None,
                })),
                Operation::Resolve { .. } => audio(),
                Operation::Lyrics { track } => {
                    provider.Requests += 1;
                    fluent_plugin_abi::io::writeAtomic(
                        "lyric-requests.txt",
                        provider.Requests.to_string().as_bytes(),
                    )
                    .map_err(|error| ApiError::new("FILE", error.to_string()))?;
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    Ok(ResultData::Lyrics(if track.id == "long" {
                        longLyrics()
                    } else {
                        lyrics(track.id == "second")
                    }))
                }
                _ => Err(ApiError::new("UNSUPPORTED", "Offline fixture")),
            }
        },
    ))
    .unwrap_or_else(|_| Err(ApiError::new("ABI", "Fixture panic")));
    unsafe {
        *out = OwnedResult::new(result).intoHandle();
    }
    0
}

unsafe extern "C" fn view(handle: fmHandle) -> *const fmResultView {
    unsafe { &(*handle.cast::<OwnedResult>()).View }
}

unsafe extern "C" fn release(handle: fmHandle) {
    if !handle.is_null() {
        let result = unsafe { &(*handle.cast::<OwnedResult>()).View };
        if result.kind == 8 {
            let media = unsafe { &*result.data.cast::<fmMedia>() };
            fluent_plugin_abi::io::releaseMedia(media.mediaHandle);
        }
        drop(unsafe { Box::from_raw(handle.cast::<OwnedResult>()) });
    }
}

static api: fmPluginApi = fmPluginApi {
    structSize: size_of::<fmPluginApi>() as u32,
    abiVersion,
    createProvider: create,
    destroyProvider: destroy,
    describe: Some(call),
    loginBegin: None,
    loginPoll: None,
    logout: None,
    getAccount: None,
    search: None,
    listPlaylists: None,
    getPlaylist: None,
    getRecommendations: Some(call),
    getHomepage: Some(call),
    getPlaylistRecommendations: None,
    resolveTrack: Some(call),
    getResultView: view,
    releaseResult: release,
    getLyrics: Some(call),
    getTrackQualities: None,
    resolveTrackWithQuality: None,
};

#[unsafe(no_mangle)]
/// # Safety
/// `out` must be writable pointer storage owned by the caller.
pub unsafe extern "C" fn fmPluginGetApi(version: u32, out: *mut *const fmPluginApi) -> fmStatus {
    if out.is_null() || version != abiVersion {
        return -1;
    }
    unsafe {
        *out = &api;
    }
    0
}
