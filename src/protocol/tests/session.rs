use super::*;
use aes::cipher::{KeyIvInit, block_padding::Pkcs7};
use base64::Engine;
#[test]
fn decrypt_numeric_string_expiry() {
    use aes::cipher::BlockEncryptMut;
    let payload = json!({"qqmusic_open_id":"synthetic", "qqmusic_access_token":"access", "qqmusic_refresh_token":"refresh", "expireTime":"2000000000"});
    let key = hash("synthetic-key").into_bytes();
    let encrypted = cbc::Encryptor::<aes::Aes256>::new_from_slices(&key, &key[..16])
        .unwrap()
        .encrypt_padded_vec_mut::<Pkcs7>(payload.to_string().as_bytes());
    let b64 = base64::engine::general_purpose::STANDARD.encode(encrypted);
    assert_eq!(
        decrypt(&b64, "synthetic-key", 1800000000).unwrap().expires,
        2000000000
    );
    assert!(decrypt(&b64, "synthetic-key", 2100000000).is_err());
    assert!(decrypt(&b64, "wrong", 0).is_err());
}
#[test]
fn decrypted_optional_music_id_is_preserved_without_guessing_or_disclosure() {
    use aes::cipher::BlockEncryptMut;
    let encrypt = |payload: &Value| {
        let key = hash("synthetic-key").into_bytes();
        base64::engine::general_purpose::STANDARD.encode(
            cbc::Encryptor::<aes::Aes256>::new_from_slices(&key, &key[..16])
                .unwrap()
                .encrypt_padded_vec_mut::<Pkcs7>(payload.to_string().as_bytes()),
        )
    };
    let base = json!({"qqmusic_open_id":"synthetic-open-id", "qqmusic_access_token":"access",
        "qqmusic_refresh_token":"refresh", "expireTime":2000000000});
    for (value, expected) in [
        (None, None),
        (Some(Value::Null), None),
        (Some(json!("")), Some("")),
        (Some(json!(9007199254740993u64)), Some("9007199254740993")),
        (Some(json!("9007199254740993")), Some("9007199254740993")),
    ] {
        let mut payload = base.clone();
        if let Some(value) = value {
            payload["music_id"] = value;
        }
        let session = decrypt(&encrypt(&payload), "synthetic-key", 1800000000).unwrap();
        assert_eq!(session.music_id.as_deref(), expected);
        assert_eq!(session.open_id, "synthetic-open-id");
    }
    for invalid in [
        json!(1.5),
        json!({"secret":"synthetic"}),
        json!([]),
        json!("x".repeat(1025)),
        json!("synthetic\nsecret"),
    ] {
        let mut payload = base.clone();
        payload["music_id"] = invalid;
        let error = decrypt(&encrypt(&payload), "synthetic-key", 1800000000)
            .err()
            .unwrap();
        assert_eq!(error.code, "SCHEMA");
        assert!(!error.message.contains("secret") && !error.message.contains("synthetic"));
    }
}
#[test]
fn old_session_json_loads_without_music_id_and_new_session_roundtrips_it() {
    let old =
        json!({"open_id":"synthetic-open", "access":"access", "refresh":"refresh", "expires":1});
    let mut session: Session = serde_json::from_value(old.clone()).unwrap();
    assert!(session.music_id.is_none());
    assert_eq!(serde_json::to_value(&session).unwrap(), old);
    session.music_id = Some("synthetic-music-id".into());
    let decoded: Session = serde_json::from_value(serde_json::to_value(&session).unwrap()).unwrap();
    assert_eq!(decoded.music_id.as_deref(), Some("synthetic-music-id"));
}
