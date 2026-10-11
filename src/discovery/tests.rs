use super::*;
#[test]
fn opi_homepage_uses_openqqmusic_fields_and_retains_valid_entries() {
    let feed=opi_feed(&json!({"rc_item_arr":[
        {"dissid":"9007199254740993","dirName":"账号推荐","albumPicUrl":"https://qpic.y.qq.com/main","big_cover":"https://qpic.y.qq.com/not-main","extra":"sensitive","song_token":"secret"},
        {"dissid":123,"dirName":"备用封面","big_cover":"http://p.qpic.cn/a"},
        {"dissid":"9007199254740993","dirName":"duplicate"},
        {"dissid":"bad-id","dirName":"broken"}
    ],"shelf_arr":[{"card_arr":[{"songInfo":{"song_id":42,"song_name":"账号歌曲", "album_mid":"AlbumMid"}}]}]})).unwrap();
    assert_eq!(
        feed.tracks[0].cover_url.as_deref(),
        Some("https://y.gtimg.cn/music/photo_new/T002R800x800M000AlbumMid.jpg")
    );
    assert_eq!(
        feed.tracks[0].thumbnail_url.as_deref(),
        Some("https://y.gtimg.cn/music/photo_new/T002R90x90M000AlbumMid.jpg")
    );
    assert_eq!(feed.playlist_source, RecommendationSource::Unknown); // Caller assigns verified request source.
    assert_eq!(feed.playlists.len(), 2);
    assert_eq!(feed.playlists[0].id, "9007199254740993");
    assert_eq!(
        feed.playlists[0].cover_url.as_deref(),
        Some("https://qpic.y.qq.com/main")
    );
    assert_eq!(feed.playlists[1].title, "备用封面");
    assert_eq!(
        feed.playlists[1].cover_url.as_deref(),
        Some("https://p.qpic.cn/a")
    );
    assert_eq!(feed.issues[0].error.code, "SCHEMA");
    let serialized = serde_json::to_string(&feed).unwrap();
    for secret in ["sensitive", "secret", "not-main"] {
        assert!(!serialized.contains(secret));
    }
    assert!(!feed.personalized); // Only the song API sets the legacy song marker.
    assert_eq!(
        opi_feed(&json!({"rc_item_arr":[]}))
            .unwrap()
            .playlists
            .len(),
        0
    );
    assert_eq!(
        opi_feed(&json!({"rc_item_arr":[{"dissid":"0","dirName":"invalid"}]}))
            .err()
            .unwrap()
            .code,
        "SCHEMA"
    );
    assert_eq!(
        opi_feed(&json!({"data":{"rc_item_arr":[]}}))
            .err()
            .unwrap()
            .code,
        "SCHEMA"
    );
}
#[test]
fn public_playlists_keep_string_ids_and_do_not_invent_counts() {
    let p = parse_plaza(&json!({"code":0,"data":{"list":[{
        "dissid":"9007199254740993","dissname":"中文歌单","imgurl":"http://qpic.y.qq.com/a.jpg",
        "listennum":10000,"creator":{"name":"artist"}
    }]}}))
    .unwrap();
    assert_eq!(p[0].id, "9007199254740993");
    assert_eq!(
        p[0].cover_url.as_deref(),
        Some("https://qpic.y.qq.com/a.jpg")
    );
    assert!(p[0].track_count.is_none());
    assert!(!serde_json::to_string(&p).unwrap().contains("creator"));
}
#[test]
fn guest_detail_is_metadata_only_and_checks_all_statuses() {
    let mut v = json!({"code":0,"req_0":{"code":0,"data":{"code":0,"dirinfo":{"id":123},
        "total_song_num":51,"songlist":[{"id":42,"mid":"mid42","title":"歌曲", "interval":120,
            "singer":[{"name":"歌手"}],"album":{"name":"专辑","mid":"AlbumMid"},"url":"secret", "song_token":"token"}]}}});
    let p = parse_detail(&v, "123", 0).unwrap();
    assert!(p.has_more);
    assert_eq!(p.items[0].artists, vec!["歌手"]);
    assert_eq!(p.items[0].duration_ms, Some(120000));
    assert_eq!(p.items[0].mid.as_deref(), Some("mid42"));
    assert_eq!(
        p.items[0].cover_url.as_deref(),
        Some("https://y.gtimg.cn/music/photo_new/T002R800x800M000AlbumMid.jpg")
    );
    assert!(!serde_json::to_string(&p).unwrap().contains("secret"));
    assert!(parse_detail(&v, "456", 0).is_err());
    v["req_0"]["code"] = json!(-1);
    assert!(parse_detail(&v, "123", 0).is_err());
    v["req_0"]["code"] = json!(0);
    v["req_0"]["data"]["subcode"] = json!(-1);
    assert!(parse_detail(&v, "123", 0).is_err());
    assert!(detail_request("0", 0).is_err());
    assert!(detail_request("123", u32::MAX).is_err());
    let body = detail_request("123", 1).unwrap();
    assert_eq!(body["req_0"]["param"]["song_begin"], 50);
    assert!(!body.to_string().contains("token"));
}
#[test]
fn unknown_cards_are_not_playlists() {
    let feed = opi_feed(&json!({"rc_item_arr":[{"dissid":123,"dirName":"歌单","albumPicUrl":"https://qpic.y.qq.com/a"}],
        "shelf_arr":[{"card_arr":[{"id":"album-id","type":200,"title":"专辑","cover":"file:///secret"}]}]})).unwrap();
    assert_eq!(feed.playlists.len(), 1);
    assert!(!feed.personalized);
    assert!(
        opi_feed(&json!({"shelf_arr":[]}))
            .unwrap()
            .playlists
            .is_empty()
    );
}
#[test]
fn cover_urls_do_not_leak_credentials_or_allow_arbitrary_hosts() {
    for value in [
        "file:///a",
        "https://qq.com.evil.test/a",
        "https://u:p@qpic.y.qq.com/a",
        "https://127.0.0.1/a",
    ] {
        assert!(cover_url(Some(value)).is_none());
    }
}
#[test]
fn public_transport_rejects_redirects_oversize_and_server_denial() {
    for (status, body, extra, code) in [
        (
            "302 Found",
            vec![],
            "Location: https://example.invalid/secret\r\n",
            "HTTP",
        ),
        ("200 OK", vec![b'x'; LIMIT as usize + 1], "", "SCHEMA"),
        (
            "200 OK",
            br#"{"code":403,"msg":"raw sensitive error"}"#.to_vec(),
            "",
            "SERVER",
        ),
        ("200 OK", b"not json".to_vec(), "", "SCHEMA"),
    ] {
        let server = testing::Server::new(vec![(status, body, extra)]);
        let client = PublicClient::test_client(&server.base);
        let e = client.playlists().err().unwrap();
        assert_eq!(e.code, code);
        assert!(!e.message.contains("secret") && !e.message.contains("sensitive"));
        server.thread.join().unwrap();
        assert_eq!(server.requests.lock().unwrap().len(), 1);
    }
}
