use super::*;

#[test]
fn catalogue_needs_no_account_and_selected_resolution_does_not_change_quality() {
    let mut provider = Provider::new();
    let ResultData::TrackQualities(catalogue) = provider
        .handle(Operation::TrackQualities { track: None })
        .unwrap()
    else {
        panic!("catalogue")
    };
    assert_eq!(catalogue.Levels[0].Priority, 1);
    assert!(catalogue.Items.is_empty());
    assert!(provider.client.is_none());
    let song = json!({"song_id":42,"song_name":"测试歌曲","song_size_sq":32000000,
        "song_size_standard":4000000,"song_play_url_standard":"https://stream.qqmusic.qq.com/a.mp3"});
    let server = replies(vec![
        json!({"ret":0,"songlist":[song.clone()]}),
        json!({"ret":0,"songlist":[song.clone()]}),
        json!({"ret":0,"songlist":[song.clone()]}),
    ]);
    let mut provider = authenticated_provider(&server.base);
    let track = track(&song).unwrap();
    let ResultData::TrackQualities(list) = provider
        .handle(Operation::TrackQualities {
            track: Some(track.clone()),
        })
        .unwrap()
    else {
        panic!("list")
    };
    assert!(!list.Items[0].Playable);
    assert_eq!(list.Items[0].FileSizeBytes, Some(32000000));
    let error = provider
        .handle(Operation::ResolveWithQuality {
            track: track.clone(),
            qualityId: "sq".into(),
        })
        .err()
        .unwrap();
    assert_eq!(error.code, "QUALITY_UNAVAILABLE");
    let ResultData::Media(MediaSource::Http { url, .. }) = provider
        .handle(Operation::ResolveWithQuality {
            track,
            qualityId: "standard".into(),
        })
        .unwrap()
    else {
        panic!("media")
    };
    assert!(url.ends_with("/a.mp3"));
    server.thread.join().unwrap();
    for request in server.requests.lock().unwrap().iter() {
        assert!(request.contains(RESOLVE));
        assert!(request.contains("song_id=42"));
        assert!(!request.to_lowercase().contains("cookie:"));
    }
}

#[test]
fn wrong_song_and_missing_login_are_not_hidden_by_audio_quality_fallback() {
    let song = json!({"song_id":42,"song_name":"测试歌曲"});
    let track = track(&song).unwrap();
    let mut provider = Provider::new();
    assert_eq!(
        provider
            .handle(Operation::TrackQualities {
                track: Some(track.clone())
            })
            .err()
            .unwrap()
            .code,
        "LOGIN_REQUIRED"
    );
    let server = replies(vec![
        json!({"ret":0,"songlist":[{"song_id":99,"song_name":"不同歌曲"}]}),
    ]);
    let mut provider = authenticated_provider(&server.base);
    assert_eq!(
        provider
            .handle(Operation::TrackQualities { track: Some(track) })
            .err()
            .unwrap()
            .code,
        "SCHEMA"
    );
    server.thread.join().unwrap();
}
