mod lrc;
mod normalize;
#[cfg(test)]
mod tests;

use crate::{
    protocol::{fail, success},
    qrc,
};
use fluent_plugin_api::{ApiError, LyricLine, LyricTimeline, LyricWord, Lyrics, Track};
use serde_json::Value;

pub(crate) const COMMAND: &str = "fcg_music_custom_get_lyric.fcg";

pub(crate) fn request_parameters(track: &Track) -> Result<Vec<(&'static str, String)>, ApiError> {
    if track.provider_id != "qqmusic" {
        return Err(fail("PROVIDER", "歌曲不属于 QQ 插件"));
    }
    let alias = track.id.strip_prefix("mid:");
    let mid = track
        .mid
        .as_deref()
        .filter(|mid| !mid.is_empty())
        .or(alias)
        .unwrap_or_default();
    if alias.is_some_and(|alias| alias != mid)
        || (!mid.is_empty()
            && (mid.len() > 128 || !mid.bytes().all(|byte| byte.is_ascii_alphanumeric())))
    {
        return Err(fail("PARAMETERS", "歌曲 mid 无效或不一致"));
    }
    let id = if alias.is_some() || track.id.is_empty() {
        ""
    } else {
        if track.id.len() > 19
            || !track.id.bytes().all(|byte| byte.is_ascii_digit())
            || track
                .id
                .parse::<u64>()
                .ok()
                .is_none_or(|id| id == 0 || id > i64::MAX as u64)
        {
            return Err(fail("PARAMETERS", "歌曲 ID 无效"));
        }
        &track.id
    };
    if id.is_empty() && mid.is_empty() {
        return Err(fail("PARAMETERS", "歌词请求需要歌曲 ID 或 mid"));
    }
    Ok(vec![
        ("song_id", id.to_owned()),
        ("song_mid", mid.to_owned()),
        ("qrc_no_enc", "1".into()),
    ])
}

pub(crate) fn parse_response(data: Value, duration_ms: Option<u64>) -> Result<Lyrics, ApiError> {
    let data = success(data)?;
    let qrc_text = field(&data, "song_qrc_enc_lyric")?;
    let lrc_text = field(&data, "song_lyric")?;
    let translation_text = field(&data, "song_translate_enc_lyric")?;
    let original = normalize::original(qrc_text, lrc_text, duration_ms)?;
    let translation = if translation_text.is_empty() || translation_text.trim() == "//" {
        None
    } else {
        let text = qrc::decrypt_qrc_api(translation_text)?;
        let plain = text.trim().trim_start_matches('\u{feff}').trim();
        let is_qrc = plain.starts_with('<')
            || plain.split(['\r', '\n']).any(|line| {
                line.trim()
                    .strip_prefix('[')
                    .and_then(|text| text.split_once(']'))
                    .is_some_and(|(header, _)| header.contains(',') && !header.contains(':'))
            });
        Some(if is_qrc {
            from_qrc(qrc::parse_qrc(plain)?)
        } else {
            lrc::parse_lrc(&text, duration_ms)?
        })
    };
    let start = match data.get("lyric_start_ms") {
        None | Some(Value::Null) => None,
        Some(Value::Number(number)) => Some(number.as_i64().ok_or_else(schema_error)?),
        Some(Value::String(text)) => Some(text.parse::<i64>().map_err(|_| schema_error())?),
        _ => return Err(schema_error()),
    };
    let lyrics = Lyrics {
        Original: original.filter(|timeline| !timeline.Lines.is_empty()),
        Translation: translation.and_then(normalize::translation),
        StartMs: start,
    };
    lyrics.validate()?;
    Ok(lyrics)
}

fn from_qrc(value: qrc::QrcLyric) -> LyricTimeline {
    LyricTimeline {
        OffsetMs: value.offset_ms,
        Lines: value
            .lines
            .into_iter()
            .map(|line| LyricLine {
                StartMs: Some(line.start_ms),
                DurationMs: Some(line.duration_ms),
                Text: line.text,
                Words: line
                    .words
                    .into_iter()
                    .map(|word| LyricWord {
                        StartMs: word.start_ms as i64,
                        DurationMs: word.duration_ms,
                        Text: word.text,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn field<'a>(data: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    match data.get(key) {
        None | Some(Value::Null) => Ok(""),
        Some(Value::String(text)) => Ok(text),
        _ => Err(schema_error()),
    }
}

fn schema_error() -> ApiError {
    fail("SCHEMA", "QQ 歌词响应字段类型无效")
}
