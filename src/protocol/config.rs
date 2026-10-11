use super::*;
use std::collections::BTreeMap;

pub struct Config {
    pub app_id: String,
    pub key: String,
    pub gateway: String,
    pub device_id: String,
    pub explicit_device_id: bool,
    pub package: String,
    pub device_name: String,
    pub common: BTreeMap<String, String>,
    pub legacy: bool,
    pub scene: u32,
    pub homepage_types: String,
}
impl Config {
    pub fn bundled() -> Self {
        use crate::plugin_config;
        Self {
            app_id: plugin_config::APP_ID.into(),
            key: plugin_config::APP_KEY.into(),
            gateway: plugin_config::GATEWAY.into(),
            explicit_device_id: false,
            device_id: uuid::Uuid::new_v4().to_string(),
            package: plugin_config::PACKAGE_NAME.into(),
            device_name: plugin_config::DEVICE_NAME.into(),
            common: BTreeMap::new(),
            legacy: plugin_config::AUTH_MODE == "legacy",
            scene: 0,
            homepage_types: String::new(),
        }
    }
    pub fn parameters(
        &self,
        command: &str,
        session: Option<&Session>,
        timestamp: u64,
    ) -> BTreeMap<String, String> {
        let mut p = BTreeMap::from([
            ("uid".into(), "0".into()),
            ("stream_format".into(), "1".into()),
            (
                "qqmusic_open_appid".into(),
                if self.legacy {
                    "2".into()
                } else {
                    self.app_id.clone()
                },
            ),
        ]);
        if !self.legacy {
            p.insert("ct".into(), "sdk_android".into());
        }
        p.extend(self.common.clone());
        p.extend([
            ("app_id".into(), self.app_id.clone()),
            ("opi_device_id".into(), self.device_id.clone()),
            ("opi_cmd".into(), command.into()),
            ("timestamp".into(), timestamp.to_string()),
        ]);
        if self.legacy {
            p.insert("app_key".into(), self.key.clone());
        } else {
            p.extend([
                ("sign_version".into(), "v2".into()),
                ("aes_version".into(), "1".into()),
            ]);
        }
        if let Some(s) = session {
            p.extend([
                ("login_type".into(), "5".into()),
                ("qqmusic_open_id".into(), s.open_id.clone()),
                ("qqmusic_access_token".into(), s.access.clone()),
            ]);
            if !self.legacy {
                p.insert("user_login_type".into(), "6".into());
            }
        }
        p
    }
    pub(super) fn homepage_values(&self) -> Vec<(&'static str, String)> {
        // Defaults verified from OpenQQMusic 1.2.0 QQMusicApi. Explicit common
        // device/version fields still win; application identity and sessions do not change.
        let short_device_id = self.device_id.replace('-', "");
        let types = if self.homepage_types.is_empty() {
            "0"
        } else {
            &self.homepage_types
        };
        let common = |key: &str, default: String| self.common.get(key).cloned().unwrap_or(default);
        vec![
            ("sn", short_device_id.clone()),
            ("type", types.into()),
            ("need_songinfo", "1".into()),
            ("device_id", common("device_id", short_device_id)),
            ("cv", common("cv", "40071".into())),
            ("bus_cv", common("bus_cv", "6090007".into())),
            (
                "client_ip",
                self.common
                    .get("client_ip")
                    .cloned()
                    .unwrap_or_else(local_ipv4),
            ),
        ]
    }
}

fn local_ipv4() -> String {
    // UDP connect asks the OS for a local route/source address; no send, DNS,
    // packets or third-party IP lookup. Failure matches OpenQQMusic's 0.0.0.0 fallback.
    let address = (|| {
        let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)).ok()?;
        socket
            .connect((std::net::Ipv4Addr::new(192, 0, 2, 1), 9))
            .ok()?;
        match socket.local_addr().ok()?.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
            _ => None,
        }
    })();
    address
        .unwrap_or(std::net::Ipv4Addr::UNSPECIFIED)
        .to_string()
}
