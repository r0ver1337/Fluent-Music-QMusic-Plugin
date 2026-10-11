use super::*;

pub fn container(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(b"fLaC") {
        Some("flac")
    } else if head.starts_with(b"OggS") {
        Some("ogg")
    } else if head.starts_with(b"ID3") {
        Some("mp3")
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WAVE") {
        Some("wav")
    } else if head.get(4..8) == Some(b"ftyp") {
        Some("m4a")
    } else if head.len() >= 2 && head[0] == 0xff && head[1] & 0xe0 == 0xe0 {
        Some("mp3")
    } else {
        None
    }
}

pub(super) fn footer_size(tail: &[u8]) -> Result<usize, ApiError> {
    let invalid = || fail("MEDIA_FORMAT", "QQ 音频尾包损坏或版本不支持");
    if tail.ends_with(b"QTag") || tail.ends_with(b"STag") {
        if tail.len() < 8 {
            return Err(invalid());
        }
        let size =
            u32::from_be_bytes(tail[tail.len() - 8..tail.len() - 4].try_into().unwrap()) as usize;
        if size > tail.len() - 8 || size == 0 {
            return Err(invalid());
        }
        let csv = std::str::from_utf8(&tail[tail.len() - 8 - size..tail.len() - 8])
            .map_err(|_| invalid())?;
        let parts: Vec<_> = csv.split(',').collect();
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let valid = parts.len() == 3
            && if tail.ends_with(b"QTag") {
                base64_text(parts[0].as_bytes()) && digits(parts[1]) && parts[2] == "2"
            } else {
                digits(parts[0]) && parts[1] == "2" && !parts[2].is_empty()
            };
        if !valid {
            return Err(invalid());
        }
        // Embedded keys are not a source of authorization. Only strip the metadata.
        return Ok(size + 8);
    }
    if tail.ends_with(b"musicex\0") {
        if tail.len() < 204 {
            return Err(invalid());
        }
        let version = u32::from_le_bytes(tail[tail.len() - 12..tail.len() - 8].try_into().unwrap());
        let size = u32::from_le_bytes(tail[tail.len() - 16..tail.len() - 12].try_into().unwrap());
        if version != 1 || size != 0xc0 {
            return Err(invalid());
        }
        return Ok(204);
    }
    if tail.len() >= 4 {
        let size = u32::from_le_bytes(tail[tail.len() - 4..].try_into().unwrap()) as usize;
        if size > 0 && size <= 0x500 && size <= tail.len() - 4 {
            let key = &tail[tail.len() - 4 - size..tail.len() - 4];
            let end = key.iter().position(|b| *b == 0).unwrap_or(key.len());
            if base64_text(&key[..end]) {
                return Ok(size + 4);
            }
        }
    }
    // Allow footerless network resources with an EKey from the same response.
    Ok(0)
}

pub(super) fn base64_text(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(b))
}
