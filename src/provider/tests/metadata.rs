use super::*;

#[test]
#[allow(non_snake_case)]
fn accountAndPublicDurationsUseSecondsWithoutInventingMissingMetadata() {
    let base = json!({"song_id":42,"song_name":"合唱歌曲"});
    for key in ["song_play_time", "interval"] {
        for value in [json!(192), json!("192")] {
            let mut song = base.clone();
            song[key] = value;
            assert_eq!(track(&song).unwrap().duration_ms, Some(192000));
        }
        for value in [
            Value::Null,
            json!(0),
            json!(-1),
            json!("invalid"),
            json!(1.5),
            json!(u64::MAX),
        ] {
            let mut song = base.clone();
            song[key] = value;
            assert_eq!(track(&song).unwrap().duration_ms, None);
        }
    }
    assert_eq!(track(&base).unwrap().duration_ms, None);
    let mut song = base;
    song["interval"] = json!(0);
    song["song_play_time"] = json!(192);
    assert_eq!(track(&song).unwrap().duration_ms, Some(192000));
    song["interval"] = json!(200);
    assert_eq!(track(&song).unwrap().duration_ms, Some(200000));
}

#[test]
#[allow(non_snake_case)]
fn primaryAndAdditionalArtistsKeepOrderAndIgnoreEmptyOrDuplicateNames() {
    let song = json!({"song_id":42,"song_name":"合唱歌曲", "singer_name":"徐良",
        "other_singer_list":[{"singer_name":"汪苏泷"},{"name":"合唱团"},{"singer_name":"徐良"},
            {"singer_name":" 汪苏泷\n"},{"name":" "},{"singer_name":"\u{0000}"},null,42]});
    assert_eq!(track(&song).unwrap().artists, ["徐良", "汪苏泷", "合唱团"]);
    for singer in [
        json!([]),
        json!([null, {"name":" "}]),
        json!(["徐良", "汪苏泷"]),
        json!([{"name":"徐良"}, {"singer_name":"汪苏泷"}]),
    ] {
        let mut song = song.clone();
        song["singer"] = singer;
        assert_eq!(track(&song).unwrap().artists, ["徐良", "汪苏泷", "合唱团"]);
    }
}

#[test]
#[allow(non_snake_case)]
fn artistNamesAreNotSplitOnPunctuationAndOtherArtistsSurviveWithoutAPrimary() {
    let song = json!({"song_id":42,"song_name":"歌曲","singer_name":"AC/DC"});
    assert_eq!(track(&song).unwrap().artists, ["AC/DC"]);
    let song = json!({"song_id":42,"song_name":"歌曲","singer_name":" ",
        "other_singer_list":[{"singer_name":"第二歌手"}]});
    assert_eq!(track(&song).unwrap().artists, ["第二歌手"]);
}

#[test]
#[allow(non_snake_case)]
fn libraryPlaylistPagesPreserveDurationAllArtistsAndPrivatePlaybackTokens() {
    use super::playlist_reentry::{PlaylistServer, read_page};
    let server = PlaylistServer::new(2, |index, request| {
        assert_eq!(request["opi_cmd"], DETAIL);
        json!({"ret":0,"song_list":[{"song_id":42 + index,"song_name":"合唱歌曲",
            "song_play_time":"192","singer_name":"徐良","other_singer_list":[{"singer_name":"汪苏泷"}],
            "song_token":"synthetic-song-token"}],"has_more":index == 0})
    });
    let mut provider = authenticated_provider(&server.base);
    for index in 0..2 {
        let page = read_page(&mut provider, "123", index, None).unwrap();
        assert_eq!(page.page, index);
        assert_eq!(page.items[0].duration_ms, Some(192000));
        assert_eq!(page.items[0].artists, ["徐良", "汪苏泷"]);
        assert_eq!(provider.tokens[&page.items[0].id], "synthetic-song-token");
        assert!(
            !serde_json::to_string(&page)
                .unwrap()
                .contains("synthetic-song-token")
        );
    }
    server.thread.join().unwrap();
}

#[test]
#[allow(non_snake_case)]
fn recommendedPlaylistStillUsesPublicMetadataWithAllArtists() {
    let server = replies(vec![json!({"code":0,"req_0":{"code":0,"data":{"code":0,
        "dirinfo":{"id":123},"total_song_num":1,"songlist":[{"id":42,"title":"合唱歌曲", "interval":192,
            "singer":[{"name":"徐良"},{"name":"汪苏泷"}]}]}}})]);
    let mut provider = authenticated_provider(&server.base);
    provider.public_playlists.insert("123".into());
    let page = super::playlist_reentry::read_page(&mut provider, "123", 0, None).unwrap();
    assert_eq!(page.items[0].duration_ms, Some(192000));
    assert_eq!(page.items[0].artists, ["徐良", "汪苏泷"]);
    server.thread.join().unwrap();
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}
