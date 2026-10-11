use std::time::Duration;

pub const APP_ID: &str = "10022";
pub const APP_KEY: &str = "C9dXRgxAP46Cej5l";
pub const AUTH_MODE: &str = "v2";
pub const GATEWAY: &str = "https://qplaycloud-quic.y.qq.com/rpc_proxy/fcgi-bin/music_open_api.fcg";
pub const PACKAGE_NAME: &str = "com.tencent.qqmusicpad";
pub const DEVICE_NAME: &str = "PLK110";
pub const TIMEOUT: Duration = Duration::from_secs(180);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const POLL_INTERVAL: Duration = Duration::from_millis(1000);
