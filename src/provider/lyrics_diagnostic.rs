use super::*;
use std::io::Read;
use std::{io::Write, path::Path};

pub fn replay_lyrics(path: &Path) -> Result<Value, ApiError> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| fail("DIAGNOSTIC", "无法读取歌词样本"))?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| fail("DIAGNOSTIC", "读取歌词样本失败"))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(fail("LYRICS_LIMIT", "歌词样本超过限制"));
    }
    let data =
        serde_json::from_slice(&bytes).map_err(|_| fail("DIAGNOSTIC", "歌词样本不是有效 JSON"))?;
    let lyrics = crate::lyrics::parse_response(data, None)?;
    Ok(json!({"parsed":true,"networkRequestsSent":false,
        "originalLines":lyrics.Original.as_ref().map_or(0, |timeline| timeline.Lines.len()),
        "originalWords":lyrics.Original.as_ref().map_or(0, |timeline| timeline.Lines.iter().map(|line| line.Words.len()).sum::<usize>()),
        "translationLines":lyrics.Translation.as_ref().map_or(0, |timeline| timeline.Lines.len()),
        "slashPlaceholdersVisible":lyrics.Translation.as_ref().is_some_and(|timeline| timeline.Lines.iter().any(|line| line.Text.trim() == "//"))}))
}

pub fn capture_lyrics(title: &str, artist: &str, output: &Path) -> Result<Value, ApiError> {
    if title.trim().is_empty()
        || artist.trim().is_empty()
        || title.len() + artist.len() > 1024
        || output.exists()
    {
        return Err(fail("PARAMETERS", "歌曲名、歌手无效或样本目录已存在"));
    }
    let mut provider = Provider::persistent();
    provider.restore_once();
    if let Some(error) = &provider.storage_issue {
        return Err(error.clone());
    }
    if provider.client.is_none() {
        provider.client = Some(Client::new(Config::bundled())?);
    }
    let account_used = provider.session.is_some();
    let data = provider.request_until(
        SEARCH,
        &[
            ("w", format!("{title} {artist}")),
            ("p", "1".into()),
            ("t", "0".into()),
            ("num", "50".into()),
            ("source", "0".into()),
            ("exclude_types", "".into()),
            ("file_format", "0".into()),
            ("max_file_type", "50".into()),
            ("support_dolby", "0".into()),
        ],
        Instant::now() + Duration::from_secs(35),
    )?;
    let songs = data
        .get("list")
        .and_then(Value::as_array)
        .ok_or_else(|| fail("SCHEMA", "搜索未返回歌曲列表"))?;
    let selected = songs
        .iter()
        .filter_map(|value| track(value).ok())
        .find(|track| {
            track.title.contains(title) && track.artists.iter().any(|name| name.contains(artist))
        })
        .ok_or_else(|| fail("NOT_FOUND", "未匹配到指定歌曲与歌手，不自动捕获其他歌曲"))?;
    let parameters = crate::lyrics::request_parameters(&selected)?;
    let response = provider.request_until(
        crate::lyrics::COMMAND,
        &parameters,
        Instant::now() + Duration::from_secs(35),
    )?;
    let sample = sample_fields(&response);
    std::fs::create_dir_all(output).map_err(|_| fail("DIAGNOSTIC", "无法创建样本目录"))?;
    save(
        output,
        "response.json",
        &serde_json::to_vec_pretty(&sample)
            .map_err(|_| fail("DIAGNOSTIC", "无法序列化歌词样本"))?,
    )?;
    for (field, file) in [
        ("song_qrc_enc_lyric", "original.qrc"),
        ("song_translate_enc_lyric", "translation.txt"),
    ] {
        if let Some(encoded) = sample[field].as_str().filter(|value| !value.is_empty())
            && let Ok(text) = crate::qrc::decrypt_qrc_api(encoded)
        {
            save(output, file, text.as_bytes())?;
        }
    }
    let parsed = crate::lyrics::parse_response(sample, selected.duration_ms);
    Ok(
        json!({"title":selected.title,"artists":selected.artists,"id":selected.id,"mid":selected.mid,"durationMs":selected.duration_ms,
        "accountUsed":account_used,"sessionRefreshed":false,"sessionWritten":false,"responseFieldsAllowlisted":true,
        "parsed":parsed.is_ok(),"parseError":parsed.err()}),
    )
}

fn sample_fields(response: &Value) -> Value {
    let mut sample = serde_json::Map::new();
    for name in [
        "ret",
        "subRet",
        "sub_ret",
        "song_qrc_enc_lyric",
        "song_lyric",
        "song_translate_enc_lyric",
        "lyric_start_ms",
    ] {
        if let Some(value) = response.get(name) {
            sample.insert(name.into(), value.clone());
        }
    }
    Value::Object(sample)
}

fn save(output: &Path, name: &str, bytes: &[u8]) -> Result<(), ApiError> {
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output.join(name))
        .map_err(|_| fail("DIAGNOSTIC", "无法创建新的歌词样本文件"))?;
    file.write_all(bytes)
        .map_err(|_| fail("DIAGNOSTIC", "无法写入歌词样本"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_includes_only_lyrics_and_safe_status_fields() {
        let sample = sample_fields(
            &json!({"ret":0,"song_lyric":"[00:01]Synthetic","access_token":"private-secret","openid":"private-secret","cookie":"private-secret","signed_url":"private-secret"}),
        );
        assert_eq!(sample["song_lyric"], "[00:01]Synthetic");
        assert!(!sample.to_string().contains("private-secret"));
        assert_eq!(sample.as_object().unwrap().len(), 2);
    }
}
