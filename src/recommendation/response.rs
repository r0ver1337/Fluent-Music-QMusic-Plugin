use super::RESPONSE_LIMIT;
use crate::protocol::fail;
use fluent_plugin_api::ApiError;
use serde_json::Value;
use std::io::Read;

pub(super) fn decompress(bytes: &[u8], gzip: bool) -> Result<Vec<u8>, ApiError> {
    let mut out = Vec::with_capacity(RESPONSE_LIMIT + 1);
    if gzip {
        let mut reader = flate2::bufread::GzDecoder::new(bytes);
        reader
            .by_ref()
            .take(RESPONSE_LIMIT as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|_| fail("SCHEMA", "Flyme 压缩响应无效"))?;
        if !reader.get_ref().is_empty() {
            return Err(fail("SCHEMA", "Flyme 压缩响应含多余内容"));
        }
    } else {
        let mut decoder = flate2::Decompress::new(true);
        let status = decoder
            .decompress_vec(bytes, &mut out, flate2::FlushDecompress::Finish)
            .map_err(|_| fail("SCHEMA", "Flyme 压缩响应无效"))?;
        if status != flate2::Status::StreamEnd || decoder.total_in() != bytes.len() as u64 {
            return Err(fail("SCHEMA", "Flyme 压缩响应不完整或超出限制"));
        }
    }
    if out.len() > RESPONSE_LIMIT {
        return Err(fail("SCHEMA", "Flyme 解压内容超出限制"));
    }
    Ok(out)
}
pub(super) fn check_status(value: &Value, required: bool) -> Result<(), ApiError> {
    if required && !value.get("code").is_some_and(Value::is_i64) {
        return Err(fail("SCHEMA", "Flyme 响应缺少整数 code"));
    }
    for field in ["code", "ret", "sub_ret", "subRet"] {
        if let Some(v) = value.get(field) {
            let n = v
                .as_i64()
                .ok_or_else(|| fail("SCHEMA", "Flyme 响应状态类型无效"))?;
            if n != 0 {
                return Err(ApiError::new(
                    "FLYME_SERVER",
                    format!("Flyme 拒绝请求：{field}={n}（未改动登录授权）"),
                ));
            }
        }
    }
    Ok(())
}
