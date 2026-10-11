use super::*;

#[cfg(windows)]
#[test]
fn expired_saved_credentials_refresh_and_rotate_the_encrypted_file() {
    let dir = tempfile::tempdir().unwrap();
    let server = replies(vec![
        json!({"ret":0,"encryptString":encrypted_session("new-access","new-refresh",now()+3600)}),
        json!({"ret":0,"user_info":{"nickname":"账号"}}),
    ]);
    let mut p = authenticated_provider(&server.base);
    p.store = Some(SessionStore::at(dir.path().into()));
    p.session.as_mut().unwrap().expires = 1;
    p.persist_session().unwrap();
    let generation = p.generation.clone();
    drop(p);
    let mut restarted = authenticated_provider(&server.base);
    restarted.session = None;
    restarted.store = Some(SessionStore::at(dir.path().into()));
    restarted.handle(Operation::Account).unwrap();
    let saved = restarted
        .store
        .as_ref()
        .unwrap()
        .load(&restarted.client.as_ref().unwrap().config)
        .unwrap()
        .unwrap();
    assert_eq!(saved.session.access, "new-access");
    assert_eq!(saved.session.refresh, "new-refresh");
    assert_eq!(Some(saved.generation), generation);
    server.thread.join().unwrap();
    let requests = server.requests.lock().unwrap();
    assert!(requests[0].contains("cmd=refreshToken") && requests[0].contains("synthetic-refresh"));
    assert!(requests[1].contains("new-access") && !requests[1].contains("synthetic-access"));
}
#[cfg(windows)]
#[test]
fn same_account_refresh_preserves_or_updates_music_id_and_persists_it() {
    for (incoming, expected) in [
        (None, "old-music-secret"),
        (Some(Value::Null), "old-music-secret"),
        (Some(json!("")), "old-music-secret"),
        (Some(json!("new-music-secret")), "new-music-secret"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let server = replies(vec![
            json!({"ret":0,"encryptString":encrypted_session_with_music_id("synthetic-open-id","new-access","new-refresh",now()+3600,incoming)}),
            json!({"ret":0,"user_info":{"nickname":"账号"}}),
        ]);
        let mut p = authenticated_provider(&server.base);
        p.session.as_mut().unwrap().music_id = Some("old-music-secret".into());
        p.session.as_mut().unwrap().expires = 1;
        p.store = Some(SessionStore::at(dir.path().into()));
        p.persist_session().unwrap();
        let generation = p.generation.clone();
        let reply = p.handle(Operation::Account).unwrap();
        assert_eq!(
            p.session.as_ref().unwrap().music_id.as_deref(),
            Some(expected)
        );
        let saved = p
            .store
            .as_ref()
            .unwrap()
            .load(&p.client.as_ref().unwrap().config)
            .unwrap()
            .unwrap();
        assert_eq!(saved.session.music_id.as_deref(), Some(expected));
        assert_eq!(Some(saved.generation), generation);
        assert!(
            !serde_json::to_string(&reply)
                .unwrap()
                .contains("music-secret")
        );
        server.thread.join().unwrap();
        assert!(
            !server
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.contains("music-secret"))
        );
    }
}
#[cfg(windows)]
#[test]
fn another_accounts_refresh_is_rejected_without_replacing_music_id_or_ciphertext() {
    let dir = tempfile::tempdir().unwrap();
    let server = replies(vec![
        json!({"ret":0,"encryptString":encrypted_session_with_music_id("other-open-id","other-access","other-refresh",now()+3600,Some(json!("other-music-secret")))}),
    ]);
    let mut p = authenticated_provider(&server.base);
    p.session.as_mut().unwrap().music_id = Some("original-music-secret".into());
    p.session.as_mut().unwrap().expires = 1;
    p.store = Some(SessionStore::at(dir.path().into()));
    p.persist_session().unwrap();
    let bytes = std::fs::read(dir.path().join("session.dat")).unwrap();
    assert_eq!(p.handle(Operation::Account).err().unwrap().code, "SESSION");
    assert_eq!(
        p.session.as_ref().unwrap().music_id.as_deref(),
        Some("original-music-secret")
    );
    assert_eq!(
        std::fs::read(dir.path().join("session.dat")).unwrap(),
        bytes
    );
    server.thread.join().unwrap();
}
#[cfg(windows)]
#[test]
fn refresh_failure_does_not_erase_saved_refresh_credentials_or_guess_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let server = replies(vec![json!({"ret":-1})]);
    let mut p = authenticated_provider(&server.base);
    p.session.as_mut().unwrap().expires = 1;
    p.store = Some(SessionStore::at(dir.path().into()));
    p.persist_session().unwrap();
    let original = std::fs::read(dir.path().join("session.dat")).unwrap();
    assert_eq!(p.handle(Operation::Account).err().unwrap().code, "SERVER");
    assert_eq!(
        std::fs::read(dir.path().join("session.dat")).unwrap(),
        original
    );
    assert_eq!(p.session.as_ref().unwrap().expires, 1);
    server.thread.join().unwrap();
}
