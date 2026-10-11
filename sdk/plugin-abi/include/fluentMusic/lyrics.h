#ifndef fluentMusicLyricsH
#define fluentMusicLyricsH
#include "types.h"
typedef struct fmLyricWord {
    uint32_t structSize;
    int64_t startMs;
    uint64_t durationMs;
    fmStringView text;
} fmLyricWord;
typedef struct fmLyricLine {
    uint32_t structSize, hasStart;
    int64_t startMs;
    uint32_t hasDuration;
    uint64_t durationMs;
    fmStringView text;
    fmSlice words;
} fmLyricLine;
typedef struct fmLyricTimeline {
    uint32_t structSize;
    int64_t offsetMs;
    fmSlice lines;
} fmLyricTimeline;
typedef struct fmLyrics {
    uint32_t structSize;
    const fmLyricTimeline *original, *translation;
    uint32_t hasStart;
    int64_t startMs;
} fmLyrics;
#endif
