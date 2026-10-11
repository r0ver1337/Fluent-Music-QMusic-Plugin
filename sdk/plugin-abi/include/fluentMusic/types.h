#ifndef fluentMusicTypesH
#define fluentMusicTypesH
#include <stdint.h>
#ifdef _WIN32
#define fmCallConvention __cdecl
#define fmExport __declspec(dllexport)
#else
#define fmCallConvention
#define fmExport
#endif
typedef int32_t fmStatus;
typedef void *fmHandle;
typedef struct fmStringView { const uint8_t *data; uint64_t length; } fmStringView;
typedef struct fmSlice { const void *data; uint64_t count, stride; } fmSlice;
typedef struct fmPluginInfo { uint32_t structSize; fmStringView id, name, version; uint64_t capabilities; } fmPluginInfo;
typedef struct fmAccount { uint32_t structSize; fmStringView displayName, avatarUrl; } fmAccount;
typedef struct fmTrack {
    uint32_t structSize;
    fmStringView id, providerId, mid, title;
    fmSlice artists;
    fmStringView album;
    uint32_t hasDuration;
    uint64_t durationMs;
    fmStringView coverUrl, thumbnailUrl;
} fmTrack;
typedef struct fmPlaylist { uint32_t structSize; fmStringView id, title; uint32_t hasTrackCount; uint64_t trackCount; fmStringView coverUrl, subtitle; } fmPlaylist;
typedef struct fmTrackPage { uint32_t structSize; fmSlice items; uint32_t page, hasMore; fmStringView cursor; } fmTrackPage;
typedef struct fmError { uint32_t structSize; fmStringView code, message; } fmError;
typedef struct fmIssue { uint32_t structSize; fmStringView stage; fmError error; } fmIssue;
typedef struct fmFeed { uint32_t structSize; fmSlice playlists, tracks; uint32_t personalized; fmStringView notice; uint32_t playlistSource, trackSource; fmSlice issues; } fmFeed;
typedef struct fmChallenge { uint32_t structSize; fmStringView challengeId, qrText; uint32_t expiresInSeconds; } fmChallenge;
typedef struct fmLoginState { uint32_t structSize, state; fmAccount account; } fmLoginState;
typedef struct fmHeader { uint32_t structSize; fmStringView name, value; } fmHeader;
typedef struct fmMedia { uint32_t structSize, kind; fmStringView url; fmSlice headers; uint64_t mediaHandle; } fmMedia;
typedef struct fmResultView { uint32_t structSize, kind; const void *data; } fmResultView;
typedef struct fmRequest { uint32_t structSize, method; fmStringView query, id; uint32_t page; fmStringView cursor; const fmTrack *track; } fmRequest;
typedef struct fmCreateInfo { uint32_t structSize; fmStringView pluginId; fmSlice options; } fmCreateInfo;
typedef struct fmCallContext {
    uint32_t structSize;
    uint64_t requestId, timeoutMs;
    void *userData;
    uint32_t (fmCallConvention *isCancelled)(void *userData);
} fmCallContext;
enum { fmAbiVersion = 2 };
enum { fmMethodDescribe, fmMethodLoginBegin, fmMethodLoginPoll, fmMethodLogout, fmMethodGetAccount, fmMethodSearch, fmMethodListPlaylists, fmMethodGetPlaylist, fmMethodGetRecommendations, fmMethodGetHomepage, fmMethodGetPlaylistRecommendations, fmMethodResolveTrack, fmMethodGetLyrics, fmMethodGetTrackQualities, fmMethodResolveTrackWithQuality };
enum { fmResultError, fmResultDescription, fmResultAccount, fmResultPlaylists, fmResultTracks, fmResultFeed, fmResultChallenge, fmResultLoginState, fmResultMedia, fmResultOk, fmResultLyrics, fmResultTrackQualities };
enum { fmCapabilityLogin = 1, fmCapabilityAccount = 2, fmCapabilitySearch = 4, fmCapabilityPlaylists = 8, fmCapabilityPlaylist = 16, fmCapabilityRecommendations = 32, fmCapabilityHomepage = 64, fmCapabilityPlaylistRecommendations = 128, fmCapabilityResolve = 256, fmCapabilityLyrics = 512, fmCapabilityTrackQualities = 1024 };
#endif
