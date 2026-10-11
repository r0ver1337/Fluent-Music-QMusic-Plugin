use super::quality_formats::{FORMATS, Format};
use super::{Playback, text, valid_url};
use crate::protocol::{fail, unsigned};
use fluent_plugin_api::{ApiError, QualityLevel, QualityList, TrackQuality};
use serde_json::Value;
use std::collections::HashSet;

pub fn catalogue() -> QualityList {
    let levels = FORMATS
        .iter()
        .map(|format| QualityLevel {
            Priority: format.priority,
            Name: format.name.into(),
            Disabled: !format.supported,
        })
        .collect();
    QualityList {
        Levels: levels,
        Items: vec![],
    }
}

fn file_size(value: &Value) -> Result<Option<u64>, ApiError> {
    if value.is_null() {
        return Ok(None);
    }
    let size = unsigned(value).ok_or_else(|| fail("SCHEMA", "QQ 音质文件大小无效"))?;
    Ok((size > 0).then_some(size))
}

fn denied(song: &Value) -> bool {
    ["playable", "copyright"]
        .iter()
        .any(|field| song[field].as_bool() == Some(false) || unsigned(&song[field]) == Some(0))
}

fn access_denied(
    song: &Value,
    format: &Format,
    override_access: Option<&Value>,
) -> Result<bool, ApiError> {
    let field = match format.quality_type {
        0 => "FQ",
        4 => "STANDARD",
        5 => "HQ",
        6 => "SQ",
        12 => "DOLBY",
        13 => "XQ",
        _ => "",
    };
    let access = override_access
        .filter(|value| !value.is_null())
        .unwrap_or(&song["action"]["playAccess"][field]);
    if access.is_null() {
        return Ok(false);
    }
    Ok(unsigned(access).ok_or_else(|| fail("SCHEMA", "QQ 音质权限字段无效"))? == 0)
}

struct Entry<'a> {
    item: TrackQuality,
    playback: Option<Playback<'a>>,
}

fn entry<'a>(
    format: &Format,
    size: Option<u64>,
    url: Option<&'a str>,
    key: Option<&'a str>,
    blocked: bool,
) -> Result<Entry<'a>, ApiError> {
    if let Some(url) = url {
        valid_url(url)?;
    }
    let reason = if blocked {
        Some("当前账号未获授权或歌曲无版权")
    } else if !format.supported {
        Some("插件暂不支持此音质格式")
    } else if url.is_none() {
        Some("服务器未提供此音质的授权播放地址")
    } else {
        None
    };
    Ok(Entry {
        item: TrackQuality {
            Id: format.id.into(),
            Name: format.name.into(),
            Priority: format.priority,
            FileSizeBytes: size,
            Playable: reason.is_none(),
            UnavailableReason: reason.map(str::to_owned),
        },
        playback: if reason.is_none() {
            url.map(|url| Playback { url, ekey: key })
        } else {
            None
        },
    })
}

fn entries(song: &Value) -> Result<Vec<Entry<'_>>, ApiError> {
    let mut entries = Vec::new();
    let mut covered = HashSet::new();
    if let Some(value) = song.get("qualityList").filter(|value| !value.is_null()) {
        let list = value
            .as_array()
            .ok_or_else(|| fail("SCHEMA", "QQ 音质数组无效"))?;
        if list.len() > fluent_plugin_api::maxQualityEntries {
            return Err(fail("SCHEMA", "QQ 音质条目过多"));
        }
        for quality in list {
            let kind = unsigned(&quality["qualityType"])
                .or_else(|| unsigned(&quality["qulityType"]))
                .ok_or_else(|| fail("SCHEMA", "QQ 音质类型无效"))?;
            if !covered.insert(kind) {
                return Err(fail("SCHEMA", "QQ 返回重复的音质类型"));
            }
            let size = file_size(&quality["size"])?;
            if let Some(format) = FORMATS.iter().find(|format| format.quality_type == kind) {
                let blocked =
                    denied(song) || access_denied(song, format, quality.get("playAccess"))?;
                entries.push(entry(
                    format,
                    size,
                    text(quality, "url"),
                    text(quality, "eKey").or_else(|| text(quality, "ekey")),
                    blocked,
                )?);
            }
        }
    }
    for format in &FORMATS {
        if covered.contains(&format.quality_type) {
            continue;
        }
        let size = file_size(&song[format.size])?;
        let url = text(song, format.url);
        let key = text(song, format.key);
        if size.is_some() || url.is_some() || key.is_some() {
            entries.push(entry(
                format,
                size,
                url,
                key,
                denied(song) || access_denied(song, format, None)?,
            )?);
        }
    }
    entries.sort_by_key(|entry| entry.item.Priority);
    Ok(entries)
}

pub fn list(song: &Value) -> Result<QualityList, ApiError> {
    let mut list = catalogue();
    list.Items = entries(song)?.into_iter().map(|entry| entry.item).collect();
    list.validate()?;
    Ok(list)
}

pub fn select<'a>(song: &'a Value, quality_id: &str) -> Result<Playback<'a>, ApiError> {
    if quality_id.is_empty() || quality_id.len() > 128 {
        return Err(fail("PARAMETERS", "音质标识无效"));
    }
    entries(song)?
        .into_iter()
        .find(|entry| entry.item.Id == quality_id)
        .and_then(|entry| entry.playback)
        .ok_or_else(|| fail("QUALITY_UNAVAILABLE", "所选音质不存在、未获授权或暂不支持"))
}

#[cfg(test)]
#[path = "quality_tests.rs"]
mod tests;
