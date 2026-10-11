use super::*;
use serde_json::json;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
mod bundled;
mod homepage;
mod session;
mod signing;
mod status;

fn config() -> Config {
    Config {
        app_id: "test-app".into(),
        key: "test-key".into(),
        gateway: crate::plugin_config::GATEWAY.into(),
        device_id: "test-device".into(),
        explicit_device_id: true,
        package: "test".into(),
        device_name: "test".into(),
        common: Default::default(),
        legacy: false,
        scene: 0,
        homepage_types: String::new(),
    }
}
