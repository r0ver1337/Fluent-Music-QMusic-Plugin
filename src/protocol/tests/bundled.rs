use super::*;

#[test]
fn bundled_settings_need_no_environment_or_external_common_params() {
    use crate::plugin_config;
    let config = Config::bundled();
    assert_eq!(config.app_id, "10022");
    assert_eq!(config.key, plugin_config::APP_KEY);
    assert_eq!(
        config.gateway,
        "https://qplaycloud-quic.y.qq.com/rpc_proxy/fcgi-bin/music_open_api.fcg"
    );
    assert_eq!(config.package, "com.tencent.qqmusicpad");
    assert_eq!(config.device_name, "PLK110");
    assert!(!config.legacy && !config.explicit_device_id);
    assert!(config.common.is_empty());
    assert_eq!(config.scene, 0);
    assert_eq!(plugin_config::TIMEOUT.as_secs(), 180);
    assert_eq!(plugin_config::REQUEST_TIMEOUT.as_secs(), 15);
    assert_eq!(plugin_config::POLL_INTERVAL.as_millis(), 1000);
    assert!(uuid::Uuid::parse_str(&config.device_id).is_ok());
    let params = config.parameters("synthetic-command", None, 123);
    assert_eq!(params["app_id"], "10022");
    assert_eq!(params["sign_version"], "v2");
    assert!(!params.contains_key("app_key"));
    assert_ne!(Config::bundled().device_id, config.device_id);
}

#[test]
fn bundled_settings_ignore_process_configuration_overrides() {
    if std::env::var_os("FM_BUNDLED_CONFIG_CHILD").is_some() {
        bundled_settings_need_no_environment_or_external_common_params();
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join(".env"), b"\xffinvalid dotenv data").unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .current_dir(directory.path())
        .args([
            "bundled_settings_ignore_process_configuration_overrides",
            "--test-threads=1",
        ])
        .env("FM_BUNDLED_CONFIG_CHILD", "1")
        .env("QQMUSIC_APP_ID", "override")
        .env("QQMUSIC_APP_KEY", "override")
        .env("QQMUSIC_GATEWAY", "http://127.0.0.1:1")
        .env("QQMUSIC_AUTH_MODE", "legacy")
        .env("QQMUSIC_PACKAGE_NAME", "override")
        .env("QQMUSIC_DEVICE_NAME", "override")
        .env("QQMUSIC_DEVICE_ID", "override")
        .env("QQMUSIC_COMMON_PARAMS_FILE", "nonexistent.json")
        .env("QQMUSIC_TIMEOUT_SECONDS", "1")
        .env("QQMUSIC_REQUEST_TIMEOUT_SECONDS", "1")
        .env("QQMUSIC_POLL_INTERVAL_MS", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "Bundled settings were overridden: {}",
        String::from_utf8_lossy(&result.stdout)
    );
}
