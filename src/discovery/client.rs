use super::*;

pub struct PublicClient {
    http: crate::http::Client,
    plaza: String,
    module: String,
}
impl PublicClient {
    pub fn new() -> Result<Self, ApiError> {
        Ok(Self {
            plaza: PLAZA.into(),
            module: MODULE.into(),
            http: crate::http::Client::builder()
                .timeout(Duration::from_secs(8))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| fail("NETWORK", "公开推荐客户端初始化失败"))?,
        })
    }
    fn read(
        &self,
        request: crate::http::RequestBuilder,
        deadline: Instant,
    ) -> Result<Value, ApiError> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(fail("TIMEOUT", "公开推荐请求超时"));
        }
        let response = request
            .timeout(remaining.min(Duration::from_secs(8)))
            .header("Referer", "https://y.qq.com/")
            .header("User-Agent", "FluentMusic/0.1")
            .send()
            .map_err(|_| fail("NETWORK", "QQ 公开音乐资料请求失败"))?;
        if !response.status().is_success() {
            return Err(ApiError::new(
                "HTTP",
                format!("QQ 公开资料 HTTP 状态 {}", response.status().as_u16()),
            ));
        }
        let mut bytes = Vec::new();
        response
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| fail("NETWORK", "公开音乐资料读取失败"))?;
        if bytes.len() as u64 > LIMIT {
            return Err(fail("SCHEMA", "公开音乐资料超过 1 MiB 限制"));
        }
        let value = serde_json::from_slice(&bytes)
            .map_err(|_| fail("SCHEMA", "公开音乐资料不是 UTF-8 JSON"))?;
        check_code(&value)?;
        Ok(value)
    }
    #[cfg(test)]
    pub fn playlists(&self) -> Result<Vec<Playlist>, ApiError> {
        self.playlists_until(Instant::now() + Duration::from_secs(8))
    }
    fn playlists_until(&self, deadline: Instant) -> Result<Vec<Playlist>, ApiError> {
        let data = self.read(
            self.http.get(&self.plaza).query(&[
                ("platform", "yqq.json"),
                ("needNewCode", "0"),
                ("sin", "0"),
                ("ein", "11"),
                ("categoryId", "10000000"),
                ("sortId", "5"),
                ("format", "json"),
                // This CGI needs the hyphenated encoding name; "utf8" can return GBK.
                ("inCharset", "utf-8"),
                ("outCharset", "utf-8"),
                ("hostUin", "0"),
            ]),
            deadline,
        )?;
        parse_plaza(&data)
    }
    pub fn playlist(&self, id: &str, page: u32) -> Result<Page<Track>, ApiError> {
        self.playlist_until(id, page, Instant::now() + Duration::from_secs(8))
    }
    fn playlist_until(
        &self,
        id: &str,
        page: u32,
        deadline: Instant,
    ) -> Result<Page<Track>, ApiError> {
        let body = detail_request(id, page)?;
        let data = self.read(self.http.post(&self.module).json(&body), deadline)?;
        parse_detail(&data, id, page)
    }
    pub fn feed(&self) -> Result<RecommendationFeed, ApiError> {
        self.feed_until(Instant::now() + Duration::from_secs(17))
    }
    pub fn feed_until(&self, deadline: Instant) -> Result<RecommendationFeed, ApiError> {
        let playlists = self.playlists_until(deadline)?;
        let mut issues = Vec::new();
        let mut notice =
            "公开歌单推荐；歌曲选自推荐歌单。登录后可获取个性歌曲推荐，播放仍须授权。".to_owned();
        let tracks = match playlists.first() {
            Some(p) => match self.playlist_until(&p.id, 0, deadline) {
                Ok(page) => page.items.into_iter().take(12).collect(),
                Err(error) => {
                    issues.push(RecommendationIssue {
                        stage: "public_tracks".into(),
                        error,
                    });
                    notice.push_str(" 歌曲暂时加载失败，可点击歌单重试。");
                    vec![]
                }
            },
            None => vec![],
        };
        Ok(RecommendationFeed {
            playlists,
            tracks,
            personalized: false,
            notice: Some(notice),
            playlist_source: RecommendationSource::Public,
            track_source: RecommendationSource::Public,
            issues,
        })
    }
    #[cfg(test)]
    pub fn test_client(base: &str) -> Self {
        assert!(base.starts_with("http://127.0.0.1:"));
        Self {
            http: crate::http::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            plaza: format!("{base}/plaza"),
            module: format!("{base}/module"),
        }
    }
}
