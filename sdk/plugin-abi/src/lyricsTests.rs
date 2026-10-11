use super::*;
use fluent_plugin_api::*;
use std::mem::offset_of;

unsafe extern "C" {
    fn fmCLyricWordSize() -> usize;
    fn fmCLyricLineSize() -> usize;
    fn fmCLyricTimelineSize() -> usize;
    fn fmCLyricsSize() -> usize;
    fn fmCGetLyricsOffset() -> usize;
    fn fmCLyricLineWordsOffset() -> usize;
}

fn sample() -> Lyrics {
    Lyrics {
        StartMs: Some(200),
        Original: Some(LyricTimeline {
            OffsetMs: -100,
            Lines: vec![LyricLine {
                StartMs: Some(1000),
                DurationMs: Some(500),
                Text: "你好🎵".into(),
                Words: vec![
                    LyricWord {
                        StartMs: 1000,
                        DurationMs: 200,
                        Text: "你好".into(),
                    },
                    LyricWord {
                        StartMs: 1200,
                        DurationMs: 300,
                        Text: "🎵".into(),
                    },
                ],
            }],
        }),
        Translation: Some(LyricTimeline {
            OffsetMs: 0,
            Lines: vec![LyricLine {
                StartMs: Some(1000),
                DurationMs: None,
                Text: "Hello".into(),
                Words: vec![],
            }],
        }),
    }
}

#[test]
fn lyricsCLayoutsMatchRustIncludingAppendedFunctionOffset() {
    unsafe {
        assert_eq!(fmCLyricWordSize(), size_of::<fmLyricWord>());
        assert_eq!(fmCLyricLineSize(), size_of::<fmLyricLine>());
        assert_eq!(fmCLyricTimelineSize(), size_of::<fmLyricTimeline>());
        assert_eq!(fmCLyricsSize(), size_of::<fmLyrics>());
        assert_eq!(fmCGetLyricsOffset(), offset_of!(fmPluginApi, getLyrics));
        assert_eq!(fmCLyricLineWordsOffset(), offset_of!(fmLyricLine, words));
        assert_eq!(
            offset_of!(fmPluginApi, getLyrics),
            offset_of!(fmPluginApi, releaseResult) + size_of::<usize>()
        );
    }
}

#[test]
fn lyricsResultIsCopiedBeforeArenaReleaseAndRoundtripsOverIpc() {
    let expected = sample();
    let owner = OwnedResult::new(Ok(ResultData::Lyrics(expected.clone())));
    assert_eq!(owner.View.kind, 10);
    let result = unsafe { decodeResult(&owner.View) }.unwrap();
    drop(owner);
    let encoded = serde_json::to_vec(&result).unwrap();
    let ResultData::Lyrics(returned) = serde_json::from_slice(&encoded).unwrap() else {
        panic!("lyrics")
    };
    assert_eq!(returned, expected);
    assert_eq!(
        serde_json::to_value(returned).unwrap()["original"]["lines"][0]["words"][0]["startMs"],
        1000
    );
}

#[test]
fn missingAndUntimedLyricsAreNotFakeZeroTimestamps() {
    let mut value = Lyrics::default();
    for plain in [
        None,
        Some(LyricTimeline {
            OffsetMs: 0,
            Lines: vec![LyricLine {
                StartMs: None,
                DurationMs: None,
                Text: "Plain".into(),
                Words: vec![],
            }],
        }),
    ] {
        value.Original = plain;
        let owner = OwnedResult::new(Ok(ResultData::Lyrics(value.clone())));
        let ResultData::Lyrics(returned) = unsafe { decodeResult(&owner.View) }.unwrap() else {
            panic!("lyrics")
        };
        assert_eq!(returned, value);
    }
}

#[test]
fn typedLyricsRequestUsesTrackAndDedicatedCapability() {
    let track = Track {
        id: "42".into(),
        provider_id: "qqmusic".into(),
        title: "T".into(),
        artists: vec![],
        album: String::new(),
        duration_ms: None,
        mid: None,
        cover_url: None,
        thumbnail_url: None,
    };
    let operation = Operation::Lyrics {
        track: track.clone(),
    };
    assert_eq!(operation.capability(), Some(Capability::Lyrics));
    let owner = OwnedRequest::new(&operation);
    assert_eq!(owner.View.method, 12);
    let Operation::Lyrics { track: returned } = unsafe { decodeRequest(&owner.View) }.unwrap()
    else {
        panic!("lyrics")
    };
    assert_eq!(returned, track);
    assert!(operation.accepts(&ResultData::Lyrics(Lyrics::default())));
    assert!(!operation.accepts(&ResultData::Ok));
}

#[test]
fn malformedFlagsPrefixesArraysAndCumulativeCopiesAreRejected() {
    let mut arena = arena::Arena::default();
    let word = fmLyricWord {
        text: arena.text("A"),
        ..fmLyricWord::new()
    };
    let wordArray = arena.array(vec![word]);
    let validLine = fmLyricLine {
        hasStart: 1,
        hasDuration: 1,
        text: arena.text("A"),
        words: wordArray,
        ..fmLyricLine::new()
    };
    let mut invalidLine = validLine;
    invalidLine.hasDuration = 2;
    for line in [
        invalidLine,
        fmLyricLine {
            structSize: 4,
            ..validLine
        },
        fmLyricLine {
            words: fmSlice {
                stride: 1,
                ..wordArray
            },
            ..validLine
        },
    ] {
        let lines = arena.array(vec![line]);
        let timeline = arena.keep(fmLyricTimeline {
            lines,
            ..fmLyricTimeline::new()
        });
        let value = fmLyrics {
            original: timeline,
            ..fmLyrics::new()
        };
        let view = fmResultView {
            kind: 10,
            data: (&value as *const fmLyrics).cast(),
            ..fmResultView::new()
        };
        assert!(unsafe { decodeResult(&view) }.is_err());
    }
    let largeLine = fmLyricLine {
        text: arena.text(&"A".repeat(MAX_MESSAGE_BYTES / 2)),
        ..fmLyricLine::new()
    };
    let lines = arena.array(vec![largeLine]);
    let timeline = arena.keep(fmLyricTimeline {
        lines,
        ..fmLyricTimeline::new()
    });
    let value = fmLyrics {
        original: timeline,
        translation: timeline,
        ..fmLyrics::new()
    };
    let view = fmResultView {
        kind: 10,
        data: (&value as *const fmLyrics).cast(),
        ..fmResultView::new()
    };
    assert_eq!(unsafe { decodeResult(&view) }.err().unwrap().code, "ABI");
}

unsafe extern "C" fn create(
    _: *const fmHostApi,
    _: *const fmCreateInfo,
    _: *mut fmHandle,
) -> fmStatus {
    0
}
unsafe extern "C" fn destroy(_: fmHandle) -> fmStatus {
    0
}
unsafe extern "C" fn call(
    _: fmHandle,
    _: *const fmCallContext,
    _: *const fmRequest,
    _: *mut fmHandle,
) -> fmStatus {
    0
}
unsafe extern "C" fn get(_: fmHandle) -> *const fmResultView {
    std::ptr::null()
}
unsafe extern "C" fn release(_: fmHandle) {}

fn table() -> fmPluginApi {
    fmPluginApi {
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
        getRecommendations: None,
        getHomepage: None,
        getPlaylistRecommendations: None,
        resolveTrack: None,
        getResultView: get,
        releaseResult: release,
        getLyrics: Some(call),
        getTrackQualities: None,
        resolveTrackWithQuality: None,
    }
}

#[test]
fn exactLegacyTableAllocationIsReadWithoutTouchingTheNewTail() {
    let mut full = table();
    let prefixSize = offset_of!(fmPluginApi, getLyrics);
    full.structSize = prefixSize as u32;
    let mut old = vec![0u64; prefixSize / 8];
    unsafe {
        std::ptr::copy_nonoverlapping(
            (&full as *const fmPluginApi).cast::<u8>(),
            old.as_mut_ptr().cast(),
            prefixSize,
        );
    }
    let loaded = unsafe { readPluginApi(old.as_ptr().cast()) }.unwrap();
    assert!(loaded.method(0).is_some());
    assert!(loaded.method(12).is_none());
    assert!(
        unsafe { readPluginApi(&table()) }
            .unwrap()
            .method(12)
            .is_some()
    );
    full.structSize = prefixSize as u32 - 1;
    assert!(unsafe { readPluginApi(&full) }.is_err());
}

#[test]
fn optionalLyricsAndMissingRequiredPointersAreHandledWithoutInvalidRustFunctions() {
    let mut full = table();
    full.getLyrics = None;
    assert!(
        unsafe { readPluginApi(&full) }
            .unwrap()
            .method(12)
            .is_none()
    );
    full.abiVersion = 1;
    assert!(unsafe { readPluginApi(&full) }.is_err());
    full.abiVersion = abiVersion;
    for offset in [
        offset_of!(fmPluginApi, createProvider),
        offset_of!(fmPluginApi, describe),
        offset_of!(fmPluginApi, getResultView),
        offset_of!(fmPluginApi, releaseResult),
    ] {
        let mut bytes = vec![0u64; size_of::<fmPluginApi>() / 8];
        unsafe {
            std::ptr::copy_nonoverlapping(
                (&full as *const fmPluginApi).cast::<u8>(),
                bytes.as_mut_ptr().cast(),
                size_of::<fmPluginApi>(),
            );
        }
        bytes[offset / 8] = 0;
        assert!(unsafe { readPluginApi(bytes.as_ptr().cast()) }.is_err());
    }
}
