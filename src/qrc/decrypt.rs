use super::{MAX_ENCRYPTED_BYTES, MAX_TEXT_BYTES, des, limit_error};
use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Decompress, FlushDecompress, Status};
use fluent_plugin_api::ApiError;

fn encoding_error() -> ApiError {
    ApiError::new("LYRICS_ENCODING", "QRC 歌词编码无效")
}

fn decrypt_error() -> ApiError {
    ApiError::new("LYRICS_DECRYPT", "QRC 歌词解密失败或压缩数据损坏")
}

pub fn decrypt_qrc_api(value: &str) -> Result<String, ApiError> {
    if value.len() > MAX_ENCRYPTED_BYTES * 4 {
        return Err(limit_error());
    }
    let encoded: Vec<u8> = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let hex = STANDARD.decode(encoded).map_err(|_| encoding_error())?;
    let hex = std::str::from_utf8(&hex).map_err(|_| encoding_error())?;
    decrypt_qrc_hex(hex)
}

pub fn decrypt_qrc_hex(value: &str) -> Result<String, ApiError> {
    if value.len() > MAX_ENCRYPTED_BYTES * 2 {
        return Err(limit_error());
    }
    let value = value.trim();
    if value.is_empty() || !value.len().is_multiple_of(16) {
        return Err(encoding_error());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().as_chunks::<2>().0 {
        let digit = |byte: u8| (byte as char).to_digit(16).map(|value| value as u8);
        bytes.push(
            (digit(pair[0]).ok_or_else(encoding_error)? << 4)
                | digit(pair[1]).ok_or_else(encoding_error)?,
        );
    }
    des::decrypt_blocks(&mut bytes);
    inflate_text(&bytes)
}

fn inflate_text(bytes: &[u8]) -> Result<String, ApiError> {
    let mut inflater = Decompress::new(true);
    let mut result = Vec::new();
    loop {
        let input_start = inflater.total_in() as usize;
        let output_start = inflater.total_out();
        let mut buffer = [0; 8192];
        let status = inflater
            .decompress(&bytes[input_start..], &mut buffer, FlushDecompress::None)
            .map_err(|_| decrypt_error())?;
        let written = (inflater.total_out() - output_start) as usize;
        if result.len() + written > MAX_TEXT_BYTES {
            return Err(limit_error());
        }
        result.extend_from_slice(&buffer[..written]);
        if status == Status::StreamEnd {
            let padding = &bytes[inflater.total_in() as usize..];
            if padding.len() > 8 || padding.iter().any(|byte| *byte != 0) {
                return Err(decrypt_error());
            }
            return String::from_utf8(result).map_err(|_| encoding_error());
        }
        if input_start == inflater.total_in() as usize && written == 0 {
            return Err(decrypt_error());
        }
    }
}
