use super::*;

#[test]
fn partial_homepage_keeps_valid_playlists_big_cover_and_embedded_songs_when_radio_fails() {
    let values = vec![
        json!({"ret":0,"rc_item_arr":[
            {"dissid":"123","dirName":"有效歌单","big_cover":"https://qpic.y.qq.com/big"},
            {"dissid":"bad","dirName":"broken","song_token":"secret"}
        ],"shelf_arr":[{"card_arr":[
            {"id":"not-a-playlist","type":200,"songInfo":{"song_id":42,"song_name":"首页歌曲","song_token":"secret"}}
        ]}]}),
        json!({"ret":-1,"msg":"raw sensitive error"}),
    ];
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
    assert_eq!(feed.playlists.len(), 1);
    assert_eq!(
        feed.playlists[0].cover_url.as_deref(),
        Some("https://qpic.y.qq.com/big")
    );
    assert_eq!(feed.playlist_source, RecommendationSource::Account);
    assert_eq!(feed.track_source, RecommendationSource::Homepage);
    assert_eq!(feed.tracks[0].id, "42");
    assert!(!feed.personalized);
    assert_eq!(
        feed.issues
            .iter()
            .map(|i| i.error.code.as_str())
            .collect::<Vec<_>>(),
        vec!["SCHEMA", "SERVER"]
    );
    let text = serde_json::to_string(&feed).unwrap();
    for secret in ["secret", "raw sensitive error", "not-a-playlist"] {
        assert!(!text.contains(secret));
    }
    server.thread.join().unwrap();
    assert_eq!(server.requests.lock().unwrap().len(), 2);
}
#[test]
fn configured_homepage_types_are_sent_directly_without_an_algorithm_probe() {
    let values = vec![
        json!({"ret":0,"rc_item_arr":[{"dissid":123,"dirName":"首页歌单"}]}),
        json!({"ret":0,"songlist":[{"song_id":42,"song_name":"账号歌曲"}]}),
    ];
    let server = discovery::testing::Server::new(
        values
            .into_iter()
            .map(|v| ("200 OK", serde_json::to_vec(&v).unwrap(), ""))
            .collect(),
    );
    let mut p = authenticated_provider(&server.base);
    p.client.as_mut().unwrap().config.homepage_types = "100,200".into();
    let ResultData::RecommendationFeed(feed) = p.handle(Operation::Homepage).unwrap() else {
        panic!("feed");
    };
    assert_eq!(feed.playlist_source, RecommendationSource::Account);
    assert_eq!(feed.track_source, RecommendationSource::Account);
    assert!(feed.issues.is_empty());
    assert!(feed.notice.unwrap().contains("OpenQQMusic 路径"));
    server.thread.join().unwrap();
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].contains(HOMEPAGE));
    assert!(requests[0].contains("type=100%2C200") && requests[0].contains("need_songinfo=1"));
    assert!(!requests.iter().any(|r| r.contains("GetAlgorithmPlConf")));
}
#[test]
fn authorized_openqqmusic_homepage_and_public_fallback_are_distinguished_without_leaking_tokens() {
    for fallback in [false, true] {
        let opi = if fallback {
            json!({"ret":-1})
        } else {
            json!({"ret":0,"rc_item_arr":[
                {"dissid":"123","dirName":"账号首页歌单","albumPicUrl":"https://qpic.y.qq.com/a"}
            ]})
        };
        let mut replies = vec![opi];
        if fallback {
            replies
                .push(json!({"code":0,"data":{"list":[{"dissid":"123","dissname":"公开歌单"}]}}));
            replies.push(json!({"code":0,"req_0":{"code":0,"data":{"code":0,"dirinfo":{"id":123},"songlist":[]}}}));
        }
        replies.push(json!({"ret":0,"songlist":[{"song_id":"42","song_name":"个性歌曲","song_token":"private-song-token"}]}));
        let server = discovery::testing::Server::new(
            replies
                .into_iter()
                .map(|v| ("200 OK", serde_json::to_vec(&v).unwrap(), ""))
                .collect(),
        );
        let mut p = authenticated_provider(&server.base);
        let ResultData::RecommendationFeed(feed) =
            p.handle(Operation::PlaylistRecommendations).unwrap()
        else {
            panic!("feed expected");
        };
        assert!(feed.personalized);
        assert_eq!(
            feed.playlists[0].title,
            if fallback {
                "公开歌单"
            } else {
                "账号首页歌单"
            }
        );
        assert_eq!(feed.tracks[0].title, "个性歌曲");
        assert_eq!(
            feed.playlist_source,
            if fallback {
                RecommendationSource::Public
            } else {
                RecommendationSource::Account
            }
        );
        assert_eq!(feed.track_source, RecommendationSource::Account);
        assert_eq!(feed.issues.len(), usize::from(fallback));
        if fallback {
            assert_eq!(feed.issues[0].stage, "account_playlists");
            assert_eq!(feed.issues[0].error.code, "SERVER");
        }
        let serialized = serde_json::to_string(&feed).unwrap();
        assert!(
            !serialized.contains("private-song-token") && !serialized.contains("synthetic-access")
        );
        assert_eq!(p.tokens["42"], "private-song-token");
        server.thread.join().unwrap();
        let requests = server.requests.lock().unwrap();
        assert!(requests[0].starts_with("GET /opi?"));
        let query = requests[0]
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .split_once('?')
            .unwrap()
            .1;
        let params = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect::<std::collections::BTreeMap<String, String>>();
        assert_eq!(params["opi_cmd"], HOMEPAGE);
        assert_eq!(params["sn"], "devicewithhyphens");
        assert_eq!(params["type"], "0"); // Old blank .env values still use OpenQQMusic's default.
        assert_eq!(params["need_songinfo"], "1");
        assert_eq!(params["qqmusic_open_id"], "synthetic-open-id");
        assert_eq!(params["qqmusic_access_token"], "synthetic-access");
        assert!(
            !requests
                .iter()
                .any(|r| r.contains("GetAlgorithmPlConf") || r.contains("aiPlayList"))
        );
        if fallback {
            for request in &requests[1..3] {
                assert!(!request.contains("synthetic-access") && !request.contains("app_key"));
            }
        }
    }
}
#[test]
fn successful_homepage_playlists_survive_empty_personal_song_response() {
    let replies = vec![
        json!({"ret":0,"rc_item_arr":[{"dissid":456,"dirName":"首页歌单"}],"shelf_arr":[]}),
        json!({"ret":0,"songlist":[]}),
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
    assert_eq!(feed.playlist_source, RecommendationSource::Account);
    assert_eq!(feed.playlists[0].title, "首页歌单");
    assert_eq!(feed.issues.len(), 1);
    assert_eq!(feed.issues[0].stage, "account_tracks");
    assert_eq!(feed.issues[0].error.code, "EMPTY");
    assert!(!feed.personalized);
    assert!(feed.notice.as_ref().unwrap().contains("由服务端决定内容"));
    server.thread.join().unwrap();
    assert!(server.requests.lock().unwrap()[0].contains("type=0"));
}
#[test]
fn identical_public_and_account_ids_are_not_proof_of_fallback_and_route_as_account() {
    let replies = vec![
        json!({"ret":0,"rc_item_arr":[{"dissid":"123","dirName":"账号结果"}]}),
        json!({"ret":0,"songlist":[{"song_id":42,"song_name":"账号歌曲"}]}),
        json!({"ret":0,"song_list":[]}),
    ];
    let server = discovery::testing::Server::new(
        replies
            .into_iter()
            .map(|v| ("200 OK", serde_json::to_vec(&v).unwrap(), ""))
            .collect(),
    );
    let mut p = authenticated_provider(&server.base);
    p.public_playlists.insert("123".into());
    let ResultData::RecommendationFeed(feed) = p.handle(Operation::Homepage).unwrap() else {
        panic!("feed");
    };
    assert_eq!(feed.playlist_source, RecommendationSource::Account);
    assert!(!p.public_playlists.contains("123"));
    p.handle(Operation::Playlist {
        id: "123".into(),
        page: 0,
        cursor: None,
    })
    .unwrap();
    server.thread.join().unwrap();
    assert!(server.requests.lock().unwrap()[2].contains(DETAIL));
}
