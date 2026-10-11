use super::*;
use fluent_plugin_api::*;
use std::mem::offset_of;

unsafe extern "C" {
    fn fmCQualityLevelSize() -> usize;
    fn fmCTrackQualitySize() -> usize;
    fn fmCQualityListSize() -> usize;
    fn fmCGetTrackQualitiesOffset() -> usize;
    fn fmCResolveTrackWithQualityOffset() -> usize;
    fn fmCTrackQualitySizeOffset() -> usize;
}

fn list() -> QualityList {
    QualityList {
        Levels: vec![
            QualityLevel {
                Priority: 1,
                Name: "无损".into(),
                Disabled: false,
            },
            QualityLevel {
                Priority: 2,
                Name: "标准".into(),
                Disabled: false,
            },
        ],
        Items: vec![
            TrackQuality {
                Id: "sq".into(),
                Name: "无损".into(),
                Priority: 1,
                FileSizeBytes: Some(1u64 << 33),
                Playable: false,
                UnavailableReason: Some("当前账号未获授权".into()),
            },
            TrackQuality {
                Id: "standard".into(),
                Name: "标准".into(),
                Priority: 2,
                FileSizeBytes: None,
                Playable: true,
                UnavailableReason: None,
            },
        ],
    }
}

#[test]
fn qualityCLayoutAndAppendOffsetsMatchRust() {
    unsafe {
        assert_eq!(fmCQualityLevelSize(), size_of::<fmQualityLevel>());
        assert_eq!(fmCTrackQualitySize(), size_of::<fmTrackQuality>());
        assert_eq!(fmCQualityListSize(), size_of::<fmQualityList>());
        assert_eq!(
            fmCGetTrackQualitiesOffset(),
            offset_of!(fmPluginApi, getTrackQualities)
        );
        assert_eq!(
            fmCResolveTrackWithQualityOffset(),
            offset_of!(fmPluginApi, resolveTrackWithQuality)
        );
        assert_eq!(
            fmCTrackQualitySizeOffset(),
            offset_of!(fmTrackQuality, fileSizeBytes)
        );
    }
    assert_eq!(
        offset_of!(fmPluginApi, getTrackQualities),
        offset_of!(fmPluginApi, getLyrics) + size_of::<fmCall>()
    );
}

#[test]
fn resultGraphIsCopiedBeforePluginRelease() {
    let expected = list();
    let owned = OwnedResult::new(Ok(ResultData::TrackQualities(expected.clone())));
    assert_eq!(owned.View.kind, 11);
    let ResultData::TrackQualities(copied) = unsafe { decodeResult(&owned.View) }.unwrap() else {
        panic!("qualities")
    };
    drop(owned);
    assert_eq!(copied, expected);
    let json = serde_json::to_vec(&ResultData::TrackQualities(copied)).unwrap();
    let ResultData::TrackQualities(copied) = serde_json::from_slice(&json).unwrap() else {
        panic!("wire")
    };
    assert_eq!(copied, expected);
}

#[test]
fn catalogueAndExactQualityRequestsUseUnchangedRequestLayout() {
    let owned = OwnedRequest::new(&Operation::TrackQualities { track: None });
    assert_eq!(owned.View.method, 13);
    assert!(owned.View.track.is_null());
    assert!(matches!(
        unsafe { decodeRequest(&owned.View) }.unwrap(),
        Operation::TrackQualities { track: None }
    ));
    let track = Track {
        id: "123".into(),
        title: "曲目".into(),
        artists: vec![],
        album: "".into(),
        duration_ms: None,
        provider_id: "test".into(),
        mid: None,
        cover_url: None,
        thumbnail_url: None,
    };
    let operation = Operation::ResolveWithQuality {
        track: track.clone(),
        qualityId: "sq".into(),
    };
    let owned = OwnedRequest::new(&operation);
    assert_eq!(owned.View.method, 14);
    let Operation::ResolveWithQuality {
        track: copied,
        qualityId,
    } = unsafe { decodeRequest(&owned.View) }.unwrap()
    else {
        panic!("request")
    };
    assert_eq!(copied, track);
    assert_eq!(qualityId, "sq");
    assert_eq!(operation.capability(), Some(Capability::TrackQualities));
    assert!(operation.accepts(&ResultData::Media(MediaSource::ManagedFile { id: 1 })));
}

#[test]
fn invalidFlagsSizesStridesAndRecordsAreRejected() {
    let mut arena = arena::Arena::default();
    let valid = fmTrackQuality {
        id: arena.text("sq"),
        name: arena.text("无损"),
        priority: 1,
        ..fmTrackQuality::new()
    };
    for item in [
        fmTrackQuality {
            hasFileSize: 2,
            ..valid
        },
        fmTrackQuality {
            playable: 2,
            ..valid
        },
        fmTrackQuality {
            fileSizeBytes: 123,
            ..valid
        },
        fmTrackQuality {
            hasFileSize: 1,
            fileSizeBytes: 0,
            ..valid
        },
        fmTrackQuality {
            structSize: 4,
            ..valid
        },
    ] {
        let name = arena.text("无损");
        let levels = arena.array(vec![fmQualityLevel {
            priority: 1,
            name,
            ..fmQualityLevel::new()
        }]);
        let value = fmQualityList {
            levels,
            items: arena.array(vec![item]),
            ..fmQualityList::new()
        };
        let view = fmResultView {
            kind: 11,
            data: (&value as *const fmQualityList).cast(),
            ..fmResultView::new()
        };
        assert!(unsafe { decodeResult(&view) }.is_err());
    }
    let owner = OwnedResult::new(Ok(ResultData::TrackQualities(list())));
    let mut value = unsafe { *owner.View.data.cast::<fmQualityList>() };
    value.items.stride = 1;
    let view = fmResultView {
        data: (&value as *const fmQualityList).cast(),
        ..owner.View
    };
    assert!(unsafe { decodeResult(&view) }.is_err());
    value.items.count = maxQualityEntries as u64 + 1;
    let view = fmResultView {
        data: (&value as *const fmQualityList).cast(),
        ..owner.View
    };
    assert!(unsafe { decodeResult(&view) }.is_err());
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

#[test]
fn exactlyAllocatedOldAndPartialTablesDoNotReadAppendedFields() {
    let mut api = fmPluginApi {
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
        getTrackQualities: Some(call),
        resolveTrackWithQuality: Some(call),
    };
    for size in [
        offset_of!(fmPluginApi, getLyrics),
        offset_of!(fmPluginApi, getTrackQualities),
        offset_of!(fmPluginApi, resolveTrackWithQuality),
    ] {
        api.structSize = size as u32;
        let mut prefix = vec![0u64; size / 8];
        unsafe {
            std::ptr::copy_nonoverlapping(
                (&api as *const fmPluginApi).cast::<u8>(),
                prefix.as_mut_ptr().cast(),
                size,
            );
        }
        let loaded = unsafe { readPluginApi(prefix.as_ptr().cast()) }.unwrap();
        assert_eq!(
            loaded.getLyrics.is_some(),
            size > offset_of!(fmPluginApi, getLyrics)
        );
        assert_eq!(
            loaded.getTrackQualities.is_some(),
            size > offset_of!(fmPluginApi, getTrackQualities)
        );
        assert!(loaded.resolveTrackWithQuality.is_none());
    }
    api.structSize = size_of::<fmPluginApi>() as u32;
    assert!(unsafe { readPluginApi(&api) }.unwrap().method(14).is_some());
}
