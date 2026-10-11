use super::*;

#[test]
fn song_artwork_survives_search_playlist_and_radio_responses_without_private_fields() {
    for (operation, key) in [
        (
            Operation::Search {
                query: "歌曲".into(),
                page: 0,
            },
            "list",
        ),
        (
            Operation::Playlist {
                id: "123".into(),
                page: 0,
                cursor: None,
            },
            "song_list",
        ),
        (Operation::Recommendations, "songlist"),
    ] {
        let data = json!({"ret":0,key:[{"song_id":42,"song_mid":"SongMid","song_name":"歌曲",
            "album_name":"专辑","album_pic_150x150":"https://y.gtimg.cn/music/photo_new/T002R150x150M000AlbumMid.jpg?token=private-token",
            "song_token":"private-token"}]});
        let server = discovery::testing::Server::new(vec![(
            "200 OK",
            serde_json::to_vec(&data).unwrap(),
            "",
        )]);
        let mut p = authenticated_provider(&server.base);
        let ResultData::Tracks(page) = p.handle(operation).unwrap() else {
            panic!("tracks")
        };
        assert_eq!(
            page.items[0].cover_url.as_deref(),
            Some("https://y.gtimg.cn/music/photo_new/T002R800x800M000AlbumMid.jpg")
        );
        assert_eq!(
            page.items[0].thumbnail_url.as_deref(),
            Some("https://y.gtimg.cn/music/photo_new/T002R90x90M000AlbumMid.jpg")
        );
        assert_eq!(page.items[0].mid.as_deref(), Some("SongMid"));
        assert!(
            !serde_json::to_string(&page)
                .unwrap()
                .contains("private-token")
        );
        assert_eq!(
            p.tokens.get("42").map(String::as_str),
            Some("private-token")
        );
        server.thread.join().unwrap();
        assert_eq!(server.requests.lock().unwrap().len(), 1); // No per-song metadata lookups.
    }
}
#[test]
fn account_profile_returns_verified_nickname_and_headimgurl_only() {
    let profile = account_profile(&json!({"user_info":{
        "nickname":"真实\n昵称", "headimgurl":"http://qlogo.cn/avatar.png#ignored",
        "enc_uin":"private-identity", "access_token":"private-token"
    }}))
    .unwrap();
    assert_eq!(profile.display_name, "真实昵称");
    assert_eq!(
        profile.avatar_url.as_deref(),
        Some("https://qlogo.cn/avatar.png")
    );
    let serialized = serde_json::to_string(&profile).unwrap();
    assert!(!serialized.contains("private-identity") && !serialized.contains("private-token"));
    for user in [
        json!({"nickname":"昵称"}),
        json!({"nickname":"昵称", "headimgurl":null}),
        json!({"nickname":"昵称", "headimgurl":17}),
        json!({"nickname":"昵称", "headimgurl":""}),
        json!({"nickname":"昵称", "head_img_url":"https://qlogo.cn/wrong.png"}),
    ] {
        let profile = account_profile(&json!({"user_info":user})).unwrap();
        assert_eq!(profile.display_name, "昵称");
        assert!(profile.avatar_url.is_none());
    }
    for user in [
        json!({}),
        json!({"nickname":"\n\t "}),
        json!({"nickname":12}),
    ] {
        assert_eq!(
            account_profile(&json!({"user_info":user}))
                .err()
                .unwrap()
                .code,
            "SCHEMA"
        );
    }
}
#[test]
fn account_avatar_urls_allow_only_public_qq_images_over_https() {
    for value in [
        "https://qlogo.cn/a",
        "http://thirdqq.qlogo.cn/a",
        "http://wx.qlogo.cn/a",
        "https://q.qpic.cn/a",
        "https://y.qq.com/a",
    ] {
        assert!(
            account_avatar_url(Some(value))
                .unwrap()
                .starts_with("https://")
        );
    }
    for value in [
        "file:///avatar",
        "data:image/png;base64,AA",
        "https://qlogo.cn.evil.test/a",
        "https://notqlogo.cn/a",
        "https://u:p@qlogo.cn/a",
        "https://127.0.0.1/a",
        "https://qlogo.cn:8080/a",
        "",
        "http://[::1]/a",
    ] {
        assert!(
            account_avatar_url(Some(value)).is_none(),
            "accepted {value}"
        );
    }
    assert!(account_avatar_url(Some(&format!("https://qlogo.cn/{}", "x".repeat(4096)))).is_none());
}
#[test]
fn authenticated_account_operation_includes_avatar_without_a_separate_profile_request() {
    let server = discovery::testing::Server::new(vec![(
        "200 OK",
        serde_json::to_vec(&json!({"ret":0,"user_info":{
            "nickname":"真实账号", "headimgurl":"http://thirdqq.qlogo.cn/avatar.png"
        }}))
        .unwrap(),
        "",
    )]);
    let mut p = authenticated_provider(&server.base);
    let ResultData::Account(account) = p.handle(Operation::Account).unwrap() else {
        panic!("account")
    };
    assert_eq!(account.display_name, "真实账号");
    assert_eq!(
        account.avatar_url.as_deref(),
        Some("https://thirdqq.qlogo.cn/avatar.png")
    );
    server.thread.join().unwrap();
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains(ACCOUNT));
}
#[test]
fn description_uses_short_name_without_homepage_options_network_or_config() {
    for _ in 0..2 {
        let mut p = Provider::new();
        let ResultData::Description(info) = p.handle(Operation::Describe).unwrap() else {
            panic!()
        };
        assert_eq!(info.name, "QQ音乐");
        assert!(info.supports(&Capability::Homepage));
        assert!(p.signed_homepage_enabled);
        assert!(p.recommendation_client.is_none() && p.client.is_none() && p.session.is_none());
    }
}
#[test]
fn library_hides_only_the_special_directory_and_does_not_inject_it() {
    for special_id in [json!(0), json!("0")] {
        let data = json!({"ret":0,"data":[
            {"dissid":special_id,"dissname":"我喜欢（特殊歌单）"},
            {"dissid":"123","dissname":"我喜欢"},
            {"playlistId":"456","name":"特殊歌单也是普通歌单的合法名称"}
        ]});
        let server = discovery::testing::Server::new(vec![(
            "200 OK",
            serde_json::to_vec(&data).unwrap(),
            "",
        )]);
        let mut p = authenticated_provider(&server.base);
        let ResultData::Playlists(items) = p.handle(Operation::Playlists).unwrap() else {
            panic!("library")
        };
        assert_eq!(
            items.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["123", "456"]
        );
        server.thread.join().unwrap();
    }
    assert!(library_playlists(&json!({"data":[]})).unwrap().is_empty());
    assert_eq!(
        library_playlists(&json!({"data":[{"dissid":"123"}]}))
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn metadata_excludes_tokens() {
    let t = track(
        &json!({"song_id":"90071992547409930001", "song_name":"song", "song_token":"secret"}),
    )
    .unwrap();
    assert_eq!(t.id, "90071992547409930001");
    assert!(!serde_json::to_string(&t).unwrap().contains("secret"));
}
#[test]
fn describe_works_without_credentials() {
    let mut p = Provider::new();
    assert!(matches!(
        p.handle(Operation::Describe).unwrap(),
        ResultData::Description(_)
    ));
    assert!(matches!(
        p.handle(Operation::Logout).unwrap(),
        ResultData::Ok
    ));
}
