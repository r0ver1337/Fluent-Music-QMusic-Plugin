#ifndef fluentMusicQualitiesH
#define fluentMusicQualitiesH
#include "types.h"
typedef struct fmQualityLevel {
    uint32_t structSize, priority;
    fmStringView name;
    uint32_t disabled;
} fmQualityLevel;
typedef struct fmTrackQuality {
    uint32_t structSize;
    fmStringView id, name;
    uint32_t priority, hasFileSize;
    uint64_t fileSizeBytes;
    uint32_t playable;
    fmStringView unavailableReason;
} fmTrackQuality;
typedef struct fmQualityList {
    uint32_t structSize;
    fmSlice levels, items;
} fmQualityList;
#endif
