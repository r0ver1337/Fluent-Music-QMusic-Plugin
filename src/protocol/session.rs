use super::*;
use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use base64::Engine;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub open_id: String,
    pub access: String,
    pub refresh: String,
    pub expires: u64,
    /// Optional account identity from the decrypted token, not the OPI openId/base uid.
    /// Missing/null remains unknown; an explicit empty string is preserved as sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music_id: Option<String>,
}
impl Drop for Session {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.open_id.zeroize();
        self.access.zeroize();
        self.refresh.zeroize();
        self.music_id.zeroize();
    }
}
pub fn decrypt(ciphertext: &str, key: &str, timestamp: u64) -> Result<Session, ApiError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(ciphertext.trim())
        .map_err(|_| fail("DECRYPT", "令牌数据不是合法 Base64"))?;
    let key = hash(key).into_bytes();
    let plaintext = zeroize::Zeroizing::new(
        cbc::Decryptor::<aes::Aes256>::new_from_slices(&key, &key[..16])
            .map_err(|_| fail("DECRYPT", "AES 参数无效"))?
            .decrypt_padded_vec_mut::<Pkcs7>(&bytes)
            .map_err(|_| fail("DECRYPT", "登录数据 AES 解密失败"))?,
    );
    let data: Value =
        serde_json::from_slice(&plaintext).map_err(|_| fail("DECRYPT", "登录数据不是 JSON"))?;
    let expires = unsigned(&data["expireTime"])
        .filter(|n| *n > timestamp)
        .ok_or_else(|| {
            fail(
                "SESSION",
                "expireTime 缺失、无效或令牌已过期；不会猜测有效期",
            )
        })?;
    Ok(Session {
        open_id: field(&data, "qqmusic_open_id")?.into(),
        access: field(&data, "qqmusic_access_token")?.into(),
        refresh: field(&data, "qqmusic_refresh_token")?.into(),
        expires,
        music_id: match data.get("music_id") {
            None | Some(Value::Null) => None,
            Some(Value::String(id)) if valid_music_id(id) => Some(id.clone()),
            // Gson's String adapter accepts an integral JSON primitive. Preserve
            // its exact decimal value, never route a musicId through f64.
            Some(Value::Number(id)) if id.is_u64() || id.is_i64() => Some(id.to_string()),
            Some(_) => return Err(fail("SCHEMA", "令牌 music_id 类型或长度无效")),
        },
    })
}

pub fn valid_music_id(id: &str) -> bool {
    id.len() <= 1024 && !id.chars().any(char::is_control)
}
