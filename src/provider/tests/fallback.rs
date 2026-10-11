use super::*;

#[test]
fn refresh_failure_is_not_hidden_by_public_recommendations() {
    let replies = vec![
        json!({"ret":-1,"msg":"secret refresh message"}),
        json!({"code":0,"data":{"list":[{"dissid":"123","dissname":"公开歌单"}]}}),
        json!({"code":0,"req_0":{"code":0,"data":{"code":0,"dirinfo":{"id":123},"songlist":[]}}}),
    ];
    let server = discovery::testing::Server::new(
        replies
            .into_iter()
            .map(|v| ("200 OK", serde_json::to_vec(&v).unwrap(), ""))
            .collect(),
    );
    let mut p = authenticated_provider(&server.base);
    p.session.as_mut().unwrap().expires = 0;
    let ResultData::RecommendationFeed(feed) = p.handle(Operation::Homepage).unwrap() else {
        panic!("feed");
    };
    assert_eq!(feed.playlist_source, RecommendationSource::Public);
    assert_eq!(feed.issues[0].stage, "session");
    assert!(feed.notice.as_ref().unwrap().contains("请重新授权"));
    assert!(!serde_json::to_string(&feed).unwrap().contains("secret"));
    server.thread.join().unwrap();
    let requests = server.requests.lock().unwrap();
    assert!(requests[0].contains("cmd=refreshToken"));
    assert!(!requests.iter().any(|r| r.contains("GetAlgorithmPlConf")));
}
#[test]
fn public_failure_does_not_discard_successful_personal_tracks() {
    let replies = vec![
        json!({"code":-1}),
        json!({"code":403}),
        json!({"ret":0,"songlist":[{"song_id":42,"song_name":"账号歌曲"}]}),
    ];
    let server = discovery::testing::Server::new(
        replies
            .into_iter()
            .map(|v| ("200 OK", serde_json::to_vec(&v).unwrap(), ""))
            .collect(),
    );
    let mut p = authenticated_provider(&server.base);
    let ResultData::RecommendationFeed(feed) = p.handle(Operation::Homepage).unwrap() else {
        panic!("feed");
    };
    assert!(feed.playlists.is_empty());
    assert_eq!(feed.track_source, RecommendationSource::Account);
    assert!(feed.personalized);
    assert_eq!(feed.issues.len(), 2);
    server.thread.join().unwrap();
}
#[test]
fn malformed_homepage_and_song_errors_keep_public_source_and_both_reasons() {
    let replies = vec![
        json!({"ret":0,"rc_item_arr":[{"dissid":"bad","dirName":"broken"}]}),
        json!({"code":0,"data":{"list":[{"dissid":"123","dissname":"公开歌单"}]}}),
        json!({"code":0,"req_0":{"code":0,"data":{"code":0,"dirinfo":{"id":123},"songlist":[{"id":42,"title":"公开歌曲"}]}}}),
        json!({"ret":-55,"msg":"private-server-text-must-not-leak"}),
    ];
    let server = discovery::testing::Server::new(
        replies
            .into_iter()
            .map(|v| ("200 OK", serde_json::to_vec(&v).unwrap(), ""))
            .collect(),
    );
    let mut p = authenticated_provider(&server.base);
    let ResultData::RecommendationFeed(feed) = p.handle(Operation::Homepage).unwrap() else {
        panic!("feed");
    };
    assert!(!feed.personalized);
    assert_eq!(feed.playlist_source, RecommendationSource::Public);
    assert_eq!(feed.track_source, RecommendationSource::Public);
    assert_eq!(
        feed.issues
            .iter()
            .map(|i| i.error.code.as_str())
            .collect::<Vec<_>>(),
        vec!["SCHEMA", "SERVER"]
    );
    assert!(feed.issues[1].error.message.contains("ret=-55"));
    assert!(
        !serde_json::to_string(&feed)
            .unwrap()
            .contains("private-server-text")
    );
    server.thread.join().unwrap();
}
#[test]
fn guest_feed_and_details_use_no_session_or_application_credentials() {
    let plaza = json!({"code":0,"data":{"list":[{"dissid":"123","dissname":"公开歌单","imgurl":"https://qpic.y.qq.com/cover"}]}});
    let detail = json!({"code":0,"req_0":{"code":0,"data":{"code":0,"dirinfo":{"id":123},"songlist":[
        {"id":42,"title":"公开歌曲","singer":[{"name":"歌手"}],"album":{"name":"专辑"}}
    ],"total_song_num":1}}});
    let replies = vec![plaza.clone(), detail.clone(), plaza, detail.clone(), detail];
    let server = discovery::testing::Server::new(
        replies
            .into_iter()
            .map(|v| ("200 OK", serde_json::to_vec(&v).unwrap(), ""))
            .collect(),
    );
    let mut p = Provider::new();
    p.signed_homepage_enabled = false;
    p.public = Some(discovery::PublicClient::test_client(&server.base));
    for operation in [Operation::Homepage, Operation::PlaylistRecommendations] {
        let ResultData::RecommendationFeed(feed) = p.handle(operation).unwrap() else {
            panic!("expected feed");
        };
        assert_eq!(feed.playlists[0].title, "公开歌单");
        assert_eq!(feed.tracks[0].title, "公开歌曲");
        assert!(!feed.personalized);
        assert!(p.client.is_none());
    }
    assert!(matches!(
        p.handle(Operation::Playlist {
            id: "123".into(),
            page: 0,
            cursor: None
        })
        .unwrap(),
        ResultData::Tracks(_)
    ));
    let t = track(&json!({"id":42,"title":"公开歌曲"})).unwrap();
    for op in [
        Operation::Resolve { track: t },
        Operation::Playlists,
        Operation::Account,
        Operation::Playlist {
            id: "0".into(),
            page: 0,
            cursor: None,
        },
    ] {
        assert_eq!(p.handle(op).err().unwrap().code, "LOGIN_REQUIRED");
    }
    server.thread.join().unwrap();
    for request in server.requests.lock().unwrap().iter() {
        for secret in [
            "access_token",
            "app_key",
            "app_id",
            "cookie:",
            "authorization:",
            "song_token",
        ] {
            assert!(!request.contains(secret));
        }
    }
}
#[test]
fn logout_erases_private_metadata_and_public_routing_state() {
    let mut p = Provider::new();
    p.tokens.insert("1".into(), "secret".into());
    p.updates.insert("1".into(), 42);
    p.public_playlists.insert("123".into());
    p.handle(Operation::Logout).unwrap();
    assert!(p.tokens.is_empty() && p.updates.is_empty() && p.public_playlists.is_empty());
}
