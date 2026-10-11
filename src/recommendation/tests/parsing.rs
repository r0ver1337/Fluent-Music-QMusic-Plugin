use super::*;
#[test]
fn albums_navigation_and_blank_ids_never_become_playlists_or_songs() {
    let tracks=homepage_tracks(&json!({"code":0,"v_shelf":[{"v_niche":[{"v_card":[{"jumptype":10014,"id":"1","title":"album"},{"jumptype":30001,"id":"","title":"navigation"},{"jumptype":10046,"id":"123","title":"song","mid":"SongMid", "cover":"http://y.gtimg.cn/music/photo_new/T002R150x150M000AlbumMid_1.jpg?token=must-not-leak", "extra":{"secret":"must-not-leak"}}]}]}]})).unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(
        tracks[0].cover_url.as_deref(),
        Some("https://y.gtimg.cn/music/photo_new/T002R800x800M000AlbumMid_1.jpg")
    );
    assert_eq!(
        tracks[0].thumbnail_url.as_deref(),
        Some("https://y.gtimg.cn/music/photo_new/T002R90x90M000AlbumMid_1.jpg")
    );
    assert!(
        !serde_json::to_string(&tracks)
            .unwrap()
            .contains("must-not-leak")
    );
}
#[test]
fn playlist_parser_deduplicates_and_strips_private_cover_queries() {
    let item = json!({"Playlist":{"basic":{"tid":"123","title":"list","song_cnt":2,"cover":{"medium_url":"https://y.gtimg.cn/a?private=secret"},"ext":{"token":"must-not-leak"}}}});
    let list = recommended_playlists(
        &json!({"HeadRsp":{"List":[item.clone()]},"FeedRsp":{"List":[item]}}),
    )
    .unwrap();
    assert_eq!(list.len(), 1);
    let raw = serde_json::to_string(&list).unwrap();
    assert!(!raw.contains("secret") && !raw.contains("token"));
}
#[test]
fn status_is_strict_and_does_not_echo_server_messages() {
    for v in [
        json!({}),
        json!({"code":"0"}),
        json!({"code":false}),
        json!({"code":0,"ret":1000,"msg":"private-token"}),
    ] {
        let error = check_status(&v, true).err().unwrap();
        assert!(!error.message.contains("private-token"));
    }
}
