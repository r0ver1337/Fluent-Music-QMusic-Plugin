use super::*;

impl Provider {
    pub(super) fn account(&mut self) -> Result<Account, ApiError> {
        let data = self.request(ACCOUNT, &[])?;
        account_profile(&data)
    }
    pub(super) fn login_begin(&mut self) -> Result<ResultData, ApiError> {
        if self.store.is_none()
            && let Some(error) = &self.storage_issue
        {
            return Err(error.clone());
        }
        self.pending = None;
        let c = self.client.as_mut().unwrap();
        let data = success(c.request(
            QR,
            &[
                ("qqmusic_package_name", c.config.package.clone()),
                ("qqmusic_dev_name", c.config.device_name.clone()),
                ("qqmusic_qrcode_type", "universal".into()),
                (
                    "qqmusic_encrypt_auth",
                    json!({"response_type":"code", "callbackUrl":"https://y.qq.com"}).to_string(),
                ),
            ],
            None,
            &[],
            false,
        )?)?;
        let code = field(&data, "auth_code")?.into();
        let qr = field(&data, "sdk_qr_code")?.into();
        let id = uuid::Uuid::new_v4().to_string();
        self.pending = Some(Pending {
            id: id.clone(),
            code,
            deadline: Instant::now() + crate::plugin_config::TIMEOUT,
            next_poll: Instant::now(),
            state: LoginState::Waiting,
        });
        Ok(ResultData::LoginChallenge(LoginChallenge {
            challenge_id: id,
            qr_text: qr,
            expires_in_seconds: crate::plugin_config::TIMEOUT.as_secs() as u32,
        }))
    }
    pub(super) fn login_poll(&mut self, id: &str) -> Result<ResultData, ApiError> {
        let pending = self
            .pending
            .as_ref()
            .ok_or_else(|| fail("LOGIN", "没有待确认的二维码"))?;
        if pending.id != id {
            return Err(fail("LOGIN", "二维码任务不匹配"));
        }
        if Instant::now() >= pending.deadline {
            self.pending = None;
            return Ok(ResultData::LoginState(LoginState::Expired));
        }
        if Instant::now() < pending.next_poll {
            return Ok(ResultData::LoginState(pending.state.clone()));
        }
        let poll_started = Instant::now();
        let response = self.client.as_mut().unwrap().request_until(
            POLL,
            &[("qqmusic_openid_authCode", pending.code.clone())],
            None,
            &[
                "login_type",
                "user_login_type",
                "qqmusic_open_id",
                "qqmusic_open_appid",
                "qqmusic_access_token",
            ],
            true,
            pending.deadline,
        );
        self.pending.as_mut().unwrap().next_poll =
            poll_started + crate::plugin_config::POLL_INTERVAL;
        let data = response?;
        let (ret, subret) = status(&data)?;
        let state = match (ret, subret) {
            (-11, 0) => LoginState::Waiting,
            (-19, 0) => LoginState::Scanned,
            (-10 | -14, 0) => {
                self.pending = None;
                LoginState::Expired
            }
            (0, 0) => {
                self.pending = None;
                let code = field(&data, "encryptString")?;
                let c = self.client.as_mut().unwrap();
                let prefix = format!("{}&", c.config.app_id);
                let code = code.strip_prefix(&prefix).unwrap_or(code);
                if code.is_empty() {
                    return Err(fail("LOGIN", "授权码为空"));
                }
                let token = success(c.request(
                    TOKEN,
                    &[("cmd", "getToken".into()), ("code", code.into())],
                    None,
                    &[
                        "app_key",
                        "login_type",
                        "user_login_type",
                        "qqmusic_open_id",
                        "qqmusic_open_appid",
                        "qqmusic_access_token",
                    ],
                    false,
                )?)?;
                let session = decrypt(field(&token, "encryptString")?, &c.config.key, c.time())?;
                // Do not publish Authorized until an authenticated account request succeeds.
                let previous = self.session.replace(session);
                match self.account() {
                    Ok(account) => {
                        let previous_generation = self.generation.take();
                        if let Err(error) = self.persist_session() {
                            self.session = previous;
                            self.generation = previous_generation;
                            return Err(error);
                        }
                        self.restored = true;
                        self.tokens.clear();
                        self.updates.clear();
                        self.media.clear();
                        self.public_playlists.clear();
                        LoginState::Authorized(account)
                    }
                    Err(e) => {
                        self.session = previous;
                        return Err(e);
                    }
                }
            }
            _ => {
                return Err(ApiError::new(
                    "SERVER",
                    format!("二维码授权失败：ret={ret}, subRet={subret}"),
                ));
            }
        };
        if let Some(pending) = &mut self.pending {
            pending.state = state.clone();
        }
        Ok(ResultData::LoginState(state))
    }
}

pub(super) fn account_profile(data: &Value) -> Result<Account, ApiError> {
    let user = &data["user_info"];
    let name = user["nickname"]
        .as_str()
        .map(clean)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| fail("SCHEMA", "账号资料缺少 user_info.nickname"))?;
    Ok(Account {
        display_name: name,
        avatar_url: account_avatar_url(user["headimgurl"].as_str()),
    })
}
pub(super) fn account_avatar_url(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.len() > 4096 {
        return None;
    }
    let mut url = url::Url::parse(value).ok()?;
    let host = url.host_str()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !(host == "qlogo.cn"
            || host.ends_with(".qlogo.cn")
            || host.ends_with(".qq.com")
            || host.ends_with(".qpic.cn")
            || host.ends_with(".gtimg.cn"))
    {
        return None;
    }
    url.set_scheme("https").ok()?;
    url.set_fragment(None);
    Some(url.into())
}
