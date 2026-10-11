use std::ffi::c_void;

pub const abiVersion: u32 = 2;
pub const maxElements: u64 = 65536;
pub type fmStatus = i32;
pub type fmHandle = *mut c_void;
#[derive(Debug)]
pub struct AbiFault;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct fmStringView {
    pub data: *const u8,
    pub length: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct fmSlice {
    pub data: *const c_void,
    pub count: u64,
    pub stride: u64,
}

macro_rules! view {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[repr(C)]
        #[derive(Clone, Copy, Default)]
        pub struct $name { pub structSize: u32, $(pub $field: $ty),* }
        impl $name { pub fn new() -> Self { Self { structSize: size_of::<Self>() as u32, ..Default::default() } } }
    };
}
view!(fmPluginInfo {
    id: fmStringView,
    name: fmStringView,
    version: fmStringView,
    capabilities: u64
});
view!(fmAccount {
    displayName: fmStringView,
    avatarUrl: fmStringView
});
view!(fmTrack {
    id: fmStringView,
    providerId: fmStringView,
    mid: fmStringView,
    title: fmStringView,
    artists: fmSlice,
    album: fmStringView,
    hasDuration: u32,
    durationMs: u64,
    coverUrl: fmStringView,
    thumbnailUrl: fmStringView
});
view!(fmPlaylist {
    id: fmStringView,
    title: fmStringView,
    hasTrackCount: u32,
    trackCount: u64,
    coverUrl: fmStringView,
    subtitle: fmStringView
});
view!(fmTrackPage {
    items: fmSlice,
    page: u32,
    hasMore: u32,
    cursor: fmStringView
});
view!(fmError {
    code: fmStringView,
    message: fmStringView
});
view!(fmIssue {
    stage: fmStringView,
    error: fmError
});
view!(fmFeed {
    playlists: fmSlice,
    tracks: fmSlice,
    personalized: u32,
    notice: fmStringView,
    playlistSource: u32,
    trackSource: u32,
    issues: fmSlice
});
view!(fmChallenge {
    challengeId: fmStringView,
    qrText: fmStringView,
    expiresInSeconds: u32
});
view!(fmLoginState {
    state: u32,
    account: fmAccount
});
view!(fmHeader {
    name: fmStringView,
    value: fmStringView
});
view!(fmMedia {
    kind: u32,
    url: fmStringView,
    headers: fmSlice,
    mediaHandle: u64
});
view!(fmLyricWord {
    startMs: i64,
    durationMs: u64,
    text: fmStringView
});
view!(fmLyricLine {
    hasStart: u32,
    startMs: i64,
    hasDuration: u32,
    durationMs: u64,
    text: fmStringView,
    words: fmSlice
});
view!(fmLyricTimeline {
    offsetMs: i64,
    lines: fmSlice
});
view!(fmLyrics {
    original: *const fmLyricTimeline, translation: *const fmLyricTimeline,
    hasStart: u32, startMs: i64
});
view!(fmQualityLevel {
    priority: u32,
    name: fmStringView,
    disabled: u32
});
view!(fmTrackQuality {
    id: fmStringView,
    name: fmStringView,
    priority: u32,
    hasFileSize: u32,
    fileSizeBytes: u64,
    playable: u32,
    unavailableReason: fmStringView
});
view!(fmQualityList {
    levels: fmSlice,
    items: fmSlice
});
view!(fmResultView { kind: u32, data: *const c_void });
view!(fmRequest { method: u32, query: fmStringView, id: fmStringView, page: u32, cursor: fmStringView, track: *const fmTrack });
view!(fmCreateInfo {
    pluginId: fmStringView,
    options: fmSlice
});

#[repr(C)]
#[derive(Clone, Copy)]
pub struct fmCallContext {
    pub structSize: u32,
    pub requestId: u64,
    pub timeoutMs: u64,
    pub userData: *mut c_void,
    pub isCancelled: Option<unsafe extern "C" fn(*mut c_void) -> u32>,
}
pub type fmCall = unsafe extern "C" fn(
    fmHandle,
    *const fmCallContext,
    *const fmRequest,
    *mut fmHandle,
) -> fmStatus;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct fmPluginApi {
    pub structSize: u32,
    pub abiVersion: u32,
    pub createProvider:
        unsafe extern "C" fn(*const fmHostApi, *const fmCreateInfo, *mut fmHandle) -> fmStatus,
    pub destroyProvider: unsafe extern "C" fn(fmHandle) -> fmStatus,
    pub describe: Option<fmCall>,
    pub loginBegin: Option<fmCall>,
    pub loginPoll: Option<fmCall>,
    pub logout: Option<fmCall>,
    pub getAccount: Option<fmCall>,
    pub search: Option<fmCall>,
    pub listPlaylists: Option<fmCall>,
    pub getPlaylist: Option<fmCall>,
    pub getRecommendations: Option<fmCall>,
    pub getHomepage: Option<fmCall>,
    pub getPlaylistRecommendations: Option<fmCall>,
    pub resolveTrack: Option<fmCall>,
    pub getResultView: unsafe extern "C" fn(fmHandle) -> *const fmResultView,
    pub releaseResult: unsafe extern "C" fn(fmHandle),
    pub getLyrics: Option<fmCall>,
    pub getTrackQualities: Option<fmCall>,
    pub resolveTrackWithQuality: Option<fmCall>,
}
impl fmPluginApi {
    pub fn method(&self, method: u32) -> Option<fmCall> {
        [
            self.describe,
            self.loginBegin,
            self.loginPoll,
            self.logout,
            self.getAccount,
            self.search,
            self.listPlaylists,
            self.getPlaylist,
            self.getRecommendations,
            self.getHomepage,
            self.getPlaylistRecommendations,
            self.resolveTrack,
            self.getLyrics,
            self.getTrackQualities,
            self.resolveTrackWithQuality,
        ]
        .get(method as usize)
        .copied()
        .flatten()
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct fmHostApi {
    pub structSize: u32,
    pub abiVersion: u32,
    pub fileApi: crate::files::fmFileApi,
    pub networkApi: crate::network::fmNetworkApi,
}

pub type fmGetApi = unsafe extern "C" fn(u32, *mut *const fmPluginApi) -> fmStatus;
