#ifndef fluentMusicPluginH
#define fluentMusicPluginH
#include "files.h"
#include "lyrics.h"
#include "qualities.h"
#include "network.h"
typedef struct fmHostApi { uint32_t structSize, abiVersion; fmFileApi fileApi; fmNetworkApi networkApi; } fmHostApi;
typedef fmStatus (fmCallConvention *fmCall)(fmHandle provider, const fmCallContext *context, const fmRequest *request, fmHandle *outResult);
typedef struct fmPluginApi {
    uint32_t structSize, abiVersion;
    fmStatus (fmCallConvention *createProvider)(const fmHostApi *host, const fmCreateInfo *info, fmHandle *outProvider);
    fmStatus (fmCallConvention *destroyProvider)(fmHandle provider);
    fmCall describe, loginBegin, loginPoll, logout, getAccount, search;
    fmCall listPlaylists, getPlaylist, getRecommendations, getHomepage;
    fmCall getPlaylistRecommendations, resolveTrack;
    const fmResultView *(fmCallConvention *getResultView)(fmHandle result);
    void (fmCallConvention *releaseResult)(fmHandle result);
    fmCall getLyrics;
    fmCall getTrackQualities, resolveTrackWithQuality;
} fmPluginApi;
#ifdef __cplusplus
extern "C" {
#endif
fmExport fmStatus fmCallConvention fmPluginGetApi(uint32_t requestedAbiVersion, const fmPluginApi **outApi);
#ifdef __cplusplus
}
#endif
#endif
