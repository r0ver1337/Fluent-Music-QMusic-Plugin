use crate::{arena::Arena, decode, types::*};
use fluent_plugin_api::*;

impl Arena {
    pub(crate) fn lyrics(&mut self, value: &Lyrics) -> fmLyrics {
        fmLyrics {
            original: self.lyricTimeline(value.Original.as_ref()),
            translation: self.lyricTimeline(value.Translation.as_ref()),
            hasStart: value.StartMs.is_some() as u32,
            startMs: value.StartMs.unwrap_or_default(),
            ..fmLyrics::new()
        }
    }

    fn lyricTimeline(&mut self, value: Option<&LyricTimeline>) -> *const fmLyricTimeline {
        let Some(value) = value else {
            return std::ptr::null();
        };
        let lines = value
            .Lines
            .iter()
            .map(|line| {
                let words = line
                    .Words
                    .iter()
                    .map(|word| fmLyricWord {
                        startMs: word.StartMs,
                        durationMs: word.DurationMs,
                        text: self.text(&word.Text),
                        ..fmLyricWord::new()
                    })
                    .collect();
                fmLyricLine {
                    hasStart: line.StartMs.is_some() as u32,
                    startMs: line.StartMs.unwrap_or_default(),
                    hasDuration: line.DurationMs.is_some() as u32,
                    durationMs: line.DurationMs.unwrap_or_default(),
                    text: self.text(&line.Text),
                    words: self.array(words),
                    ..fmLyricLine::new()
                }
            })
            .collect();
        let timeline = fmLyricTimeline {
            offsetMs: value.OffsetMs,
            lines: self.array(lines),
            ..fmLyricTimeline::new()
        };
        self.keep(timeline)
    }
}

pub(crate) unsafe fn decodeLyrics(pointer: *const std::ffi::c_void) -> Result<Lyrics, ApiError> {
    let value: fmLyrics = unsafe { decode::body(pointer)? };
    if value.hasStart > 1 {
        return Err(decode::invalid());
    }
    let lyrics = Lyrics {
        Original: unsafe { timeline(value.original)? },
        Translation: unsafe { timeline(value.translation)? },
        StartMs: (value.hasStart == 1).then_some(value.startMs),
    };
    lyrics.validate()?;
    Ok(lyrics)
}

unsafe fn timeline(pointer: *const fmLyricTimeline) -> Result<Option<LyricTimeline>, ApiError> {
    if pointer.is_null() {
        return Ok(None);
    }
    let value: fmLyricTimeline = unsafe { decode::body(pointer.cast())? };
    let mut lines = Vec::new();
    for line in unsafe { decode::array::<fmLyricLine>(value.lines)? } {
        if line.structSize < size_of::<fmLyricLine>() as u32
            || line.hasStart > 1
            || line.hasDuration > 1
        {
            return Err(decode::invalid());
        }
        let mut words = Vec::new();
        for word in unsafe { decode::array::<fmLyricWord>(line.words)? } {
            if word.structSize < size_of::<fmLyricWord>() as u32 {
                return Err(decode::invalid());
            }
            words.push(LyricWord {
                StartMs: word.startMs,
                DurationMs: word.durationMs,
                Text: unsafe { decode::text(word.text)? },
            });
        }
        lines.push(LyricLine {
            StartMs: (line.hasStart == 1).then_some(line.startMs),
            DurationMs: (line.hasDuration == 1).then_some(line.durationMs),
            Text: unsafe { decode::text(line.text)? },
            Words: words,
        });
    }
    Ok(Some(LyricTimeline {
        OffsetMs: value.offsetMs,
        Lines: lines,
    }))
}
