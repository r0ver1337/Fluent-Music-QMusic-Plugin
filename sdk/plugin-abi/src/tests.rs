#![allow(non_snake_case)]
use super::*;
use fluent_plugin_api::*;
unsafe extern "C" {
    fn fmCTrackSize() -> usize;
    fn fmCRequestSize() -> usize;
    fn fmCPluginApiSize() -> usize;
    fn fmCHostApiSize() -> usize;
    fn fmCNetworkApiSize() -> usize;
    fn fmCNetworkApiOffset() -> usize;
    fn fmCAbiVersion() -> u32;
    fn fmCFileRequestSize() -> usize;
    fn fmCFileReplySize() -> usize;
    fn fmCTrackDurationOffset() -> usize;
}
#[test]
fn publicCHeaderMatchesRustLayout() {
    unsafe {
        assert_eq!(fmCTrackSize(), size_of::<fmTrack>());
        assert_eq!(fmCRequestSize(), size_of::<fmRequest>());
        assert_eq!(fmCPluginApiSize(), size_of::<fmPluginApi>());
        assert_eq!(fmCHostApiSize(), size_of::<fmHostApi>());
        assert_eq!(fmCNetworkApiSize(), size_of::<network::fmNetworkApi>());
        assert_eq!(
            fmCNetworkApiOffset(),
            std::mem::offset_of!(fmHostApi, networkApi)
        );
        assert_eq!(fmCAbiVersion(), abiVersion);
        assert_eq!(fmCFileRequestSize(), size_of::<files::fmFileRequest>());
        assert_eq!(fmCFileReplySize(), size_of::<files::fmFileReply>());
        assert_eq!(
            fmCTrackDurationOffset(),
            std::mem::offset_of!(fmTrack, durationMs)
        );
    }
}
#[test]
fn typedRequestsAndResultsRoundtrip() {
    let track = Track {
        id: "123".into(),
        provider_id: "test".into(),
        title: "歌曲".into(),
        artists: vec!["艺人".into()],
        album: "专辑".into(),
        mid: None,
        duration_ms: None,
        cover_url: None,
        thumbnail_url: None,
    };
    let input = OwnedRequest::new(&Operation::Resolve {
        track: track.clone(),
    });
    let Operation::Resolve { track: returned } = unsafe { decodeRequest(&input.View) }.unwrap()
    else {
        panic!("resolve");
    };
    assert_eq!(returned, track);
    let result = OwnedResult::new(Ok(ResultData::Tracks(Page {
        items: vec![track.clone()],
        page: 0,
        has_more: false,
        cursor: None,
    })));
    let ResultData::Tracks(page) = unsafe { decodeResult(&result.View) }.unwrap() else {
        panic!("tracks");
    };
    assert_eq!(page.items[0], track);
    let result = OwnedResult::new(Err(ApiError::new("TEST", "safe error")));
    assert_eq!(
        unsafe { decodeResult(&result.View) }.err().unwrap().code,
        "TEST"
    );
}
