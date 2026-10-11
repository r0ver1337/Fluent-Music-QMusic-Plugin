use crate::http::Client;
use crate::{protocol::fail, qmc};
use fluent_plugin_api::{ApiError, MediaSource};
use serde_json::Value;
use std::{
    collections::{BTreeMap, VecDeque},
    io::Read,
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;
pub mod qualities;
mod quality_formats;

const MAX_MEDIA_BYTES: u64 = 512 * 1024 * 1024;

fn temporary_audio() -> Result<NamedTempFile, ApiError> {
    tempfile::Builder::new()
        .prefix("fluent-qqmusic-")
        .suffix(".audio")
        .make(|path| {
            let mut options = std::fs::OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                // The host kills plugin processes at shutdown; Rust Drop alone is insufficient.
                // Windows removes the cache after the last plugin/decoder handle closes.
                options.custom_flags(0x04000000); // FILE_FLAG_DELETE_ON_CLOSE
            }
            options.open(path)
        })
        .map_err(|_| fail("MEDIA", "无法创建 QQ 临时音频文件"))
}

fn valid_url(raw: &str) -> Result<url::Url, ApiError> {
    let parsed = url::Url::parse(raw).map_err(|_| fail("MEDIA", "QQ 返回的播放地址无效"))?;
    let host = parsed.host_str().unwrap_or_default();
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || !host.ends_with(".qq.com")
    {
        return Err(fail("MEDIA", "QQ 播放地址不属于允许的 QQ HTTP(S) 域名"));
    }
    Ok(parsed)
}

fn text<'a>(song: &'a Value, key: &str) -> Option<&'a str> {
    song[key].as_str().filter(|value| !value.is_empty())
}

// Deliberately no Debug/Serialize: EKeys never cross the plugin protocol.
pub struct Playback<'a> {
    pub url: &'a str,
    pub ekey: Option<&'a str>,
}

/// Resolve only URLs and EKeys returned for the requested, authenticated song.
/// No client database, static key, cookie, or alternative URL is consulted.
pub fn select(song: &Value) -> Result<Playback<'_>, ApiError> {
    let denied = |v: &Value| v.as_bool() == Some(false) || crate::protocol::unsigned(v) == Some(0);
    if denied(&song["playable"]) || denied(&song["copyright"]) {
        return Err(fail("UNPLAYABLE", "此歌曲不可播放或无版权"));
    }
    // Preserve the existing quality preference; never mix one quality's URL and another's key.
    let choices = [
        ("song_play_url_standard", "song_ekey_standard"),
        ("song_play_url", "song_ekey_lq"),
        ("song_play_url_hq", "song_ekey_hq"),
        ("song_play_url_sq", "song_ekey_sq"),
    ];
    for (url_key, ekey_key) in choices {
        let Some(url) = text(song, url_key) else {
            continue;
        };
        valid_url(url)?;
        return Ok(Playback {
            url,
            ekey: text(song, ekey_key),
        });
    }
    Err(fail(
        "UNPLAYABLE",
        "没有可供播放的普通音频或服务器授权的 QMC v2 音频",
    ))
}

#[derive(Default)]
pub struct Adapter {
    http: Option<Client>,
    files: VecDeque<NamedTempFile>,
    broker_files: VecDeque<crate::broker_media::BrokerMedia>,
}

impl Adapter {
    pub fn clear(&mut self) {
        self.files.clear();
        self.broker_files.clear();
    }

    pub fn source_until(
        &mut self,
        song: &Value,
        deadline: Instant,
    ) -> Result<MediaSource, ApiError> {
        let playback = select(song)?;
        self.source_playback_until(playback, deadline)
    }

    pub fn source_quality_until(
        &mut self,
        song: &Value,
        quality_id: &str,
        deadline: Instant,
    ) -> Result<MediaSource, ApiError> {
        self.source_playback_until(qualities::select(song, quality_id)?, deadline)
    }

    fn source_playback_until(
        &mut self,
        playback: Playback<'_>,
        deadline: Instant,
    ) -> Result<MediaSource, ApiError> {
        let Some(ekey) = playback.ekey else {
            return Ok(MediaSource::Http {
                url: playback.url.into(),
                headers: BTreeMap::new(),
            });
        };
        if self.http.is_none() {
            self.http = Some(
                Client::builder()
                    // Leave room within the host's 90-second RPC deadline for resolution/conversion.
                    .timeout(Duration::from_secs(45))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(|_| fail("MEDIA", "QQ 音频传输初始化失败"))?,
            );
        }
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(45));
        if timeout.is_zero() {
            return Err(fail("MEDIA", "QQ 音频解析或格式转换超时"));
        }
        let response = self
            .http
            .as_ref()
            .unwrap()
            .get(valid_url(playback.url)?)
            .timeout(timeout)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .map_err(|_| fail("MEDIA", "QQ 音频下载失败或超时"))?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(ApiError::new(
                "MEDIA",
                format!("QQ 音频 HTTP 状态 {}", response.status().as_u16()),
            ));
        }
        if response
            .content_length()
            .is_some_and(|len| len > MAX_MEDIA_BYTES)
        {
            return Err(fail("MEDIA", "QQ 音频超过 512 MiB 本地限制"));
        }
        if fluent_plugin_abi::io::available() {
            let file = crate::broker_media::convert(response, ekey, deadline, MAX_MEDIA_BYTES)?;
            let source = file.source();
            self.broker_files.push_back(file);
            while self.broker_files.len() > 2 {
                self.broker_files.pop_front();
            }
            return Ok(source);
        }
        let mut file = temporary_audio()?;
        let copied = std::io::copy(&mut response.take(MAX_MEDIA_BYTES + 1), file.as_file_mut())
            .map_err(|_| fail("MEDIA", "QQ 音频缓存写入失败或下载超时"))?;
        if copied > MAX_MEDIA_BYTES {
            return Err(fail("MEDIA", "QQ 音频超过 512 MiB 本地限制"));
        }
        qmc::convert_until(file.as_file_mut(), ekey, deadline)?;
        Ok(self.retain(file))
    }

    #[cfg(test)]
    fn source(&mut self, song: &Value) -> Result<MediaSource, ApiError> {
        self.source_until(song, Instant::now() + Duration::from_secs(60))
    }

    fn retain(&mut self, file: NamedTempFile) -> MediaSource {
        let source = MediaSource::File {
            path: file.path().to_string_lossy().into_owned(),
        };
        self.files.push_back(file);
        // Keep the previous source while the player switches; do not accumulate a music library.
        while self.files.len() > 2 {
            self.files.pop_front();
        }
        source
    }
}

#[cfg(test)]
mod tests;
