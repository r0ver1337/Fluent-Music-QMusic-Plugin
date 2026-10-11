use super::*;

impl RecommendationClient {
    pub fn set_open_id_account(&mut self, app_id: &str, session: &Session) -> Result<(), ApiError> {
        for value in [app_id, session.open_id.as_str(), session.access.as_str()] {
            if value.is_empty()
                || value.len() > 16384
                || value.contains('&')
                || value.chars().any(char::is_control)
            {
                return Err(fail("SESSION", "OpenID 授权字段无效"));
            }
        }
        if self.account.as_ref().is_some_and(|a| {
            a.app_id == app_id && a.open_id == session.open_id && a.access == session.access
        }) {
            return Ok(());
        }
        self.account_verified_until = None;
        self.account = Some(OpenIdAccount {
            app_id: app_id.into(),
            open_id: session.open_id.clone(),
            access: session.access.clone(),
        });
        Ok(())
    }
    pub fn clear_account(&mut self) {
        self.account = None;
        self.account_verified_until = None;
    }
    pub fn verify_open_id_account(&mut self, deadline: Instant) -> Result<Vec<String>, ApiError> {
        if self
            .account_verified_until
            .is_some_and(|until| Instant::now() < until)
        {
            return Ok(Vec::new());
        }
        self.session(deadline)?;
        let uin = self
            .account
            .as_ref()
            .ok_or_else(|| fail("LOGIN_REQUIRED", "未设置本人 OpenID 授权"))?
            .open_id
            .clone();
        let data = self.request(
            "music.UserInfo.userInfoServer",
            "GetLoginUserInfo",
            json!({"uin":uin}),
            false,
            deadline,
        )?;
        let fields = data
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        if fields.is_empty()
            || !data["info"]
                .as_object()
                .is_some_and(|info| !info.is_empty())
        {
            return Err(fail("SCHEMA", "Flyme 账号验证未返回资料字段"));
        }
        self.account_verified_until = Some(Instant::now() + Duration::from_secs(60));
        Ok(fields)
    }
    pub(super) fn session(&mut self, deadline: Instant) -> Result<(), ApiError> {
        if self
            .base
            .as_ref()
            .is_some_and(|s| s.created.elapsed() < Duration::from_secs(600))
        {
            return Ok(());
        }
        self.base = None;
        let data = self.request(
            "music.getSession.session",
            "GetSession",
            json!({"uid":"0","key":0}),
            true,
            deadline,
        )?;
        let s = &data["session"];
        let id = id(&s["uid"]).ok_or_else(|| fail("SESSION", "Flyme 未返回有效基础 uid"))?;
        let sid = s["sid"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control))
            .ok_or_else(|| fail("SESSION", "Flyme 未返回有效基础 sid"))?;
        self.base = Some(BaseSession {
            uid: id,
            sid: sid.into(),
            user_ip: s["userip"].as_str().unwrap_or_default().into(),
            created: Instant::now(),
        });
        Ok(())
    }
}
