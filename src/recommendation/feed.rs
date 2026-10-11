use super::*;

impl RecommendationClient {
    pub fn feed(&mut self, deadline: Instant) -> Result<RecommendationFeed, ApiError> {
        if self.account.is_some()
            && !self
                .account_verified_until
                .is_some_and(|until| Instant::now() < until)
        {
            return Err(fail("SESSION", "Flyme OpenID 账号尚未验证"));
        }
        self.session(deadline)?;
        let mut feed = RecommendationFeed::default();
        match self
            .request(
                HOMEPAGE_MODULE,
                "GetRecommendFullPage",
                json!({"closed_personality":false}),
                false,
                deadline,
            )
            .and_then(|v| homepage_tracks(&v))
        {
            Ok(tracks) => feed.tracks = tracks,
            Err(error) => feed.issues.push(RecommendationIssue {
                stage: "flyme_homepage".into(),
                error,
            }),
        }
        match self
            .request(
                PLAYLIST_MODULE,
                "GetRecommendWhole",
                json!({"IsReqHead":true,"IsReqFeed":true,"FeedReq":{"From":0,"Size":12}}),
                false,
                deadline,
            )
            .and_then(|v| recommended_playlists(&v))
        {
            Ok(playlists) => feed.playlists = playlists,
            Err(error) => feed.issues.push(RecommendationIssue {
                stage: "flyme_playlists".into(),
                error,
            }),
        }
        if feed.playlists.is_empty() && feed.tracks.is_empty() {
            return Err(feed
                .issues
                .first()
                .map(|i| i.error.clone())
                .unwrap_or_else(|| fail("EMPTY", "Flyme 首页和歌单推荐均为空")));
        }
        if self.account.is_some() {
            feed.playlist_source = RecommendationSource::Account;
            feed.track_source = RecommendationSource::Account;
            feed.personalized = !feed.tracks.is_empty();
            feed.notice = Some(
                "当前授权账号的 Flyme 首页与歌单推荐（OPEN_ID 路径；内容由服务端决定，不保证与游客不同）。不依赖 HAR。".into(),
            );
        } else {
            feed.playlist_source = RecommendationSource::Homepage;
            feed.track_source = RecommendationSource::Homepage;
            feed.notice = Some(
                "Flyme 首页与歌单广场（独立游客请求，不依赖 HAR；不是账号个性化）。播放仍需原 QQ 授权。".into(),
            );
        }
        Ok(feed)
    }
}
