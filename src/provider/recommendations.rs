use super::*;

impl Provider {
    pub(super) fn public_client(&mut self) -> Result<&discovery::PublicClient, ApiError> {
        if self.public.is_none() {
            self.public = Some(discovery::PublicClient::new()?);
        }
        Ok(self.public.as_ref().unwrap())
    }
    pub(super) fn public_feed(&mut self) -> Result<RecommendationFeed, ApiError> {
        let mut feed = self.public_client()?.feed()?;
        if let Some(error) = &self.storage_issue {
            feed.issues.push(RecommendationIssue {
                stage: "session_storage".into(),
                error: error.clone(),
            });
        }
        self.remember_public(&feed);
        Ok(feed)
    }
    pub(super) fn remember_public(&mut self, feed: &RecommendationFeed) {
        if self.public_playlists.len() > 1000 {
            self.public_playlists.clear();
        }
        self.public_playlists
            .extend(feed.playlists.iter().map(|p| p.id.clone()));
    }
    pub(super) fn recommendation_feed(&mut self) -> Result<RecommendationFeed, ApiError> {
        let deadline = Instant::now() + Duration::from_secs(75);
        if self.session.is_none() {
            return self.public_feed();
        }
        let mut issues = Vec::new();
        // Authentication and all fallback branches share the same host-safe deadline.
        let refresh =
            self.ensure_session_until(deadline.min(Instant::now() + Duration::from_secs(15)));
        if let Err(error) = refresh {
            issues.push(RecommendationIssue {
                stage: "session".into(),
                error,
            });
            let mut feed = self.public_fallback(deadline, issues);
            feed.notice = Some("账号刷新失败，当前不是账号推荐；请重新授权。".into());
            return finish_feed(feed);
        }
        // Match OpenQQMusic's concrete GET homepage path, not the separate
        // GetAlgorithmPlConf/aiPlayList or native Car V3 protocols.
        let homepage =
            self.account_playlist_feed(deadline.min(Instant::now() + Duration::from_secs(20)));
        let account_feed = match homepage {
            Ok(feed) => Some(feed),
            Err(error) => {
                issues.push(RecommendationIssue {
                    stage: "account_playlists".into(),
                    error,
                });
                None
            }
        };
        let mut feed = match account_feed {
            Some(mut feed) => {
                // A new account result must not accidentally use an old public-route entry.
                for playlist in &feed.playlists {
                    self.public_playlists.remove(&playlist.id);
                }
                issues.append(&mut feed.issues);
                feed.issues = issues;
                feed
            }
            None => self.public_fallback(deadline, issues),
        };
        let scene = self.client.as_ref().unwrap().config.scene;
        let songs = self
            .request_until(
                RECOMMEND,
                &[("scene", scene.to_string())],
                deadline.min(Instant::now() + Duration::from_secs(15)),
            )
            .and_then(|data| self.track_page(&data, "songlist", 0));
        match songs {
            Ok(ResultData::Tracks(page)) if !page.items.is_empty() => {
                feed.tracks = page.items;
                feed.personalized = true;
                feed.track_source = RecommendationSource::Account;
            }
            Ok(_) => feed.issues.push(RecommendationIssue {
                stage: "account_tracks".into(),
                error: fail("EMPTY", "账号个性歌曲接口返回空列表"),
            }),
            Err(error) => feed.issues.push(RecommendationIssue {
                stage: "account_tracks".into(),
                error,
            }),
        }
        let playlist_label = match feed.playlist_source {
            RecommendationSource::Account => {
                "当前账号的 OPI 首页推荐（OpenQQMusic 路径，由服务端决定内容）"
            }
            RecommendationSource::Homepage => "SDK 首页歌单（未确认个性化）",
            RecommendationSource::Public
                if feed.issues.iter().any(|issue| {
                    issue.stage == "account_playlists"
                        && issue.error.code == "APP_PERMISSION_DENIED"
                }) =>
            {
                "公开歌单（该 OPI 首页请求被应用权限拒绝，非个性化）"
            }
            RecommendationSource::Public => "公开歌单（账号首页歌单失败后回退，不是个性化）",
            RecommendationSource::Unknown => "歌单暂未取得",
        };
        let track_label = match feed.track_source {
            RecommendationSource::Account => "账号个性歌曲",
            RecommendationSource::Homepage => "SDK 首页歌曲",
            RecommendationSource::Public => "公开歌单中的歌曲",
            RecommendationSource::Unknown => "歌曲暂未取得",
        };
        feed.notice = Some(format!("歌单：{playlist_label}；歌曲：{track_label}。"));
        finish_feed(feed)
    }
    pub(super) fn account_playlist_feed(
        &mut self,
        deadline: Instant,
    ) -> Result<RecommendationFeed, ApiError> {
        if let Some((until, error)) = &self.homepage_denial
            && Instant::now() < *until
        {
            return Err(error.clone());
        }
        self.homepage_denial = None;
        let result = self
            .client
            .as_mut()
            .unwrap()
            .homepage_playlists(self.session.as_ref(), deadline)
            .and_then(|data| discovery::opi_feed(&data))
            .and_then(|mut feed| {
                if feed.playlists.is_empty() {
                    return Err(fail(
                        "EMPTY",
                        "OPI 首页没有可识别的推荐歌单；未将歌曲、专辑或未知 Card 当作歌单",
                    ));
                }
                feed.playlist_source = RecommendationSource::Account;
                if !feed.tracks.is_empty() {
                    feed.track_source = RecommendationSource::Homepage;
                }
                Ok(feed)
            });
        if let Err(error) = &result
            && error.code == "APP_PERMISSION_DENIED"
        {
            // Do not spam the denied method on every page/QR refresh. Other authorized
            // methods still run. A later refresh after 60s can discover a newly granted app.
            self.homepage_denial =
                Some((Instant::now() + HOMEPAGE_PERMISSION_COOLDOWN, error.clone()));
        }
        result
    }
    pub(super) fn public_fallback(
        &mut self,
        deadline: Instant,
        mut issues: Vec<RecommendationIssue>,
    ) -> RecommendationFeed {
        match self
            .public_client()
            .and_then(|client| client.feed_until(deadline))
        {
            Ok(mut feed) => {
                self.remember_public(&feed);
                issues.append(&mut feed.issues);
                feed.issues = issues;
                feed
            }
            Err(error) => {
                issues.push(RecommendationIssue {
                    stage: "public_playlists".into(),
                    error,
                });
                RecommendationFeed {
                    issues,
                    ..Default::default()
                }
            }
        }
    }
}

pub(super) fn finish_feed(feed: RecommendationFeed) -> Result<RecommendationFeed, ApiError> {
    if feed.playlists.is_empty() && feed.tracks.is_empty() {
        let reasons = feed
            .issues
            .iter()
            .map(|issue| {
                format!(
                    "{} [{}]: {}",
                    issue.stage, issue.error.code, issue.error.message
                )
            })
            .collect::<Vec<_>>()
            .join("；");
        return Err(ApiError::new(
            "RECOMMENDATIONS",
            format!("未取得推荐内容。{reasons}"),
        ));
    }
    Ok(feed)
}
