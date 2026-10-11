#![allow(non_snake_case)]
use fluent_plugin_api::ApiError;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    time::Duration,
};
pub fn downloadHttp(
    url: String,
    headers: BTreeMap<String, String>,
    file: &mut File,
) -> Result<(), ApiError> {
    downloadControlled(url, headers, file, || true, false)
}

pub(crate) fn downloadControlled(
    url: String,
    headers: BTreeMap<String, String>,
    file: &mut File,
    wanted: impl Fn() -> bool,
    speculative: bool,
) -> Result<(), ApiError> {
    let parsed = url::Url::parse(&url).map_err(|_| ApiError::new("MEDIA", "音频地址无效"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(ApiError::new("MEDIA", "只支持 HTTP(S) 音频地址"));
    }
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(if speculative { 20 } else { 180 }))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| ApiError::new("MEDIA", "音频传输初始化失败"))?;
    let mut request = client.get(parsed);
    for (key, value) in headers {
        let name = reqwest::header::HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| ApiError::new("MEDIA", "音频请求头无效"))?;
        let value = reqwest::header::HeaderValue::from_str(&value)
            .map_err(|_| ApiError::new("MEDIA", "音频请求头无效"))?;
        request = request.header(name, value);
    }
    let mut response = request
        .send()
        .map_err(|_| ApiError::new("MEDIA", "音频下载失败"))?;
    if !response.status().is_success() {
        return Err(ApiError::new(
            "MEDIA",
            format!("音频 HTTP 状态 {}", response.status().as_u16()),
        ));
    }
    let limit = 512 * 1024 * 1024;
    let mut copied = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        if !wanted() {
            return Err(ApiError::new("CANCELLED", "音频缓存已取消"));
        }
        let count = response
            .read(&mut buffer)
            .map_err(|_| ApiError::new("MEDIA", "音频下载失败"))?;
        if count == 0 {
            break;
        }
        copied += count;
        if copied > limit {
            return Err(ApiError::new("MEDIA", "音频超过 512 MiB 本地限制"));
        }
        file.write_all(&buffer[..count])
            .map_err(|_| ApiError::new("MEDIA", "音频缓存写入失败"))?;
    }
    file.sync_all()
        .map_err(|_| ApiError::new("MEDIA", "音频缓存刷新失败"))?;
    Ok(())
}
