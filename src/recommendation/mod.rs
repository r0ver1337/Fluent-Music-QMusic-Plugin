//! Signed recommendation discovery with separate guest and verified OpenID sessions.
//! Verified protocol constants are built in; no native binary or HAR is read or executed.
use crate::{
    discovery::cover_url,
    protocol::{Session, fail, unsigned},
    provider::clean,
};
use fluent_plugin_api::{
    ApiError, Playlist, RecommendationFeed, RecommendationIssue, RecommendationSource, Track,
};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    io::{Read, Write},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::{Zeroize, Zeroizing};

const BOOTSTRAP_URL: &str = "https://mz.y.qq.com/cgi-bin/musicu.fcg";
const SIGNED_URL: &str = "https://mz.y.qq.com/cgi-bin/musics.fcg";
const RESPONSE_LIMIT: usize = 1024 * 1024;
const CLIENT_VERSION: &str = "22190504";
const HOMEPAGE_MODULE: &str = "music.qqmusiclite.MtHomePageSvr";
const PLAYLIST_MODULE: &str = "music.playlist.PlaylistSquare";

mod authorization;
mod feed;
mod parsing;
mod response;
mod signing;
mod transport;
use parsing::{homepage_tracks, id, recommended_playlists};
use response::{check_status, decompress};
use signing::{material, signature};
#[cfg(test)]
mod tests;

struct BaseSession {
    uid: String,
    sid: String,
    user_ip: String,
    created: Instant,
}
// Official OPEN_ID branch: MusicKey.musicID=AccessToken.openID,
// musicKey=AccessToken.accessToken, openID=AccessToken.appID. It is NOT the QQ/WX
// branch, and does not substitute the separately saved numeric music_id.
struct OpenIdAccount {
    app_id: String,
    open_id: String,
    access: String,
}
impl Drop for OpenIdAccount {
    fn drop(&mut self) {
        self.app_id.zeroize();
        self.open_id.zeroize();
        self.access.zeroize();
    }
}
impl Drop for BaseSession {
    fn drop(&mut self) {
        self.uid.zeroize();
        self.sid.zeroize();
        self.user_ip.zeroize();
    }
}
pub struct RecommendationClient {
    http: crate::http::Client,
    device: String,
    key: Zeroizing<[u8; 12]>,
    base: Option<BaseSession>,
    account: Option<OpenIdAccount>,
    account_verified_until: Option<Instant>,
    #[cfg(test)]
    bootstrap_url: String,
    #[cfg(test)]
    signed_url: String,
}
impl RecommendationClient {
    #[cfg(test)]
    pub(crate) fn test_client(base: &str) -> Self {
        assert!(base.starts_with("http://127.0.0.1:"));
        let mut client = Self::new("synthetic-device".into()).unwrap();
        client.http = crate::http::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        client.bootstrap_url = format!("{base}/normal");
        client.signed_url = format!("{base}/safe");
        client
    }
    pub fn new(device: String) -> Result<Self, ApiError> {
        if device.is_empty()
            || device.len() > 128
            || device.contains('&')
            || device.chars().any(char::is_control)
        {
            return Err(fail("CONFIG", "Flyme 设备标识无效"));
        }
        Ok(Self {
            http: crate::http::Client::builder()
                .timeout(crate::plugin_config::REQUEST_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| fail("NETWORK", "Flyme HTTPS 初始化失败"))?,
            device,
            key: material(),
            base: None,
            account: None,
            account_verified_until: None,
            #[cfg(test)]
            bootstrap_url: BOOTSTRAP_URL.into(),
            #[cfg(test)]
            signed_url: SIGNED_URL.into(),
        })
    }
    pub fn clear(&mut self) {
        self.base = None;
        self.account = None;
        self.account_verified_until = None;
    }
}
