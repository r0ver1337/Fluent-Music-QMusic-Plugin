use super::*;

fn envelope(name: &str, data: Value) -> Value {
    json!({"code":0,name:{"code":0,"data":data}})
}

fn session_reply() -> Value {
    envelope(
        "music.getSession.session.GetSession",
        json!({"session":{"uid":12345,"sid":"synthetic-sid","userip":"192.0.2.1"}}),
    )
}

fn content_replies() -> [Value; 2] {
    [
        envelope(
            "music.qqmusiclite.MtHomePageSvr.GetRecommendFullPage",
            json!({"code":0,"v_shelf":[{"v_niche":[{"v_card":[{"jumptype":10046,"id":"42","title":"签名首页歌曲"}]}]}]}),
        ),
        envelope(
            "music.playlist.PlaylistSquare.GetRecommendWhole",
            json!({"HeadRsp":{"List":[{"Playlist":{"basic":{"tid":123,"title":"签名推荐歌单"}}}]},"FeedRsp":{"List":[]}}),
        ),
    ]
}

#[test]
fn all_recommendation_operations_default_to_signed_homepage_without_options() {
    let server = replies(
        std::iter::once(session_reply())
            .chain((0..3).flat_map(|_| content_replies()))
            .collect(),
    );
    let mut provider = Provider::new();
    provider.public = Some(discovery::PublicClient::test_client(&server.base));
    provider.recommendation_client = Some(
        crate::recommendation::RecommendationClient::test_client(&server.base),
    );
    assert!(provider.uses_signed_homepage());
    for operation in [
        Operation::Homepage,
        Operation::PlaylistRecommendations,
        Operation::Recommendations,
    ] {
        match provider.handle(operation).unwrap() {
            ResultData::RecommendationFeed(feed) => {
                assert_eq!(feed.playlist_source, RecommendationSource::Homepage);
                assert_eq!(feed.track_source, RecommendationSource::Homepage);
                assert_eq!(feed.playlists[0].title, "签名推荐歌单");
                assert_eq!(feed.tracks[0].title, "签名首页歌曲");
                assert!(!feed.personalized);
            }
            ResultData::Tracks(page) => assert_eq!(page.items[0].title, "签名首页歌曲"),
            _ => panic!("Unexpected recommendation reply"),
        }
    }
    assert!(provider.client.is_none());
    assert!(provider.public_playlists.contains("123"));
    server.thread.join().unwrap();
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 7);
    assert!(requests[0].starts_with("POST /normal "));
    assert!(
        requests[1..]
            .iter()
            .all(|request| request.starts_with("POST /safe "))
    );
}

#[test]
fn default_signed_homepage_preserves_the_existing_verified_openid_account_path() {
    let server = replies(
        std::iter::once(session_reply())
            .chain(std::iter::once(envelope(
                "music.UserInfo.userInfoServer.GetLoginUserInfo",
                json!({"info":{"nickname":"合成账号"}}),
            )))
            .chain(content_replies())
            .collect(),
    );
    let mut provider = authenticated_provider(&server.base);
    provider.signed_homepage_enabled = true;
    provider.recommendation_client = Some(
        crate::recommendation::RecommendationClient::test_client(&server.base),
    );
    let ResultData::RecommendationFeed(feed) = provider.handle(Operation::Homepage).unwrap() else {
        panic!("feed")
    };
    assert_eq!(feed.playlist_source, RecommendationSource::Account);
    assert_eq!(feed.track_source, RecommendationSource::Account);
    assert!(feed.personalized);
    assert_eq!(
        provider.session.as_ref().unwrap().access,
        "synthetic-access"
    );
    assert!(provider.public_playlists.contains("123"));
    server.thread.join().unwrap();
    assert_eq!(server.requests.lock().unwrap().len(), 4);
}
