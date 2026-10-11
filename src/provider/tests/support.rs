use super::*;

pub(crate) fn public_replies() -> Vec<Value> {
    vec![
        json!({"code":0,"data":{"list":[{"dissid":123,"dissname":"公开歌单"}]}}),
        json!({"code":0,"req_0":{"code":0,"data":{"code":0,"dirinfo":{"id":123},"songlist":[]}}}),
        json!({"ret":0,"songlist":[{"song_id":42,"song_name":"账号个性歌曲","song_token":"synthetic-song-secret"}]}),
    ]
}
#[cfg(windows)]
pub(crate) fn encrypted_session(access: &str, refresh: &str, expires: u64) -> String {
    encrypted_session_with_music_id("synthetic-open-id", access, refresh, expires, None)
}
#[cfg(windows)]
pub(crate) fn encrypted_session_with_music_id(
    open_id: &str,
    access: &str,
    refresh: &str,
    expires: u64,
    music_id: Option<Value>,
) -> String {
    use aes::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
    use base64::Engine;
    let key = hash("synthetic-key").into_bytes();
    let mut payload = json!({"qqmusic_open_id":open_id, "qqmusic_access_token":access,
        "qqmusic_refresh_token":refresh,"expireTime":expires});
    if let Some(music_id) = music_id {
        payload["music_id"] = music_id;
    }
    let plain = zeroize::Zeroizing::new(payload.to_string());
    base64::engine::general_purpose::STANDARD.encode(
        cbc::Encryptor::<aes::Aes256>::new_from_slices(&key, &key[..16])
            .unwrap()
            .encrypt_padded_vec_mut::<Pkcs7>(plain.as_bytes()),
    )
}
pub(crate) fn replies(values: Vec<Value>) -> discovery::testing::Server {
    discovery::testing::Server::new(
        values
            .into_iter()
            .map(|v| ("200 OK", serde_json::to_vec(&v).unwrap(), ""))
            .collect(),
    )
}
#[cfg(windows)]
pub(crate) fn login_replies(account: Value) -> Vec<Value> {
    vec![
        json!({"ret":0,"auth_code":"synthetic-qr","sdk_qr_code":"https://y.qq.com/synthetic-qr"}),
        json!({"ret":0,"encryptString":"synthetic-app&synthetic-code"}),
        json!({"ret":0,"encryptString":encrypted_session("login-access","login-refresh",now()+3600)}),
        account,
    ]
}
#[cfg(windows)]
pub(crate) fn finish_login(p: &mut Provider) -> Result<ResultData, ApiError> {
    let ResultData::LoginChallenge(challenge) = p.handle(Operation::LoginBegin)? else {
        panic!("challenge");
    };
    p.handle(Operation::LoginPoll {
        challenge_id: challenge.challenge_id,
    })
}
pub(crate) fn authenticated_provider(base: &str) -> Provider {
    let mut p = Provider::new();
    p.signed_homepage_enabled = false;
    p.client = Some(Client::test_client(Config {
        app_id: "synthetic-app".into(),
        key: "synthetic-key".into(),
        gateway: format!("{base}/opi"),
        device_id: "-device-with-hyphens".into(),
        explicit_device_id: true,
        package: "synthetic".into(),
        device_name: "test".into(),
        common: Default::default(),
        legacy: false,
        scene: 0,
        homepage_types: String::new(),
    }));
    p.session = Some(Session {
        open_id: "synthetic-open-id".into(),
        access: "synthetic-access".into(),
        refresh: "synthetic-refresh".into(),
        expires: now() + 3600,
        music_id: None,
    });
    p.public = Some(discovery::PublicClient::test_client(base));
    p
}
