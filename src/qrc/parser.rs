use super::{
    MAX_LINES, MAX_TEXT_BYTES, MAX_WORDS, QrcLine, QrcLyric, QrcWord, format_error, limit_error,
    xml,
};
use fluent_plugin_api::ApiError;

pub fn parse_qrc(value: &str) -> Result<QrcLyric, ApiError> {
    if value.len() > MAX_TEXT_BYTES {
        return Err(limit_error());
    }
    let value = value.trim().trim_start_matches('\u{feff}').trim();
    let content = if value.starts_with('<') {
        xml::lyric_content(value)?
    } else {
        value.to_owned()
    };
    let mut lyric = QrcLyric::default();
    let mut word_count = 0;
    for line in content
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if !line.starts_with('[') {
            return Err(format_error());
        }
        let end = line.find(']').ok_or_else(format_error)?;
        let header = &line[1..end];
        if !header.contains(':') {
            if lyric.lines.len() >= MAX_LINES {
                return Err(limit_error());
            }
            let parsed = parse_line(header, &line[end + 1..])?;
            word_count += parsed.words.len();
            if word_count > MAX_WORDS {
                return Err(limit_error());
            }
            lyric.lines.push(parsed);
        } else {
            parse_metadata(line, &mut lyric)?;
        }
    }
    lyric.lines.sort_by_key(|line| line.start_ms);
    Ok(lyric)
}

fn parse_metadata(mut value: &str, lyric: &mut QrcLyric) -> Result<(), ApiError> {
    while !value.trim().is_empty() {
        value = value.trim_start();
        let header = value.strip_prefix('[').ok_or_else(format_error)?;
        let end = header.find(']').ok_or_else(format_error)?;
        let (key, text) = header[..end].split_once(':').ok_or_else(format_error)?;
        if key.is_empty()
            || !key.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_alphabetic()
                    || (index > 0 && (byte.is_ascii_digit() || matches!(byte, b'_' | b'-')))
            })
        {
            return Err(format_error());
        }
        if !lyric.metadata.contains_key(key) && lyric.metadata.len() >= 128 {
            return Err(limit_error());
        }
        let text = text.trim();
        match key {
            "ti" => lyric.title = Some(text.to_owned()),
            "ar" => lyric.artist = Some(text.to_owned()),
            "al" => lyric.album = Some(text.to_owned()),
            "offset" => lyric.offset_ms = text.parse().map_err(|_| format_error())?,
            _ => {}
        }
        lyric.metadata.insert(key.to_owned(), text.to_owned());
        value = &header[end + 1..];
    }
    Ok(())
}

fn time_pair(value: &str) -> Result<(i64, i64), ApiError> {
    let (start, duration) = value.split_once(',').ok_or_else(format_error)?;
    Ok((
        start.trim().parse().map_err(|_| format_error())?,
        duration.trim().parse().map_err(|_| format_error())?,
    ))
}

fn parse_line(header: &str, value: &str) -> Result<QrcLine, ApiError> {
    let (start, duration) = time_pair(header)?;
    if duration < 0 || start.checked_add(duration).is_none() {
        return Err(format_error());
    }
    let mut line = QrcLine {
        start_ms: start,
        duration_ms: duration as u64,
        text: String::new(),
        words: Vec::new(),
    };
    let mut text_start = 0;
    let mut opening = None;
    let mut utf16_length = 0;
    for (closing, character) in value.char_indices() {
        if character == '(' {
            opening = Some(closing);
            continue;
        }
        if character != ')' {
            continue;
        }
        let Some(marker_start) = opening.take() else {
            continue;
        };
        let pair = &value[marker_start + 1..closing];
        let Some((start_token, duration_token)) = pair.split_once(',') else {
            continue;
        };
        let numeric = |token: &str| {
            let token = token.trim();
            token
                .strip_prefix('-')
                .unwrap_or(token)
                .bytes()
                .all(|byte| byte.is_ascii_digit())
        };
        if !numeric(start_token) || !numeric(duration_token) {
            continue;
        }
        let (word_start, word_duration) = time_pair(pair)?;
        let text = &value[text_start..marker_start];
        if text.is_empty() {
            return Err(format_error());
        }
        if line.words.len() >= MAX_WORDS {
            return Err(limit_error());
        }
        let (word_start, word_duration) = (word_start.max(0) as u64, word_duration.max(0) as u64);
        if word_start
            .checked_add(word_duration)
            .is_none_or(|end| end > i64::MAX as u64)
        {
            return Err(format_error());
        }
        let end_utf16 = utf16_length + text.encode_utf16().count();
        line.words.push(QrcWord {
            start_ms: word_start,
            duration_ms: word_duration,
            text: text.to_owned(),
            start_utf16: utf16_length,
            end_utf16,
        });
        line.text.push_str(text);
        utf16_length = end_utf16;
        text_start = closing + 1;
    }
    if line.words.is_empty() || !value[text_start..].trim().is_empty() {
        return Err(format_error());
    }
    Ok(line)
}
