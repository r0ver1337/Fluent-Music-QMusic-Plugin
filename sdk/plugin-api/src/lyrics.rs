#![allow(non_snake_case)]
use crate::{ApiError, MAX_MESSAGE_BYTES};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    pub Original: Option<LyricTimeline>,
    pub Translation: Option<LyricTimeline>,
    pub StartMs: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricTimeline {
    pub OffsetMs: i64,
    pub Lines: Vec<LyricLine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    pub StartMs: Option<i64>,
    pub DurationMs: Option<u64>,
    pub Text: String,
    pub Words: Vec<LyricWord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricWord {
    pub StartMs: i64,
    pub DurationMs: u64,
    pub Text: String,
}

impl Lyrics {
    pub fn validate(&self) -> Result<(), ApiError> {
        let mut remaining = MAX_MESSAGE_BYTES - 256;
        let mut lines = 0;
        let mut words = 0;
        for timeline in [&self.Original, &self.Translation].into_iter().flatten() {
            let timed = timeline
                .Lines
                .first()
                .is_some_and(|line| line.StartMs.is_some());
            let mut previous = None;
            for line in &timeline.Lines {
                lines += 1;
                spend(&mut remaining, 80 + line.Text.len())?;
                if lines > 10_000 {
                    return Err(limit());
                }
                if line.StartMs.is_some() != timed {
                    return Err(invalid());
                }
                if let Some(start) = line.StartMs {
                    if previous.is_some_and(|previous| start < previous) {
                        return Err(invalid());
                    }
                    previous = Some(start);
                    if let Some(duration) = line.DurationMs {
                        checkEnd(start, duration)?;
                    }
                } else if line.DurationMs.is_some() || !line.Words.is_empty() {
                    return Err(invalid());
                }
                let mut textPosition = 0;
                let mut previousWord = None;
                for word in &line.Words {
                    words += 1;
                    spend(&mut remaining, 64 + word.Text.len())?;
                    if words > 65_536 {
                        return Err(limit());
                    }
                    if previousWord.is_some_and(|start| word.StartMs < start) {
                        return Err(invalid());
                    }
                    checkEnd(word.StartMs, word.DurationMs)?;
                    previousWord = Some(word.StartMs);
                    if word.Text.is_empty() || !line.Text[textPosition..].starts_with(&word.Text) {
                        return Err(invalid());
                    }
                    textPosition += word.Text.len();
                }
                if !line.Words.is_empty() && textPosition != line.Text.len() {
                    return Err(invalid());
                }
            }
        }
        if serde_json::to_vec(self).map_err(|_| invalid())?.len() >= MAX_MESSAGE_BYTES - 256 {
            return Err(limit());
        }
        Ok(())
    }
}

fn checkEnd(start: i64, duration: u64) -> Result<(), ApiError> {
    let duration = i64::try_from(duration).map_err(|_| invalid())?;
    start.checked_add(duration).ok_or_else(invalid)?;
    Ok(())
}

fn spend(remaining: &mut usize, bytes: usize) -> Result<(), ApiError> {
    *remaining = remaining.checked_sub(bytes).ok_or_else(limit)?;
    Ok(())
}

fn invalid() -> ApiError {
    ApiError::new("LYRICS_FORMAT", "歌词时间轴、逐字文本或时间数据无效")
}

fn limit() -> ApiError {
    ApiError::new("LYRICS_LIMIT", "歌词超过通用插件传输限制")
}
