use super::*;

impl RecommendationClient {
    pub(super) fn comm(&self, early: bool) -> BTreeMap<String, String> {
        let mut map = BTreeMap::from([
            ("ct".into(), "11".into()),
            ("cv".into(), CLIENT_VERSION.into()),
            ("v".into(), CLIENT_VERSION.into()),
            ("versionCode".into(), CLIENT_VERSION.into()),
            ("tmeAppID".into(), "meizulite".into()),
            ("os_ver".into(), "16".into()),
            ("phonetype".into(), "FluentMusic".into()),
            ("devicelevel".into(), "36".into()),
            ("rom".into(), "FluentMusic".into()),
            ("aid".into(), self.device.clone()),
            ("nettype".into(), "wifi".into()),
            ("OpenUDID".into(), self.device.clone()),
            ("udid".into(), self.device.clone()),
        ]);
        if early {
            map.insert("earlyStage".into(), "1".into());
        } else if let Some(s) = &self.base {
            map.extend([
                ("uid".into(), s.uid.clone()),
                ("sid".into(), s.sid.clone()),
                ("userip".into(), s.user_ip.clone()),
            ]);
        }
        if !early && let Some(account) = &self.account {
            map.extend([
                ("openAppID".into(), account.app_id.clone()),
                ("uin".into(), account.open_id.clone()),
                ("authst".into(), account.access.clone()),
            ]);
        }
        map
    }
    pub(super) fn request(
        &self,
        module: &str,
        method: &str,
        param: Value,
        early: bool,
        deadline: Instant,
    ) -> Result<Value, ApiError> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(fail("TIMEOUT", "Flyme 请求超时"));
        }
        let name = format!("{module}.{method}");
        let body = Zeroizing::new(
            serde_json::to_vec(&json!({
                "comm": self.comm(early),
                name.clone(): {"module": module, "method": method, "param": param}
            }))
            .map_err(|_| fail("SCHEMA", "Flyme 请求编码失败"))?,
        );
        let mut random = [0u8; 32];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| fail("RANDOM", "签名随机数生成失败"))?;
        #[cfg(not(test))]
        let url = if early { BOOTSTRAP_URL } else { SIGNED_URL };
        #[cfg(test)]
        let url = if early {
            &self.bootstrap_url
        } else {
            &self.signed_url
        };
        let mut builder = self
            .http
            .post(url)
            .timeout(remaining.min(crate::plugin_config::REQUEST_TIMEOUT))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("M-Encoding", "m1")
            .header("User-Agent", "FluentMusic/0.1");
        if !early {
            let uid = self
                .base
                .as_ref()
                .ok_or_else(|| fail("SESSION", "Flyme 基础会话尚未取得"))?
                .uid
                .as_str();
            let millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let uin = self.account.as_ref().map_or("0", |a| a.open_id.as_str());
            let metadata = Zeroizing::new(format!(
                "11&{CLIENT_VERSION}&{uid}&{uin}&{millis}&{}&android",
                self.device
            ));
            let (sign, mask) = signature(&body, metadata.as_bytes(), &self.key, &random);
            builder = builder
                .header("sign", sign)
                .header("mask", mask)
                .header("x-sign-data-type", "json");
        }
        let mut zipped =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        zipped
            .write_all(&body)
            .map_err(|_| fail("SCHEMA", "Flyme 请求压缩失败"))?;
        let mut packed = random[27..32].iter().map(|b| b % 100).collect::<Vec<_>>();
        packed.extend(
            zipped
                .finish()
                .map_err(|_| fail("SCHEMA", "Flyme 请求压缩失败"))?,
        );
        let response = builder
            .body(packed)
            .send()
            .map_err(|_| fail("NETWORK", "Flyme HTTPS 请求失败"))?;
        if !response.status().is_success() {
            return Err(ApiError::new(
                "HTTP",
                format!("Flyme HTTP 状态 {}", response.status().as_u16()),
            ));
        }
        let gzip = response
            .headers()
            .get("Content-Encoding")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("gzip"));
        let mut bytes = Vec::new();
        response
            .take(RESPONSE_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| fail("NETWORK", "Flyme 响应读取失败"))?;
        if bytes.len() > RESPONSE_LIMIT {
            return Err(fail("SCHEMA", "Flyme 响应超出限制"));
        }
        let decoded = if gzip {
            decompress(&bytes, true)?
        } else if bytes.len() > 7
            && matches!(
                bytes.get(5..7),
                Some([0x78, 0x9c] | [0x78, 0xda] | [0x78, 0x01])
            )
        {
            decompress(&bytes[5..], false)?
        } else {
            bytes
        };
        let root: Value =
            serde_json::from_slice(&decoded).map_err(|_| fail("SCHEMA", "Flyme 响应不是 JSON"))?;
        check_status(&root, true)?;
        check_status(&root[&name], true)?;
        let data = root[&name]
            .get("data")
            .filter(|v| v.is_object())
            .ok_or_else(|| fail("SCHEMA", "Flyme 响应缺少 data"))?;
        check_status(data, false)?;
        Ok(data.clone())
    }
}
