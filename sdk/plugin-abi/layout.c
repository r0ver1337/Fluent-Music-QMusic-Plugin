#include "include/fluentMusic/plugin.h"
#include <stddef.h>
size_t fmCTrackSize(void) { return sizeof(fmTrack); }
size_t fmCRequestSize(void) { return sizeof(fmRequest); }
size_t fmCPluginApiSize(void) { return sizeof(fmPluginApi); }
size_t fmCHostApiSize(void) { return sizeof(fmHostApi); }
size_t fmCNetworkApiSize(void) { return sizeof(fmNetworkApi); }
size_t fmCNetworkApiOffset(void) { return offsetof(fmHostApi, networkApi); }
uint32_t fmCAbiVersion(void) { return fmAbiVersion; }
size_t fmCFileRequestSize(void) { return sizeof(fmFileRequest); }
size_t fmCFileReplySize(void) { return sizeof(fmFileReply); }
size_t fmCTrackDurationOffset(void) { return offsetof(fmTrack, durationMs); }
size_t fmCLyricWordSize(void) { return sizeof(fmLyricWord); }
size_t fmCLyricLineSize(void) { return sizeof(fmLyricLine); }
size_t fmCLyricTimelineSize(void) { return sizeof(fmLyricTimeline); }
size_t fmCLyricsSize(void) { return sizeof(fmLyrics); }
size_t fmCGetLyricsOffset(void) { return offsetof(fmPluginApi, getLyrics); }
size_t fmCLyricLineWordsOffset(void) { return offsetof(fmLyricLine, words); }
size_t fmCQualityLevelSize(void) { return sizeof(fmQualityLevel); }
size_t fmCTrackQualitySize(void) { return sizeof(fmTrackQuality); }
size_t fmCQualityListSize(void) { return sizeof(fmQualityList); }
size_t fmCGetTrackQualitiesOffset(void) { return offsetof(fmPluginApi, getTrackQualities); }
size_t fmCResolveTrackWithQualityOffset(void) { return offsetof(fmPluginApi, resolveTrackWithQuality); }
size_t fmCTrackQualitySizeOffset(void) { return offsetof(fmTrackQuality, fileSizeBytes); }
