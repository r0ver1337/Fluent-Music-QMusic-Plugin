use super::*;

#[test]
fn denied_homepage_keeps_public_lists_personal_songs_and_retries_after_short_cooldown() {
    let mut values = vec![json!({"ret":10200,"code":0,"msg":"private-text-must-not-leak"})];
    values.extend(public_replies());
    values.extend(public_replies());
    values.push(json!({"ret":0,"rc_item_arr":[{"dissid":456,"dirName":"账号首页歌单"}]}));
    values.push(json!({"ret":0,"songlist":[{"song_id":43,"song_name":"账号歌曲"}]}));
    let server = discovery::testing::Server::new(
        values
            .into_iter()
            .map(|v| ("200 OK", serde_json::to_vec(&v).unwrap(), ""))
            .collect(),
    );
    let mut p = authenticated_provider(&server.base);
    for _ in 0..2 {
        let ResultData::RecommendationFeed(feed) = p.handle(Operation::Homepage).unwrap() else {
            panic!("feed");
        };
        assert_eq!(feed.playlist_source, RecommendationSource::Public);
        assert_eq!(feed.track_source, RecommendationSource::Account);
        assert!(feed.personalized);
        assert_eq!(feed.issues.len(), 1);
        assert_eq!(feed.issues[0].error.code, "APP_PERMISSION_DENIED");
        assert!(feed.notice.as_ref().unwrap().contains("该 OPI 首页请求"));
        assert!(p.session.is_some());
        assert_eq!(p.session.as_ref().unwrap().access, "synthetic-access");
        let text = serde_json::to_string(&feed).unwrap();
        for secret in ["private-text", "synthetic-access", "synthetic-song-secret"] {
            assert!(!text.contains(secret));
        }
    }
    p.homepage_denial.as_mut().unwrap().0 = Instant::now(); // No sleeping to exercise the next permission probe.
    let ResultData::RecommendationFeed(feed) = p.handle(Operation::Homepage).unwrap() else {
        panic!("feed");
    };
    assert_eq!(feed.playlist_source, RecommendationSource::Account);
    assert!(feed.issues.is_empty());
    assert!(p.homepage_denial.is_none());
    server.thread.join().unwrap();
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 9);
    assert_eq!(requests.iter().filter(|r| r.contains(HOMEPAGE)).count(), 2);
    assert_eq!(requests.iter().filter(|r| r.contains(RECOMMEND)).count(), 3);
    assert!(!requests.iter().any(|r| r.contains("GetAlgorithmPlConf")));
    assert!(!requests.iter().any(|r| r.contains(QR) || r.contains(TOKEN)));
}
#[test]
fn transient_homepage_failures_are_not_cached_as_application_permission_denials() {
    let values = vec![
        json!({"ret":-1}),
        json!({"ret":0,"rc_item_arr":[{"dissid":123,"dirName":"retry"}]}),
    ];
    let server = discovery::testing::Server::new(
        values
            .into_iter()
            .map(|v| ("200 OK", serde_json::to_vec(&v).unwrap(), ""))
            .collect(),
    );
    let mut p = authenticated_provider(&server.base);
    assert_eq!(
        p.account_playlist_feed(Instant::now() + Duration::from_secs(3))
            .err()
            .unwrap()
            .code,
        "SERVER"
    );
    assert!(p.homepage_denial.is_none());
    assert!(
        p.account_playlist_feed(Instant::now() + Duration::from_secs(3))
            .is_ok()
    );
    server.thread.join().unwrap();
    assert_eq!(server.requests.lock().unwrap().len(), 2);
}
#[test]
fn empty_or_denied_homepage_never_becomes_a_successful_account_playlist_feed() {
    for (homepage, expected_code) in [
        (json!({"ret":0,"rc_item_arr":[]}), "EMPTY"),
        (
            json!({"ret":0,"subRet":0,"sub_ret":-7,"rc_item_arr":[{"dissid":123,"dirName":"must not use"}]}),
            "SERVER",
        ),
        (
            json!({"code":0,"rc_item_arr":[{"dissid":123,"dirName":"must not use"}]}),
            "SCHEMA",
        ),
    ] {
        let mut values = vec![homepage];
        values.extend(public_replies());
        let server = discovery::testing::Server::new(
            values
                .into_iter()
                .map(|v| ("200 OK", serde_json::to_vec(&v).unwrap(), ""))
                .collect(),
        );
        let mut p = authenticated_provider(&server.base);
        let ResultData::RecommendationFeed(feed) = p.handle(Operation::Homepage).unwrap() else {
            panic!("feed");
        };
        assert_eq!(feed.playlist_source, RecommendationSource::Public);
        assert_eq!(feed.track_source, RecommendationSource::Account);
        assert_eq!(feed.playlists[0].title, "公开歌单");
        assert_eq!(feed.issues[0].stage, "account_playlists");
        assert_eq!(feed.issues[0].error.code, expected_code);
        server.thread.join().unwrap();
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.iter().filter(|r| r.contains(HOMEPAGE)).count(), 1);
        assert!(!requests.iter().any(|r| r.contains("GetAlgorithmPlConf")));
    }
}
#[cfg(windows)]
#[test]
fn app_denial_never_deletes_or_rotates_saved_user_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let mut values = vec![json!({"ret":10200})];
    values.extend(public_replies());
    values.push(json!({"ret":0,"user_info":{"nickname":"原账号"}}));
    let server = replies(values);
    let mut p = authenticated_provider(&server.base);
    p.store = Some(SessionStore::at(dir.path().into()));
    p.persist_session().unwrap();
    let bytes = std::fs::read(dir.path().join("session.dat")).unwrap();
    let generation = p.generation.clone();
    p.handle(Operation::Homepage).unwrap();
    assert_eq!(p.generation, generation);
    assert_eq!(
        std::fs::read(dir.path().join("session.dat")).unwrap(),
        bytes
    );
    assert!(
        matches!(p.handle(Operation::Account).unwrap(),ResultData::Account(account) if account.display_name=="原账号")
    );
    server.thread.join().unwrap();
}
#[cfg(windows)]
#[test]
fn reauthorizing_another_user_does_not_reset_an_application_level_denial() {
    let mut values = vec![json!({"ret":10200})];
    values.extend(login_replies(
        json!({"ret":0,"user_info":{"nickname":"账号"}}),
    ));
    let server = replies(values);
    let mut p = authenticated_provider(&server.base);
    let error = p
        .account_playlist_feed(Instant::now() + Duration::from_secs(3))
        .err()
        .unwrap();
    assert_eq!(error.code, "APP_PERMISSION_DENIED");
    p.handle(Operation::Logout).unwrap();
    assert!(p.homepage_denial.is_some());
    assert!(matches!(
        finish_login(&mut p).unwrap(),
        ResultData::LoginState(LoginState::Authorized(_))
    ));
    assert_eq!(
        p.account_playlist_feed(Instant::now() + Duration::from_secs(3))
            .err()
            .unwrap()
            .code,
        "APP_PERMISSION_DENIED"
    );
    assert_eq!(p.session.as_ref().unwrap().access, "login-access");
    server.thread.join().unwrap();
    assert_eq!(server.requests.lock().unwrap().len(), 5);
}
