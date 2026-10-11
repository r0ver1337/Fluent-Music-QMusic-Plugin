use crate::protocol::fail;
use fluent_plugin_api::{ApiError, LyricLine, LyricTimeline};

pub(super) fn parse_lrc(value: &str, duration_ms: Option<u64>) -> Result<LyricTimeline, ApiError> {
    if value.len() > 2 * 1024 * 1024 {
        return Err(fail("LYRICS_LIMIT", "LRC 歌词超过本地处理限制"));
    }
    let value = unescape(value.trim().trim_start_matches('\u{feff}'));
    let mut timeline = LyricTimeline::default();
    for line in value
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut text = line;
        let mut starts = Vec::new();
        while let Some(header) = text.strip_prefix('[') {
            let Some((token, tail)) = header.split_once(']') else {
                return Err(invalid());
            };
            if token.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                starts.push(timestamp(token)?);
            } else if let Some((key, value)) = token.split_once(':') {
                if key == "offset" {
                    timeline.OffsetMs = value.trim().parse().map_err(|_| invalid())?;
                }
            } else {
                break;
            }
            text = tail;
        }
        if text.is_empty() && starts.is_empty() {
            continue;
        }
        if starts.is_empty() {
            timeline.Lines.push(LyricLine {
                StartMs: None,
                DurationMs: None,
                Text: text.to_owned(),
                Words: vec![],
            });
        } else {
            for start in starts {
                timeline.Lines.push(LyricLine {
                    StartMs: Some(start),
                    DurationMs: None,
                    Text: text.to_owned(),
                    Words: vec![],
                });
                if timeline.Lines.len() > 10_000 {
                    return Err(fail("LYRICS_LIMIT", "LRC 歌词行数超过限制"));
                }
            }
        }
        if timeline.Lines.len() > 10_000 {
            return Err(fail("LYRICS_LIMIT", "LRC 歌词行数超过限制"));
        }
    }
    if timeline.Lines.iter().any(|line| line.StartMs.is_some()) {
        if timeline.Lines.iter().any(|line| line.StartMs.is_none()) {
            return Err(invalid());
        }
        timeline.Lines.sort_by_key(|line| line.StartMs);
        let mut index = 0;
        while index < timeline.Lines.len() {
            let start = timeline.Lines[index].StartMs.unwrap();
            let mut end = index + 1;
            while end < timeline.Lines.len() && timeline.Lines[end].StartMs == Some(start) {
                end += 1;
            }
            let next = timeline
                .Lines
                .get(end)
                .and_then(|line| line.StartMs)
                .or_else(|| duration_ms.and_then(|duration| i64::try_from(duration).ok()));
            let duration = next
                .filter(|next| *next >= start)
                .map(|next| (next - start) as u64);
            for line in &mut timeline.Lines[index..end] {
                line.DurationMs = duration;
            }
            index = end;
        }
    }
    Ok(timeline)
}

fn timestamp(value: &str) -> Result<i64, ApiError> {
    let mut fields = value.split([':', '.']);
    let minutes: i64 = fields
        .next()
        .ok_or_else(invalid)?
        .parse()
        .map_err(|_| invalid())?;
    let seconds: i64 = fields
        .next()
        .ok_or_else(invalid)?
        .parse()
        .map_err(|_| invalid())?;
    if minutes < 0 || !(0..60).contains(&seconds) {
        return Err(invalid());
    }
    let fraction = match fields.next() {
        None => 0,
        Some(text)
            if (1..=3).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            text.parse::<i64>().map_err(|_| invalid())? * 10i64.pow(3 - text.len() as u32)
        }
        _ => return Err(invalid()),
    };
    if fields.next().is_some() {
        return Err(invalid());
    }
    minutes
        .checked_mul(60)
        .and_then(|value| value.checked_add(seconds))
        .and_then(|value| value.checked_mul(1000))
        .and_then(|value| value.checked_add(fraction))
        .ok_or_else(invalid)
}

fn unescape(mut value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    while let Some(index) = value.find('&') {
        output.push_str(&value[..index]);
        value = &value[index..];
        let entity = [
            ("&amp;", '&'),
            ("&lt;", '<'),
            ("&gt;", '>'),
            ("&quot;", '"'),
            ("&apos;", '\''),
        ]
        .into_iter()
        .find(|(entity, _)| value.starts_with(entity));
        if let Some((entity, character)) = entity {
            output.push(character);
            value = &value[entity.len()..];
        } else {
            output.push('&');
            value = &value[1..];
        }
    }
    output.push_str(value);
    output
}

fn invalid() -> ApiError {
    fail("LYRICS_FORMAT", "LRC 歌词时间戳或结构无效")
}
