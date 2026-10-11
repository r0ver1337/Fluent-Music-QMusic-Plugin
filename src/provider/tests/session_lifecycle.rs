use super::*;

#[cfg(windows)]
#[test]
fn saved_session_restarts_directly_into_openqqmusic_homepage_without_rescanning() {
    let dir = tempfile::tempdir().unwrap();
    let server = replies(vec![
        json!({"ret":0,"rc_item_arr":[{"dissid":123,"dirName":"复用登录歌单"}]}),
        json!({"ret":0,"songlist":[{"song_id":42,"song_name":"歌曲"}]}),
    ]);
    let mut p = authenticated_provider(&server.base);
    p.store = Some(SessionStore::at(dir.path().into()));
    p.persist_session().unwrap();
    let generation = p.generation.clone();
    let bytes = std::fs::read(dir.path().join("session.dat")).unwrap();
    drop(p);
    let mut restarted = authenticated_provider(&server.base);
    restarted.session = None;
    restarted.client.as_mut().unwrap().config.explicit_device_id = false;
    restarted.client.as_mut().unwrap().config.device_id = "new-random-device".into();
    restarted.store = Some(SessionStore::at(dir.path().into()));
    let ResultData::RecommendationFeed(feed) = restarted.handle(Operation::Homepage).unwrap()
    else {
        panic!("feed");
    };
    assert_eq!(feed.playlist_source, RecommendationSource::Account);
    assert_eq!(feed.playlists[0].title, "复用登录歌单");
    assert_eq!(restarted.generation, generation);
    assert_eq!(
        std::fs::read(dir.path().join("session.dat")).unwrap(),
        bytes
    );
    server.thread.join().unwrap();
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].contains(HOMEPAGE));
    assert!(
        requests[0].contains("sn=devicewithhyphens") && requests[0].contains("synthetic-access")
    );
    assert!(
        !requests
            .iter()
            .any(|r| r.contains(QR) || r.contains(TOKEN) || r.contains("new-random-device"))
    );
}
#[test]
fn saved_session_diagnostic_reports_presence_only_without_identity_values() {
    let mut p = authenticated_provider("http://127.0.0.1:1");
    for (id, state, available) in [
        (None, "absent", false),
        (Some(""), "empty", false),
        (Some("0"), "zero", false),
        (Some("synthetic-music-secret"), "present", true),
    ] {
        p.session.as_mut().unwrap().music_id = id.map(str::to_owned);
        let status = saved_session_status(p.session.as_ref());
        assert_eq!(status["music_id_state"], state);
        assert_eq!(status["music_id_available"], available);
        let text = status.to_string();
        for secret in [
            "synthetic-music-secret",
            "synthetic-open-id",
            "synthetic-access",
            "synthetic-refresh",
        ] {
            assert!(!text.contains(secret));
        }
    }
    assert_eq!(saved_session_status(None)["saved_session"], false);
}
#[cfg(windows)]
#[test]
fn logout_of_another_instance_invalidates_memory_before_any_authenticated_request() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = authenticated_provider("http://127.0.0.1:1");
    p.store = Some(SessionStore::at(dir.path().into()));
    p.persist_session().unwrap();
    p.tokens.insert("1".into(), "private-song-token".into());
    let mut other = Provider::new();
    other.store = Some(SessionStore::at(dir.path().into()));
    other.handle(Operation::Logout).unwrap();
    assert_eq!(
        p.handle(Operation::Account).err().unwrap().code,
        "SESSION_REVOKED"
    );
    assert!(p.session.is_none() && p.tokens.is_empty());
    assert!(!dir.path().join("session.dat").exists());
}
#[cfg(windows)]
#[test]
fn corrupt_cache_does_not_break_handshake_or_public_browsing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("session.dat"), b"bad secret").unwrap();
    let server = replies(vec![
        json!({"code":0,"data":{"list":[{"dissid":123,"dissname":"public"}]}}),
        json!({"code":0,"req_0":{"code":0,"data":{"code":0,"dirinfo":{"id":123},"songlist":[]}}}),
    ]);
    let mut p = authenticated_provider(&server.base);
    p.session = None;
    p.store = Some(SessionStore::at(dir.path().into()));
    p.handle(Operation::Describe).unwrap();
    assert!(!p.restored);
    let ResultData::RecommendationFeed(feed) = p.handle(Operation::Homepage).unwrap() else {
        panic!("feed");
    };
    assert_eq!(feed.playlist_source, RecommendationSource::Public);
    assert_eq!(feed.issues[0].stage, "session_storage");
    assert_eq!(feed.issues[0].error.code, "SESSION_STORAGE");
    assert!(p.session.is_none());
    assert!(!serde_json::to_string(&feed).unwrap().contains("secret"));
    assert_eq!(
        p.handle(Operation::Account).err().unwrap().code,
        "SESSION_STORAGE"
    );
    p.handle(Operation::Logout).unwrap();
    assert!(!dir.path().join("session.dat").exists());
    server.thread.join().unwrap();
}
