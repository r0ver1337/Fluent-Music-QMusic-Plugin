use super::*;

impl Provider {
    pub(super) fn signed_recommendation_feed(&mut self) -> Result<RecommendationFeed, ApiError> {
        let deadline = Instant::now() + Duration::from_secs(75);
        let mut auth_issues = Vec::new();
        let result = (|| {
            if self.recommendation_client.is_none() {
                let device = self
                    .client
                    .as_ref()
                    .map(|c| c.config.device_id.clone())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
                self.recommendation_client =
                    Some(crate::recommendation::RecommendationClient::new(device)?);
            }
            if self.session.is_some() {
                let authorization = self
                    .ensure_session_until(deadline.min(Instant::now() + Duration::from_secs(15)))
                    .and_then(|_| {
                        let c = self.client.as_ref().unwrap();
                        self.recommendation_client
                            .as_mut()
                            .unwrap()
                            .set_open_id_account(&c.config.app_id, self.session.as_ref().unwrap())
                    })
                    .and_then(|_| {
                        self.recommendation_client
                            .as_mut()
                            .unwrap()
                            .verify_open_id_account(
                                deadline.min(Instant::now() + Duration::from_secs(25)),
                            )
                            .map(|_| ())
                    });
                if let Err(error) = authorization {
                    self.recommendation_client.as_mut().unwrap().clear_account();
                    auth_issues.push(RecommendationIssue {
                        stage: "flyme_account".into(),
                        error,
                    });
                }
            } else {
                self.recommendation_client.as_mut().unwrap().clear_account();
            }
            self.recommendation_client
                .as_mut()
                .unwrap()
                .feed(deadline.min(Instant::now() + Duration::from_secs(45)))
        })();
        let mut feed = match result {
            Ok(feed) => feed,
            Err(error) => {
                if let Some(client) = &mut self.recommendation_client {
                    client.clear();
                }
                let mut fallback = self.public_fallback(
                    deadline,
                    vec![RecommendationIssue {
                        stage: "flyme_homepage".into(),
                        error,
                    }],
                );
                fallback.notice =
                    Some("Flyme 首页请求失败，当前是公开歌单回退，不是 Flyme 账号推荐。".into());
                return finish_feed(fallback);
            }
        };
        // Public dissids use the existing guest-safe metadata/detail route.
        feed.issues.extend(auth_issues);
        self.remember_public(&feed);
        if let Some(error) = &self.storage_issue {
            feed.issues.push(RecommendationIssue {
                stage: "session_storage".into(),
                error: error.clone(),
            });
        }
        if self.session.is_some()
            && !(feed.track_source == RecommendationSource::Account && !feed.tracks.is_empty())
        {
            let songs = self
                .ensure_session_until(deadline.min(Instant::now() + Duration::from_secs(15)))
                .and_then(|_| {
                    let scene = self.client.as_ref().unwrap().config.scene;
                    self.request_until(
                        RECOMMEND,
                        &[("scene", scene.to_string())],
                        deadline.min(Instant::now() + Duration::from_secs(15)),
                    )
                })
                .and_then(|data| self.track_page(&data, "songlist", 0));
            match songs {
                Ok(ResultData::Tracks(page)) if !page.items.is_empty() => {
                    feed.tracks = page.items;
                    feed.personalized = true;
                    feed.track_source = RecommendationSource::Account;
                    feed.notice = Some(if feed.playlist_source == RecommendationSource::Account {
                        "歌单：当前账号的 Flyme 推荐（已验证 OPEN_ID 登录态，服务端决定内容）；歌曲：当前账号个性歌曲（OPI）。不依赖 HAR。"
                    } else {
                        "歌单：Flyme 游客推荐（账号路径未验证）；歌曲：当前账号个性歌曲（OPI）。不依赖 HAR。"
                    }.into());
                }
                Ok(_) => feed.issues.push(RecommendationIssue {
                    stage: "account_tracks".into(),
                    error: fail("EMPTY", "账号个性歌曲为空，保留 Flyme 首页歌曲"),
                }),
                Err(error) => feed.issues.push(RecommendationIssue {
                    stage: "account_tracks".into(),
                    error,
                }),
            }
        }
        finish_feed(feed)
    }
}
