#![allow(non_snake_case, non_upper_case_globals)]
use fluent_plugin_abi::*;
use fluent_plugin_abi::{files::*, io::RemoteFile};
use fluent_plugin_api::*;
use std::io::{Read, Write};
mod crypto;
mod network;
mod qualities;

pub fn marker() {}
struct Fixture {
    Name: String,
    Media: Vec<u64>,
}
unsafe extern "C" fn create(
    host: *const fmHostApi,
    info: *const fmCreateInfo,
    out: *mut fmHandle,
) -> fmStatus {
    if out.is_null() || info.is_null() {
        return -1;
    }
    let result = std::panic::catch_unwind(|| -> Result<Fixture, ()> {
        unsafe {
            fluent_plugin_abi::io::installApi(host).map_err(|_| ())?;
        }
        let mut outside = String::new();
        for h in unsafe { decode::array::<fmHeader>((*info).options) }.map_err(|_| ())? {
            if unsafe { decode::text(h.name) }.map_err(|_| ())? == "outsidePath" {
                outside = unsafe { decode::text(h.value) }.map_err(|_| ())?;
            }
        }
        let allowed = std::fs::read(&outside).is_ok();
        crypto::verify()?;
        fluent_plugin_abi::io::writeAtomic("atomic.dat", b"one").map_err(|_| ())?;
        fluent_plugin_abi::io::writeAtomic("atomic.dat", b"two").map_err(|_| ())?;
        if fluent_plugin_abi::io::readFile(rootData, "atomic.dat", 16).map_err(|_| ())? != b"two" {
            return Err(());
        }
        let plugin = fluent_plugin_abi::io::root(rootPlugin).map_err(|_| ())?;
        if RemoteFile::open(plugin, "../outside.txt", openRead).is_ok() {
            return Err(());
        }
        if RemoteFile::open(plugin, "bad.txt", openWrite | openCreate).is_ok() {
            return Err(());
        }
        if RemoteFile::open(plugin, &outside, openRead).is_ok() {
            return Err(());
        }
        if std::fs::read("plugin.txt").is_err() {
            return Err(());
        }
        for rootKind in [rootData, rootTemp] {
            let root = fluent_plugin_abi::io::root(rootKind).map_err(|_| ())?;
            if rootKind == rootTemp
                && RemoteFile::open(root, ".FMMEDIA-1-1/output.audio", openRead)
                    .err()
                    .is_none_or(|e| e.kind() != std::io::ErrorKind::PermissionDenied)
            {
                return Err(());
            }
            let mut file = RemoteFile::open(
                root,
                "probe.txt",
                openRead | openWrite | openCreate | openTruncate,
            )
            .map_err(|_| ())?;
            file.write_all(b"private data").map_err(|_| ())?;
            drop(file);
            let mut bytes = vec![];
            RemoteFile::open(root, "probe.txt", openRead)
                .map_err(|_| ())?
                .read_to_end(&mut bytes)
                .map_err(|_| ())?;
            if bytes != b"private data" {
                return Err(());
            }
        }
        Ok(Fixture {
            Name: if allowed {
                "raw-file-access-allowed"
            } else {
                "raw-file-access-denied"
            }
            .into(),
            Media: vec![],
        })
    });
    match result {
        Ok(Ok(f)) => {
            unsafe {
                *out = Box::into_raw(Box::new(f)).cast();
            }
            0
        }
        _ => -1,
    }
}
unsafe extern "C" fn destroy(h: fmHandle) -> fmStatus {
    if h.is_null() {
        return -1;
    }
    let f = unsafe { Box::from_raw(h.cast::<Fixture>()) };
    for h in f.Media {
        fluent_plugin_abi::io::releaseMedia(h);
    }
    0
}
fn track() -> Track {
    Track {
        id: "fixture".into(),
        title: "Synthetic WAV".into(),
        artists: vec![],
        album: String::new(),
        duration_ms: Some(1000),
        provider_id: "sandbox-test".into(),
        mid: None,
        cover_url: None,
        thumbnail_url: None,
    }
}
unsafe extern "C" fn call(
    h: fmHandle,
    _: *const fmCallContext,
    r: *const fmRequest,
    out: *mut fmHandle,
) -> fmStatus {
    if h.is_null() || out.is_null() {
        return -1;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<ResultData, ApiError> {
            let f = unsafe { &mut *h.cast::<Fixture>() };
            match unsafe { decodeRequest(r)? } {
                Operation::Describe => Ok(ResultData::Description(PluginInfo {
                    id: "sandbox-test".into(),
                    name: f.Name.clone(),
                    version: "1".into(),
                    api_version: API_VERSION,
                    capabilities: vec![
                        Capability::Recommendations,
                        Capability::Resolve,
                        Capability::Search,
                    ],
                    extensions: vec![Capability::Lyrics, Capability::TrackQualities],
                })),
                Operation::Recommendations => Ok(ResultData::Tracks(Page {
                    items: vec![track()],
                    page: 0,
                    has_more: false,
                    cursor: None,
                })),
                Operation::Logout => {
                    for h in f.Media.drain(..) {
                        fluent_plugin_abi::io::releaseMedia(h);
                    }
                    Ok(ResultData::Ok)
                }
                Operation::Search { query, .. } => network::probe(&query),
                Operation::TrackQualities { track } => {
                    Ok(ResultData::TrackQualities(qualities::list(track.is_some())))
                }
                Operation::ResolveWithQuality { qualityId, .. } if qualityId != "standard" => Err(
                    ApiError::new("QUALITY_UNAVAILABLE", "当前账号未获此音质授权"),
                ),
                Operation::Resolve { .. } | Operation::ResolveWithQuality { .. } => {
                    let mut file = RemoteFile::mediaOutput()
                        .map_err(|_| ApiError::new("FILE", "output failed"))?;
                    let mut bytes = Vec::new();
                    bytes.extend_from_slice(b"RIFF");
                    bytes.extend_from_slice(&16036u32.to_le_bytes());
                    bytes.extend_from_slice(b"WAVEfmt ");
                    bytes.extend_from_slice(&16u32.to_le_bytes());
                    bytes.extend_from_slice(&1u16.to_le_bytes());
                    bytes.extend_from_slice(&1u16.to_le_bytes());
                    bytes.extend_from_slice(&8000u32.to_le_bytes());
                    bytes.extend_from_slice(&16000u32.to_le_bytes());
                    bytes.extend_from_slice(&2u16.to_le_bytes());
                    bytes.extend_from_slice(&16u16.to_le_bytes());
                    bytes.extend_from_slice(b"data");
                    bytes.extend_from_slice(&16000u32.to_le_bytes());
                    bytes.resize(16044, 0);
                    file.write_all(&bytes)
                        .map_err(|_| ApiError::new("FILE", "write failed"))?;
                    let id = file
                        .finishMedia()
                        .map_err(|_| ApiError::new("FILE", "finish failed"))?;
                    Ok(ResultData::Media(MediaSource::ManagedFile { id }))
                }
                Operation::Lyrics { .. } => Ok(ResultData::Lyrics(Lyrics {
                    StartMs: Some(100),
                    Original: Some(LyricTimeline {
                        OffsetMs: -50,
                        Lines: vec![LyricLine {
                            StartMs: Some(1000),
                            DurationMs: Some(1000),
                            Text: "你好🎵".into(),
                            Words: vec![
                                LyricWord {
                                    StartMs: 1000,
                                    DurationMs: 500,
                                    Text: "你好".into(),
                                },
                                LyricWord {
                                    StartMs: 1500,
                                    DurationMs: 500,
                                    Text: "🎵".into(),
                                },
                            ],
                        }],
                    }),
                    Translation: Some(LyricTimeline {
                        OffsetMs: 0,
                        Lines: vec![LyricLine {
                            StartMs: Some(1100),
                            DurationMs: None,
                            Text: "Hello".into(),
                            Words: vec![],
                        }],
                    }),
                })),
                _ => Err(ApiError::new("UNSUPPORTED", "offline fixture")),
            }
        },
    ))
    .unwrap_or_else(|_| Err(ApiError::new("ABI", "fixture panic")));
    unsafe {
        *out = OwnedResult::new(result).intoHandle();
    }
    0
}
unsafe extern "C" fn get(h: fmHandle) -> *const fmResultView {
    unsafe { &(*h.cast::<OwnedResult>()).View }
}
unsafe extern "C" fn release(h: fmHandle) {
    if !h.is_null() {
        unsafe {
            let view = &(*h.cast::<OwnedResult>()).View;
            if view.kind == 8 {
                let media = &*view.data.cast::<fmMedia>();
                fluent_plugin_abi::io::releaseMedia(media.mediaHandle);
            }
            drop(Box::from_raw(h.cast::<OwnedResult>()));
        }
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
    logout: Some(call),
    getAccount: None,
    search: Some(call),
    listPlaylists: None,
    getPlaylist: None,
    getRecommendations: Some(call),
    getHomepage: None,
    getPlaylistRecommendations: None,
    resolveTrack: Some(call),
    getResultView: get,
    releaseResult: release,
    getLyrics: Some(call),
    getTrackQualities: Some(call),
    resolveTrackWithQuality: Some(call),
};
#[unsafe(no_mangle)]
/// # Safety
/// `out` must point to writable caller-owned pointer storage.
pub unsafe extern "C" fn fmPluginGetApi(v: u32, out: *mut *const fmPluginApi) -> fmStatus {
    if out.is_null() || v != abiVersion {
        return -1;
    }
    unsafe {
        *out = &api;
    }
    0
}
