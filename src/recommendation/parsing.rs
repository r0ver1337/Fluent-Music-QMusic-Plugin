use super::*;

pub(super) fn safe_cover(value: Option<&str>) -> Option<String> {
    let value = cover_url(value)?;
    let mut u = url::Url::parse(&value).ok()?;
    u.set_query(None);
    Some(u.into())
}
pub(super) fn id(value: &Value) -> Option<String> {
    let value = value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|n| n.to_string()))?;
    if !value.is_empty()
        && value.len() <= 20
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.parse::<u64>().is_ok_and(|n| n != 0)
    {
        Some(value)
    } else {
        None
    }
}
pub(super) fn recommended_playlists(data: &Value) -> Result<Vec<Playlist>, ApiError> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for name in ["HeadRsp", "FeedRsp"] {
        let list = data[name]["List"]
            .as_array()
            .ok_or_else(|| fail("SCHEMA", "Flyme 歌单响应缺少列表"))?;
        for item in list.iter().take(50) {
            let b = &item["Playlist"]["basic"];
            let Some(id) = id(&b["tid"]) else {
                continue;
            };
            let Some(title) = b["title"].as_str().filter(|v| !v.is_empty()) else {
                continue;
            };
            if seen.insert(id.clone()) {
                out.push(Playlist {
                    id,
                    title: clean(title),
                    track_count: unsigned(&b["song_cnt"]),
                    cover_url: safe_cover(b["cover"]["medium_url"].as_str())
                        .or_else(|| safe_cover(b["cover"]["big_url"].as_str())),
                    subtitle: b["desc"].as_str().filter(|s| !s.is_empty()).map(clean),
                });
            }
        }
    }
    if out.is_empty() {
        return Err(fail("EMPTY", "Flyme 未返回有效歌单"));
    }
    Ok(out)
}
pub(super) fn homepage_tracks(data: &Value) -> Result<Vec<Track>, ApiError> {
    check_status(data, true)?;
    let shelves = data["v_shelf"]
        .as_array()
        .ok_or_else(|| fail("SCHEMA", "Flyme 首页缺少栏目数组"))?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for s in shelves.iter().take(32) {
        if let Some(niches) = s["v_niche"].as_array() {
            for n in niches.iter().take(32) {
                if let Some(cards) = n["v_card"].as_array() {
                    for c in cards.iter().take(100) {
                        // Only the verified song-card mapping. Album/navigation cards are NOT playlists.
                        if c["jumptype"].as_i64() != Some(10046) {
                            continue;
                        }
                        let Some(id) = id(&c["id"]) else {
                            continue;
                        };
                        let Some(title) = c["title"].as_str().filter(|s| !s.is_empty()) else {
                            continue;
                        };
                        if seen.insert(id.clone()) && out.len() < 100 {
                            let (cover_url, thumbnail_url) = crate::artwork::song_cover(c);
                            out.push(Track {
                                id,
                                title: clean(title),
                                artists: c["subtitle"]
                                    .as_str()
                                    .filter(|s| !s.is_empty())
                                    .map(|s| vec![clean(s)])
                                    .unwrap_or_default(),
                                album: String::new(),
                                duration_ms: unsigned(&c["duration"])
                                    .filter(|n| *n > 0 && *n <= 86400)
                                    .and_then(|n| n.checked_mul(1000)),
                                provider_id: "qqmusic".into(),
                                mid: None,
                                cover_url,
                                thumbnail_url,
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}
