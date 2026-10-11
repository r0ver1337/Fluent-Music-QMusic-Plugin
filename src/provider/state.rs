use super::*;

impl Provider {
    pub(super) fn restore_once(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;
        let Some(store) = &self.store else {
            return;
        };
        if !store.enabled() {
            return;
        }
        let loaded = (|| {
            if !store.exists()? {
                return Ok(None);
            }
            if self.client.is_none() {
                self.client = Some(Client::new(Config::bundled())?);
            }
            store.load(&self.client.as_ref().unwrap().config)
        })();
        match loaded {
            Ok(Some(saved)) => {
                self.client.as_mut().unwrap().config.device_id = saved.device_id;
                self.session = Some(saved.session);
                self.generation = Some(saved.generation);
            }
            Ok(None) => {}
            Err(error) => self.storage_issue = Some(error),
        }
    }
    pub(super) fn clear_private_state(&mut self) {
        self.session = None;
        self.pending = None;
        self.generation = None;
        self.persistence_dirty = false;
        self.tokens.clear();
        self.updates.clear();
        self.media.clear();
        self.public_playlists.clear();
        if let Some(client) = &mut self.recommendation_client {
            client.clear();
        }
    }
    pub(super) fn persist_session(&mut self) -> Result<(), ApiError> {
        if let Some(store) = &self.store
            && store.enabled()
        {
            self.generation = Some(store.save(
                &self.client.as_ref().unwrap().config,
                self.session.as_ref().unwrap(),
                self.generation.as_deref(),
            )?);
        } else if let Some(error) = &self.storage_issue {
            return Err(error.clone());
        }
        if self.store.as_ref().is_some_and(|s| !s.enabled()) {
            self.generation = None;
        }
        self.persistence_dirty = false;
        self.storage_issue = None;
        Ok(())
    }
    pub(super) fn ensure_session(&mut self) -> Result<(), ApiError> {
        self.ensure_session_until(Instant::now() + crate::plugin_config::TIMEOUT)
    }
    pub(super) fn ensure_session_until(&mut self, deadline: Instant) -> Result<(), ApiError> {
        if let (Some(store), Some(generation)) = (&self.store, &self.generation)
            && let Err(error) = store.check(&self.client.as_ref().unwrap().config, generation)
        {
            if error.code == "SESSION_REVOKED" {
                self.clear_private_state();
            }
            return Err(error);
        }
        if self.persistence_dirty {
            self.persist_session()?;
        }
        let c = self.client.as_ref().unwrap();
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| fail("LOGIN_REQUIRED", "请先通过 QQ 插件扫码登录"))?;
        if session.expires > c.time() + 60 {
            return Ok(());
        }
        let data = self.client.as_mut().unwrap().request_until(
            TOKEN,
            &[
                ("cmd", "refreshToken".into()),
                ("qqmusic_refresh_token", session.refresh.clone()),
            ],
            Some(session),
            &["login_type"],
            false,
            deadline,
        )?;
        let data = success(data)?;
        let c = self.client.as_ref().unwrap();
        let mut next = decrypt(field(&data, "encryptString")?, &c.config.key, c.time())?;
        if next.open_id != session.open_id {
            return Err(fail("SESSION", "刷新账号不一致"));
        }
        // Only a refresh for the SAME openId may backfill an omitted/empty musicId.
        // Fresh QR login never inherits the previous account's identity.
        if next.music_id.as_deref().is_none_or(str::is_empty)
            && let Some(previous) = &session.music_id
        {
            next.music_id = Some(previous.clone());
        }
        self.session = Some(next);
        self.persistence_dirty = true;
        self.persist_session()?;
        Ok(())
    }
}
