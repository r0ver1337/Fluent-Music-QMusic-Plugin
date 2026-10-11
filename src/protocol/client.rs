use super::*;
use std::io::Read;
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

pub struct Client {
    pub config: Config,
    http: crate::http::Client,
    pub(super) offset_seconds: i64,
}
impl Client {
    #[cfg(test)]
    pub fn test_client(config: Config) -> Self {
        assert!(config.gateway.starts_with("http://127.0.0.1:"));
        Self {
            config,
            http: crate::http::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            offset_seconds: 0,
        }
    }
    pub fn new(config: Config) -> Result<Self, ApiError> {
        Ok(Self {
            config,
            http: crate::http::Client::builder()
                .timeout(crate::plugin_config::REQUEST_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| fail("NETWORK", "HTTPS 客户端初始化失败"))?,
            offset_seconds: 0,
        })
    }
    pub fn time(&self) -> u64 {
        (now() as i64 + self.offset_seconds).max(0) as u64
    }
    pub fn request(
        &mut self,
        command: &str,
        values: &[(&str, String)],
        session: Option<&Session>,
        clear: &[&str],
        poll: bool,
    ) -> Result<Value, ApiError> {
        self.request_until(
            command,
            values,
            session,
            clear,
            poll,
            Instant::now() + crate::plugin_config::TIMEOUT,
        )
    }
    pub fn request_until(
        &mut self,
        command: &str,
        values: &[(&str, String)],
        session: Option<&Session>,
        clear: &[&str],
        poll: bool,
        deadline: Instant,
    ) -> Result<Value, ApiError> {
        for attempt in 0..2 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(fail("TIMEOUT", "QQ 请求超时，请重试"));
            }
            let mut p = self.config.parameters(command, session, self.time());
            if poll && let Some(appid) = p.remove("qqmusic_open_appid") {
                p.insert("qqmusic_openid_appId".into(), appid);
            }
            for key in clear {
                p.remove(*key);
            }
            for (k, v) in values {
                p.insert((*k).into(), v.clone());
            }
            let p = signed(p, &self.config.key);
            let query = p
                .iter()
                .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
                .collect::<Vec<_>>()
                .join("&");
            let response = self
                .http
                .get(format!("{}?{query}", self.config.gateway))
                .timeout(remaining.min(crate::plugin_config::REQUEST_TIMEOUT))
                .send()
                .map_err(|_| fail("NETWORK", "QQ HTTPS 请求失败"))?;
            if !response.status().is_success() {
                return Err(ApiError::new(
                    "HTTP",
                    format!("QQ HTTP 状态 {}", response.status().as_u16()),
                ));
            }
            let mut bytes = vec![];
            response
                .take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| fail("NETWORK", "QQ 响应读取失败"))?;
            if bytes.len() > 1024 * 1024 {
                return Err(fail("SCHEMA", "QQ 响应超过 1 MiB 限制"));
            }
            let data: Value = serde_json::from_slice(&bytes)
                .map_err(|_| fail("SCHEMA", "QQ 响应不是合法 JSON"))?;
            if attempt == 0
                && status(&data).ok() == Some((10203, 1200))
                && let Some(ts) = unsigned(&data["ts"])
            {
                self.offset_seconds = (ts / 1000) as i64 - now() as i64;
                continue;
            }
            return Ok(data);
        }
        Err(fail("NETWORK", "服务器时间同步失败"))
    }
    pub fn homepage_playlists(
        &mut self,
        session: Option<&Session>,
        deadline: Instant,
    ) -> Result<Value, ApiError> {
        let values = self.config.homepage_values();
        let data = self.request_until(HOMEPAGE, &values, session, &[], false, deadline)?;
        homepage_success(data)
    }
}
