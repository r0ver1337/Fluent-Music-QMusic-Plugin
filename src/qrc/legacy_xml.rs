use super::{MAX_TEXT_BYTES, limit_error};
use fluent_plugin_api::ApiError;

pub(super) fn repair_quotes(value: &str) -> Result<Option<String>, ApiError> {
    let mut fields = value.match_indices("LyricContent");
    let Some((field, _)) = fields.next() else {
        return Ok(None);
    };
    if fields.next().is_some()
        || !value
            .as_bytes()
            .get(field.wrapping_sub(1))
            .is_some_and(u8::is_ascii_whitespace)
    {
        return Ok(None);
    }
    let rest = value[field + "LyricContent".len()..].trim_start();
    let Some(rest) = rest.strip_prefix('=').map(str::trim_start) else {
        return Ok(None);
    };
    let Some(delimiter @ ('"' | '\'')) = rest.chars().next() else {
        return Ok(None);
    };
    let body = &rest[1..];
    let Some(tag_end) = body.find("/>") else {
        return Ok(None);
    };
    let Some(closing) = body[..tag_end].rfind(delimiter) else {
        return Ok(None);
    };
    if !body[closing + 1..tag_end]
        .bytes()
        .all(|byte| byte.is_ascii_whitespace())
    {
        return Ok(None);
    }
    let raw = &body[..closing];
    if trailing_attribute(raw, delimiter) {
        return Ok(None);
    }
    if !raw.contains(delimiter) {
        return Ok(None);
    }
    let extra = raw
        .chars()
        .filter(|character| *character == delimiter)
        .count()
        * 5;
    if value
        .len()
        .checked_add(extra)
        .is_none_or(|length| length > MAX_TEXT_BYTES)
    {
        return Err(limit_error());
    }
    let start = value.len() - body.len();
    let mut repaired = String::with_capacity(value.len() + extra);
    repaired.push_str(&value[..start]);
    // QQ's legacy envelope ends this last attribute at the quote before '/>', not at lyric punctuation.
    for character in raw.chars() {
        if character == delimiter {
            repaired.push_str(if delimiter == '"' { "&quot;" } else { "&apos;" });
        } else {
            repaired.push(character);
        }
    }
    repaired.push_str(&value[start + closing..]);
    Ok(Some(repaired))
}

fn trailing_attribute(value: &str, delimiter: char) -> bool {
    value.match_indices(delimiter).any(|(index, _)| {
        let tail = &value[index + 1..];
        let text = tail.trim_start();
        if text.len() == tail.len() {
            return false;
        }
        let name_end = text
            .find(|character: char| {
                !character.is_alphanumeric() && !matches!(character, '_' | '-' | ':' | '.')
            })
            .unwrap_or(text.len());
        if name_end == 0 {
            return false;
        }
        text[name_end..]
            .trim_start()
            .strip_prefix('=')
            .is_some_and(|value| value.trim_start().starts_with(['\'', '"']))
    })
}
