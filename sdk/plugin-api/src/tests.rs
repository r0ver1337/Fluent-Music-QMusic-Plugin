#![allow(non_snake_case)]
use super::*;
use serde_json::Value;

#[test]
fn rpcRoundtrip() {
    let request = Request {
        api_version: 1,
        id: 7,
        operation: Operation::Search {
            query: "汉字".into(),
            page: 0,
        },
    };
    let value: Value = serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    assert_eq!(value["method"], "search");
    assert_eq!(value["params"]["page"], 0);
    assert!(serde_json::from_value::<Request>(value).is_ok());
}

#[test]
fn framesAreBounded() {
    assert!(read_frame(&mut std::io::Cursor::new(vec![b'x'; MAX_MESSAGE_BYTES + 1])).is_err());
    assert!(read_frame(&mut std::io::Cursor::new(b"{}".to_vec())).is_err());
    assert_eq!(
        read_frame(&mut std::io::Cursor::new(b"{}\n".to_vec()))
            .unwrap()
            .unwrap(),
        b"{}\n"
    );
}

#[test]
fn typedResponsesAreChecked() {
    assert!(Operation::Logout.accepts(&ResultData::Ok));
    assert!(!Operation::LoginBegin.accepts(&ResultData::Ok));
    assert!(Operation::Homepage.accepts(&ResultData::RecommendationFeed(
        RecommendationFeed::default()
    )));
    assert!(
        !Operation::Recommendations.accepts(&ResultData::RecommendationFeed(
            RecommendationFeed::default()
        ))
    );
}

#[test]
fn legacyMetadataAndDescriptionsRemainReadable() {
    let track: Track = serde_json::from_str(
        r#"{"id":"42","title":"Old song","duration_ms":null,"provider_id":"old","mid":null}"#,
    )
    .unwrap();
    assert!(track.cover_url.is_none() && track.thumbnail_url.is_none());
    let serialized = serde_json::to_value(&track).unwrap();
    assert!(serialized.get("cover_url").is_none() && serialized.get("thumbnail_url").is_none());
    let playlist: Playlist =
        serde_json::from_str(r#"{"id":"123","title":"Old","track_count":null}"#).unwrap();
    assert!(playlist.cover_url.is_none() && playlist.subtitle.is_none());
    let info: PluginInfo = serde_json::from_str(r#"{"id":"old","name":"Old","version":"1","api_version":1,"capabilities":["recommendations"]}"#).unwrap();
    assert!(info.supports(&Capability::Recommendations));
    assert!(!info.supports(&Capability::Homepage));
    let feed: RecommendationFeed = serde_json::from_str(
        r#"{"playlists":[],"tracks":[],"personalized":true,"notice":"legacy"}"#,
    )
    .unwrap();
    assert_eq!(feed.playlist_source, RecommendationSource::Unknown);
    assert_eq!(feed.track_source, RecommendationSource::Unknown);
    assert!(feed.issues.is_empty());
}

#[test]
fn songArtworkIsOptionalAndRoundtripsWithoutChangingApiVersion() {
    let track: Track = serde_json::from_value(serde_json::json!({"id":"42","title":"Song","provider_id":"qqmusic","cover_url":"https://y.gtimg.cn/album.jpg","thumbnail_url":"https://y.gtimg.cn/small.jpg"})).unwrap();
    let request = Request {
        api_version: API_VERSION,
        id: 1,
        operation: Operation::Resolve {
            track: track.clone(),
        },
    };
    let parsed: Request = serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    assert_eq!(parsed.api_version, API_VERSION);
    let Operation::Resolve { track: returned } = parsed.operation else {
        panic!("resolve")
    };
    assert_eq!(track, returned);
    assert_eq!(
        track.thumbnail_url.as_deref(),
        Some("https://y.gtimg.cn/small.jpg")
    );
}

#[test]
fn accountAvatarIsOptionalAndRoundtripsInLoginAndAccountReplies() {
    for json in [
        r#"{"display_name":"旧昵称"}"#,
        r#"{"display_name":"旧昵称","avatar_url":null}"#,
    ] {
        let account: Account = serde_json::from_str(json).unwrap();
        assert!(account.avatar_url.is_none());
        assert_eq!(
            serde_json::to_value(account).unwrap(),
            serde_json::json!({"display_name":"旧昵称"})
        );
    }
    for result in [
        ResultData::Account(Account {
            display_name: "真实昵称".into(),
            avatar_url: Some("https://qlogo.cn/avatar.png".into()),
        }),
        ResultData::LoginState(LoginState::Authorized(Account {
            display_name: "真实昵称".into(),
            avatar_url: Some("https://qlogo.cn/avatar.png".into()),
        })),
    ] {
        let json = serde_json::to_string(&result).unwrap();
        let parsed: ResultData = serde_json::from_str(&json).unwrap();
        let account = match parsed {
            ResultData::Account(account)
            | ResultData::LoginState(LoginState::Authorized(account)) => account,
            _ => panic!("expected account"),
        };
        assert_eq!(account.display_name, "真实昵称");
        assert_eq!(
            account.avatar_url.as_deref(),
            Some("https://qlogo.cn/avatar.png")
        );
    }
}
