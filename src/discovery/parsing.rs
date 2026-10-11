use super::*;

pub(super) fn check_code(data: &Value) -> Result<(), ApiError> {
    let code = data
        .get("code")
        .and_then(Value::as_i64)
        .ok_or_else(|| fail("SCHEMA", "公开音乐资料缺少整数 code"))?;
    let sub = data
        .get("subcode")
        .map(Value::as_i64)
        .unwrap_or(Some(0))
        .ok_or_else(|| fail("SCHEMA", "公开音乐资料 subcode 无效"))?;
    if code != 0 || sub != 0 {
        return Err(ApiError::new(
            "SERVER",
            format!("公开音乐资料被拒绝：code={code}, subcode={sub}"),
        ));
    }
    Ok(())
}

pub(super) fn parse_plaza(data: &Value) -> Result<Vec<Playlist>, ApiError> {
    check_code(data)?;
    let list = data["data"]["list"]
        .as_array()
        .ok_or_else(|| fail("SCHEMA", "公开推荐缺少 data.list"))?;
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for item in list.iter().take(100) {
        let mut p = playlist(item)?;
        validate_public_id(&p.id)?;
        p.cover_url = cover_url(item["imgurl"].as_str());
        p.subtitle = item["introduction"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(clean);
        if seen.insert(p.id.clone()) {
            items.push(p);
        }
    }
    Ok(items)
}

pub(super) fn validate_public_id(id: &str) -> Result<u64, ApiError> {
    validate_id(id)?;
    id.parse::<u64>()
        .ok()
        .filter(|id| *id != 0)
        .ok_or_else(|| fail("PARAMETERS", "公开歌单 ID 必须为非零十进制整数"))
}
pub(super) fn detail_request(id: &str, page: u32) -> Result<Value, ApiError> {
    let id = validate_public_id(id)?;
    let begin = page
        .checked_mul(PAGE_SIZE)
        .filter(|_| page <= 10000)
        .ok_or_else(|| fail("PARAMETERS", "歌单页码无效"))?;
    Ok(json!({"comm":{"ct":24,"cv":0},"req_0":{
        "module":"music.srfDissInfo.aiDissInfo","method":"uniform_get_Dissinfo",
        "param":{"disstid":id,"enc_hostuin":"","tag":1,"userinfo":1,
            "song_begin":begin,"song_num":PAGE_SIZE}
    }}))
}
pub(super) fn parse_detail(data: &Value, id: &str, page: u32) -> Result<Page<Track>, ApiError> {
    check_code(data)?;
    check_code(&data["req_0"])?;
    let data = &data["req_0"]["data"];
    check_code(data)?;
    if unsigned(&data["dirinfo"]["id"]) != Some(validate_public_id(id)?) {
        return Err(fail("SCHEMA", "公开资料返回了不同的歌单"));
    }
    let list = data["songlist"]
        .as_array()
        .ok_or_else(|| fail("SCHEMA", "公开歌单缺少 songlist"))?;
    if list.len() > PAGE_SIZE as usize {
        return Err(fail("SCHEMA", "公开歌单超出请求页大小"));
    }
    let items = list.iter().map(track).collect::<Result<Vec<_>, _>>()?;
    let consumed = u64::from(page) * u64::from(PAGE_SIZE) + items.len() as u64;
    let has_more = !items.is_empty()
        && data["hasmore"].as_bool().unwrap_or_else(|| {
            unsigned(&data["hasmore"])
                .map(|v| v > 0)
                .unwrap_or_else(|| unsigned(&data["total_song_num"]).is_some_and(|n| consumed < n))
        });
    Ok(Page {
        items,
        page,
        has_more,
        cursor: None,
    })
}

/// Never turn an arbitrary server field into a file URL, authenticated URL or non-QQ fetch.
pub fn cover_url(value: Option<&str>) -> Option<String> {
    let mut url = url::Url::parse(value?).ok()?;
    let host = url.host_str()?;
    if !matches!(url.scheme(), "https" | "http")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !(host.ends_with(".qq.com") || host.ends_with(".gtimg.cn") || host.ends_with(".qpic.cn"))
    {
        return None;
    }
    url.set_scheme("https").ok()?;
    url.set_fragment(None);
    Some(url.into())
}

/// Only RcItem has a statically established playlist ID. Card.type's numeric mapping
/// is unknown: don't misroute albums/radios as dissids. Embedded songInfo is safe metadata.
pub fn opi_feed(data: &Value) -> Result<RecommendationFeed, ApiError> {
    let list = match data.get("rc_item_arr") {
        None | Some(Value::Null) if data["shelf_arr"].is_array() => &[][..],
        Some(Value::Array(list)) => list.as_slice(),
        _ => return Err(fail("SCHEMA", "首页推荐缺少 rc_item_arr/shelf_arr")),
    };
    let mut feed = RecommendationFeed::default();
    let mut seen = HashSet::new();
    let mut rejected = 0;
    for item in list.iter().take(100) {
        let parsed = playlist(&json!({"dissid":item["dissid"],"dissname":item["dirName"]}))
            .and_then(|p| {
                validate_public_id(&p.id)?;
                Ok(p)
            });
        let Ok(mut p) = parsed else {
            rejected += 1;
            continue;
        };
        p.cover_url = cover_url(item["albumPicUrl"].as_str())
            .or_else(|| cover_url(item["big_cover"].as_str()));
        if seen.insert(p.id.clone()) {
            feed.playlists.push(p);
        }
    }
    if let Some(shelves) = data["shelf_arr"].as_array() {
        for shelf in shelves.iter().take(20) {
            if let Some(cards) = shelf["card_arr"].as_array() {
                for card in cards.iter().take(100) {
                    if feed.tracks.len() >= 100 {
                        break;
                    }
                    if card["songInfo"].is_object() {
                        match track(&card["songInfo"]) {
                            Ok(track) => feed.tracks.push(track),
                            Err(_) => rejected += 1,
                        }
                    }
                }
            }
        }
    }
    if rejected > 0 {
        feed.issues.push(RecommendationIssue {
            stage: "account_playlists".into(),
            error: ApiError::new("SCHEMA", format!("跳过 {rejected} 个无效首页推荐条目")),
        });
        if feed.playlists.is_empty() {
            return Err(fail("SCHEMA", "首页推荐没有可识别的有效歌单条目"));
        }
    }
    // This is homepage content, not proven to be personalized merely because a session exists.
    Ok(feed)
}
