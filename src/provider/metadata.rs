use super::*;

pub(crate) fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(2000).collect()
}
pub(crate) fn validate_id(id: &str) -> Result<(), ApiError> {
    if id.is_empty() || id.len() > 128 || id.contains(',') || id.chars().any(char::is_whitespace) {
        Err(fail("PARAMETERS", "歌曲/歌单标识无效"))
    } else {
        Ok(())
    }
}
fn alias<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| v.get(k).filter(|v| !v.is_null()))
}
fn identity(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::to_owned)
        .or_else(|| v.as_u64().map(|i| i.to_string()))
}
pub(crate) fn playlist(v: &Value) -> Result<Playlist, ApiError> {
    let id = alias(v, &["dissid", "diss_id", "playlistId"])
        .and_then(identity)
        .ok_or_else(|| fail("SCHEMA", "歌单缺少 ID"))?;
    validate_id(&id)?;
    let title = alias(v, &["dissname", "diss_name", "diss_title", "name"])
        .and_then(Value::as_str)
        .map(clean)
        .unwrap_or_else(|| id.clone());
    Ok(Playlist {
        id,
        title,
        track_count: alias(v, &["song_num", "songnum", "total_num"]).and_then(unsigned),
        cover_url: discovery::cover_url(
            alias(v, &["pic_url", "diss_pic", "logo", "pic"]).and_then(Value::as_str),
        ),
        subtitle: alias(v, &["introduction", "diss_desc", "desc"])
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(clean),
    })
}
pub(super) fn library_playlists(data: &Value) -> Result<Vec<Playlist>, ApiError> {
    let array = data["data"]
        .as_array()
        .ok_or_else(|| fail("SCHEMA", "歌单响应缺少 data 数组"))?;
    // The synthetic liked-songs directory is not a regular library playlist.
    // Keep direct detail support and ordinary playlists with any display name.
    array
        .iter()
        .filter(|v| {
            alias(v, &["dissid", "diss_id", "playlistId"])
                .and_then(identity)
                .as_deref()
                != Some("0")
        })
        .map(playlist)
        .collect()
}
fn artist(v: &Value) -> Option<String> {
    v.as_str()
        .or_else(|| v["name"].as_str())
        .or_else(|| v["singer_name"].as_str())
        .map(|name| clean(name).trim().to_owned())
        .filter(|name| !name.is_empty())
}
fn artists(v: &Value) -> Vec<String> {
    let mut artists: Vec<_> = v["singer"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(artist)
        .collect();
    if artists.is_empty()
        && let Some(name) = artist(&v["singer_name"])
    {
        artists.push(name);
    }
    artists.extend(
        v["other_singer_list"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(artist),
    );
    let mut seen = HashSet::new();
    artists.retain(|name| seen.insert(name.clone()));
    artists
}
fn duration(v: &Value) -> Option<u64> {
    ["interval", "song_play_time"].iter().find_map(|key| {
        unsigned(&v[key])
            .filter(|seconds| *seconds > 0)
            .and_then(|seconds| seconds.checked_mul(1000))
    })
}
pub fn track(v: &Value) -> Result<Track, ApiError> {
    let (cover_url, thumbnail_url) = crate::artwork::song_cover(v);
    let mid = alias(v, &["song_mid", "mid"])
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let id = alias(v, &["song_id", "songId", "id", "trackId"])
        .and_then(identity)
        .or_else(|| mid.as_ref().map(|m| format!("mid:{m}")))
        .ok_or_else(|| fail("SCHEMA", "歌曲缺少 ID/mid"))?;
    validate_id(&id)?;
    let title = alias(v, &["song_name", "title", "song_title", "name"])
        .and_then(Value::as_str)
        .ok_or_else(|| fail("SCHEMA", "歌曲缺少名称"))?;
    Ok(Track {
        id,
        mid,
        title: clean(title),
        artists: artists(v),
        album: alias(v, &["album_name", "album_title"])
            .and_then(Value::as_str)
            .map(clean)
            .or_else(|| v["album"]["name"].as_str().map(clean))
            .unwrap_or_default(),
        duration_ms: duration(v),
        provider_id: "qqmusic".into(),
        cover_url,
        thumbnail_url,
    })
}
