use super::*;

#[test]
fn bundled_configuration_recovers_the_saved_device_without_rotating_existing_login_data() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::at(directory.path().into());
    let mut previous = Config::bundled();
    previous.device_id = "synthetic-persisted-device".into();
    previous.explicit_device_id = true;
    let session = Session {
        open_id: "synthetic-open-id".into(),
        access: "synthetic-access".into(),
        refresh: "synthetic-refresh".into(),
        expires: crate::protocol::now() + 3600,
        music_id: None,
    };
    let generation = store.save(&previous, &session, None).unwrap();
    let bytes = std::fs::read(directory.path().join("session.dat")).unwrap();
    let bundled = Config::bundled();
    assert_ne!(bundled.device_id, previous.device_id);
    let restored = store.load(&bundled).unwrap().unwrap();
    assert_eq!(restored.device_id, previous.device_id);
    assert_eq!(restored.generation, generation);
    assert_eq!(restored.session.open_id, session.open_id);
    assert_eq!(
        std::fs::read(directory.path().join("session.dat")).unwrap(),
        bytes
    );
}
fn config() -> Config {
    Config {
        app_id: "test-app".into(),
        key: "test-key".into(),
        gateway: crate::plugin_config::GATEWAY.into(),
        device_id: "original-device".into(),
        explicit_device_id: false,
        package: "test-package".into(),
        device_name: "test".into(),
        common: Default::default(),
        legacy: false,
        scene: 0,
        homepage_types: String::new(),
    }
}
fn session() -> Session {
    Session {
        open_id: "synthetic-user".into(),
        access: "synthetic-access-secret".into(),
        refresh: "synthetic-refresh-secret".into(),
        expires: 1,
        music_id: Some("synthetic-music-secret".into()),
    }
}
#[test]
fn dpapi_roundtrip_is_encrypted_atomic_bound_and_accepts_expired_access_for_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::at(dir.path().join("private"));
    let mut config = config();
    assert!(store.load(&config).unwrap().is_none());
    assert!(!store.directory.exists()); // Browsing without a session writes nothing.
    let generation = store.save(&config, &session(), None).unwrap();
    let bytes = fs::read(store.path()).unwrap();
    for text in [
        "synthetic-user",
        "synthetic-access-secret",
        "synthetic-refresh-secret",
        "test-key",
        "synthetic-music-secret",
    ] {
        assert!(!bytes.windows(text.len()).any(|s| s == text.as_bytes()));
    }
    config.device_id = "new-random-device".into();
    let restored = store.load(&config).unwrap().unwrap();
    assert_eq!(restored.device_id, "original-device");
    assert_eq!(restored.generation, generation);
    assert_eq!(restored.session.refresh, "synthetic-refresh-secret");
    assert_eq!(
        restored.session.music_id.as_deref(),
        Some("synthetic-music-secret")
    );
    assert_eq!(restored.session.expires, 1);
    config.explicit_device_id = true;
    assert_eq!(store.load(&config).err().unwrap().code, "SESSION_STORAGE");
    config.explicit_device_id = false;
    config.key = "different-key".into();
    assert!(store.load(&config).is_err());
    config = self::config();
    config.app_id = "other-app".into();
    assert!(store.load(&config).is_err());
    let config = self::config();
    let mut next = session();
    next.access = "rotated-access".into();
    assert_eq!(
        store.save(&config, &next, Some(&generation)).unwrap(),
        generation
    );
    assert_eq!(
        store.load(&config).unwrap().unwrap().session.access,
        "rotated-access"
    );
    assert_eq!(fs::read_dir(&store.directory).unwrap().count(), 2); // Final ciphertext and empty lock only.
    store.clear().unwrap();
    store.clear().unwrap();
    assert!(store.load(&config).unwrap().is_none());
}
#[test]
fn legacy_encrypted_record_restores_and_music_id_survives_a_new_record() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::at(dir.path().into());
    let config = config();
    let old = Record {
        version: 1,
        binding: binding(&config),
        device_id: config.device_id.clone(),
        generation: uuid::Uuid::new_v4().to_string(),
        session: session(),
    };
    let mut old_json = serde_json::to_value(&old).unwrap();
    old_json["session"]
        .as_object_mut()
        .unwrap()
        .remove("music_id");
    let plain = Zeroizing::new(serde_json::to_vec(&old_json).unwrap());
    fs::write(store.path(), [MAGIC, &protect(&plain).unwrap()].concat()).unwrap();
    let mut saved = store.load(&config).unwrap().unwrap();
    assert!(saved.session.music_id.is_none());
    saved.session.music_id = Some("synthetic-new-music-secret".into());
    store
        .save(&config, &saved.session, Some(&saved.generation))
        .unwrap();
    assert_eq!(
        store
            .load(&config)
            .unwrap()
            .unwrap()
            .session
            .music_id
            .as_deref(),
        Some("synthetic-new-music-secret")
    );
    assert!(
        !String::from_utf8_lossy(&fs::read(store.path()).unwrap())
            .contains("synthetic-new-music-secret")
    );
}
#[test]
fn logout_or_relogin_cannot_be_undone_by_a_stale_writer() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::at(dir.path().into());
    let config = config();
    let old = store.save(&config, &session(), None).unwrap();
    store.clear().unwrap();
    assert_eq!(
        store
            .save(&config, &session(), Some(&old))
            .err()
            .unwrap()
            .code,
        "SESSION_REVOKED"
    );
    assert!(!store.exists().unwrap());
    let new = store.save(&config, &session(), None).unwrap();
    assert_ne!(new, old);
    assert_eq!(
        store.check(&config, &old).err().unwrap().code,
        "SESSION_REVOKED"
    );
    assert!(store.save(&config, &session(), Some(&old)).is_err());
    assert_eq!(store.load(&config).unwrap().unwrap().generation, new);
}
#[test]
fn corrupt_oversized_and_wrong_version_records_never_restore_or_leak_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::at(dir.path().into());
    let config = config();
    for bytes in [
        b"plaintext secret".to_vec(),
        [MAGIC, b"invalid encrypted secret"].concat(),
        vec![b'x'; LIMIT as usize + 1],
    ] {
        fs::write(store.path(), bytes).unwrap();
        let error = store.load(&config).err().unwrap();
        assert_eq!(error.code, "SESSION_STORAGE");
        assert!(!error.message.contains("secret"));
        assert!(store.exists().unwrap()); // Preserve unexpected data; explicit logout can clear it.
    }
    let record = Record {
        version: 99,
        binding: binding(&config),
        device_id: config.device_id.clone(),
        generation: uuid::Uuid::new_v4().to_string(),
        session: session(),
    };
    let plain = Zeroizing::new(serde_json::to_vec(&record).unwrap());
    fs::write(store.path(), [MAGIC, &protect(&plain).unwrap()].concat()).unwrap();
    assert!(store.load(&config).is_err());
    // A newly verified login replaces a corrupt cache instead of forcing manual deletion.
    store.save(&config, &session(), None).unwrap();
    assert!(store.load(&config).is_ok());
}
#[test]
fn failed_replacement_keeps_previous_ciphertext_and_never_leaves_plaintext_temps() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = tempfile::tempdir().unwrap();
    let store = SessionStore::at(dir.path().into());
    let config = config();
    let old = store.save(&config, &session(), None).unwrap();
    let bytes = fs::read(store.path()).unwrap();
    let held = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(store.path())
        .unwrap(); // No FILE_SHARE_DELETE.
    assert_eq!(
        store
            .save(&config, &session(), Some(&old))
            .err()
            .unwrap()
            .code,
        "SESSION_STORAGE"
    );
    assert_eq!(fs::read(store.path()).unwrap(), bytes);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    assert!(store.clear().is_err());
    drop(held);
    store.clear().unwrap();
}
