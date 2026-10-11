use super::*;

impl Provider {
    pub(super) fn track_page(
        &mut self,
        data: &Value,
        key: &str,
        page: u32,
    ) -> Result<ResultData, ApiError> {
        let list = data[key]
            .as_array()
            .ok_or_else(|| ApiError::new("SCHEMA", format!("响应缺少 {key} 数组")))?;
        let mut items = vec![];
        for item in list {
            let track = track(item)?;
            if let Some(token) = item["song_token"].as_str() {
                self.tokens.insert(track.id.clone(), token.into());
            }
            items.push(track);
        }
        // Avoid retaining an unbounded account music library in plugin memory.
        if self.tokens.len() > 10000 {
            self.tokens.clear();
        }
        let has_more = data["has_more"]
            .as_bool()
            .unwrap_or_else(|| unsigned(&data["has_more"]).is_some_and(|v| v > 0));
        let cursor = data["passback"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        Ok(ResultData::Tracks(Page {
            items,
            page,
            has_more,
            cursor,
        }))
    }
    pub(super) fn resolve(&mut self, item: Track) -> Result<ResultData, ApiError> {
        let deadline = Instant::now() + Duration::from_secs(80);
        let song = self.song_info(&item, deadline)?;
        Ok(ResultData::Media(self.media.source_until(&song, deadline)?))
    }

    pub(super) fn song_info(&mut self, item: &Track, deadline: Instant) -> Result<Value, ApiError> {
        self.ensure_session()?;
        if item.provider_id != "qqmusic" {
            return Err(fail("PROVIDER", "歌曲不属于 QQ 插件"));
        }
        validate_id(&item.id)?;
        let token = self.tokens.get(&item.id).cloned().unwrap_or_default();
        let mid = item.mid.clone().unwrap_or_default();
        if !mid.is_empty() {
            validate_id(&mid)?;
        }
        let data = self.request_until(
            RESOLVE,
            &[
                (
                    "song_id",
                    if item.id.starts_with("mid:") {
                        "".into()
                    } else {
                        item.id.clone()
                    },
                ),
                ("song_mid", mid.clone()),
                ("song_token", token),
                ("player_status", "0".into()),
                ("no_url", "0".into()),
            ],
            deadline,
        )?;
        let list = data["songlist"]
            .as_array()
            .ok_or_else(|| fail("SCHEMA", "歌曲解析响应缺少 songlist"))?;
        let song = list
            .iter()
            .find(|value| {
                track(value).is_ok_and(|t| {
                    t.id == item.id
                        || (item.id.starts_with("mid:") && t.mid.as_deref() == Some(mid.as_str()))
                })
            })
            .ok_or_else(|| fail("SCHEMA", "解析返回了不同的歌曲"))?;
        Ok(song.clone())
    }
}
