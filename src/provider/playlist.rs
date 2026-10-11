use super::*;

impl Provider {
    pub(super) fn playlist_details(
        &mut self,
        id: String,
        page: u32,
        cursor: Option<String>,
    ) -> Result<ResultData, ApiError> {
        self.ensure_session()?;
        validate_id(&id)?;
        let liked = id == "0";
        if liked && page > 0 && cursor.as_deref().is_none_or(str::is_empty) {
            return Err(fail("PARAMETERS", "我喜欢下一页需要服务器游标"));
        }
        // Ordinary playlists use page numbers, not incremental version reads.
        // Only liked songs continue with the server's version/cursor pair.
        let last_update = if liked && page > 0 {
            self.updates.get(&id).copied().unwrap_or(0)
        } else {
            0
        };
        let passback = if liked && page > 0 {
            cursor.unwrap_or_default()
        } else {
            String::new()
        };
        let data = self.request(
            DETAIL,
            &[
                ("dissid", id.clone()),
                ("page", if liked { "0".into() } else { page.to_string() }),
                ("page_size", "50".into()),
                ("source", "0".into()),
                ("pagemode", if liked { "1".into() } else { "0".into() }),
                ("passback", passback),
                ("lastUpdateTime", last_update.to_string()),
            ],
        )?;
        let result = self.track_page(&data, "song_list", page)?;
        if page == 0 || !liked {
            self.updates.remove(&id);
        }
        if liked && let Some(update) = unsigned(&data["lastUpdateTime"]) {
            self.updates.insert(id, update);
        }
        Ok(result)
    }
}
