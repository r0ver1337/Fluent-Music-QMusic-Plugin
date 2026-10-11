use super::*;
use super::{authenticated_provider, playlist_reentry::PlaylistServer};

fn song() -> Track {
    Track {
        id: "42".into(),
        provider_id: "qqmusic".into(),
        mid: Some("SongMid".into()),
        title: "Synthetic".into(),
        artists: vec![],
        album: String::new(),
        duration_ms: Some(5000),
        cover_url: None,
        thumbnail_url: None,
    }
}

#[test]
fn lyrics_capability_is_optional_and_describe_needs_no_credentials() {
    let ResultData::Description(info) = Provider::new().handle(Operation::Describe).unwrap() else {
        panic!("describe")
    };
    assert!(info.extensions.contains(&Capability::Lyrics));
}

#[test]
fn signed_lyrics_request_returns_standard_original_and_encrypted_translation() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/qrcVectors.json")).unwrap();
    let server = PlaylistServer::new(
        1,
        move |_, _| json!({"ret":0,"sub_ret":0,"song_qrc_enc_lyric":fixture["samples"][0]["api"],"song_translate_enc_lyric":fixture["translation"]["api"],"lyric_start_ms":100}),
    );
    let mut provider = authenticated_provider(&server.base);
    let ResultData::Lyrics(lyrics) = provider
        .handle(Operation::Lyrics { track: song() })
        .unwrap()
    else {
        panic!("lyrics")
    };
    assert_eq!(lyrics.Original.unwrap().Lines[0].Words[1].StartMs, 1500);
    assert_eq!(lyrics.Translation.unwrap().Lines[0].Text, "译文A");
    server.thread.join().unwrap();
    let requests = server.requests.lock().unwrap();
    let request = &requests[0];
    assert_eq!(request["opi_cmd"], crate::lyrics::COMMAND);
    assert_eq!(request["song_id"], "42");
    assert_eq!(request["song_mid"], "SongMid");
    assert_eq!(request["qrc_no_enc"], "1");
    assert_eq!(request["qqmusic_access_token"], "synthetic-access");
    assert_eq!(
        request["sign"],
        signed(request.clone(), "synthetic-key")["sign"]
    );
}

#[test]
fn mid_only_guest_requests_use_application_signing_without_forcing_login() {
    let server = PlaylistServer::new(1, |_, _| json!({"ret":0,"song_lyric":"[00:01.25]A"}));
    let mut provider = authenticated_provider(&server.base);
    provider.session = None;
    let mut track = song();
    track.id = "mid:OnlyMid".into();
    track.mid = None;
    let ResultData::Lyrics(lyrics) = provider.handle(Operation::Lyrics { track }).unwrap() else {
        panic!("lyrics")
    };
    assert_eq!(lyrics.Original.unwrap().Lines[0].StartMs, Some(1250));
    server.thread.join().unwrap();
    let request = &server.requests.lock().unwrap()[0];
    assert_eq!(request["song_id"], "");
    assert_eq!(request["song_mid"], "OnlyMid");
    assert!(!request.contains_key("qqmusic_access_token"));
}

#[test]
fn invalid_track_is_rejected_before_configuration_or_network() {
    for (id, mid, provider_id) in [
        ("", None, "qqmusic"),
        ("42", None, "other"),
        ("mid:A", Some("B"), "qqmusic"),
        ("-1", None, "qqmusic"),
        ("0", None, "qqmusic"),
        ("42", Some("a,b"), "qqmusic"),
    ] {
        let mut track = song();
        track.id = id.into();
        track.mid = mid.map(str::to_owned);
        track.provider_id = provider_id.into();
        let mut provider = Provider::new();
        assert!(provider.handle(Operation::Lyrics { track }).is_err());
        assert!(provider.client.is_none());
    }
}

#[test]
fn upstream_failures_never_become_no_lyrics_or_delete_the_session() {
    for response in [
        json!({"ret":10200,"errorMsg":"private-secret"}),
        json!({"ret":0,"subRet":1,"msg":"private-secret"}),
        json!({"ret":0,"song_qrc_enc_lyric":"private-secret"}),
    ] {
        let server = PlaylistServer::new(1, move |_, _| response.clone());
        let mut provider = authenticated_provider(&server.base);
        let error = provider
            .handle(Operation::Lyrics { track: song() })
            .err()
            .unwrap();
        assert!(!error.message.contains("private-secret"));
        assert!(provider.session.is_some());
        server.thread.join().unwrap();
    }
}

#[test]
fn actual_empty_response_is_successful_and_revoked_access_cannot_fetch_lyrics() {
    let server = PlaylistServer::new(
        1,
        |_, _| json!({"ret":0,"song_lyric":"","song_qrc_enc_lyric":""}),
    );
    let mut provider = authenticated_provider(&server.base);
    let ResultData::Lyrics(lyrics) = provider
        .handle(Operation::Lyrics { track: song() })
        .unwrap()
    else {
        panic!("lyrics")
    };
    assert_eq!(lyrics, Lyrics::default());
    server.thread.join().unwrap();
    let server = PlaylistServer::new(1, |_, _| json!({"ret":1}));
    let mut provider = authenticated_provider(&server.base);
    provider.session.as_mut().unwrap().expires = 0;
    assert!(
        provider
            .handle(Operation::Lyrics { track: song() })
            .is_err()
    );
    server.thread.join().unwrap();
    assert_eq!(server.requests.lock().unwrap()[0]["opi_cmd"], TOKEN);
}
