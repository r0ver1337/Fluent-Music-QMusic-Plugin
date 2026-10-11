use super::*;
fn policy(entries: &[&str]) -> NetworkPolicy {
    NetworkPolicy::new(&PluginManifest {
        Network: entries.iter().map(|entry| (*entry).into()).collect(),
    })
    .unwrap()
}
#[test]
fn originsMatchProtocolHostPortAndEveryPathWithoutMatchingNeighbors() {
    let policy = policy(&["https://api.example.com", "http://localhost:1234"]);
    for url in [
        "https://api.example.com",
        "https://API.EXAMPLE.COM:443/a?token=fixture",
        "http://localhost:1234/a",
    ] {
        assert!(policy.check(url).is_ok(), "{url}");
    }
    for url in [
        "http://api.example.com/a",
        "https://api.example.com:444/a",
        "https://api.example.com.evil.test/a",
        "https://evil.test/api.example.com",
        "https://other.api.example.com/a",
        "http://localhost/a",
        "https://user:password@api.example.com/a",
        "file:///a",
        "https://api.example.com/a#fragment",
        "https://api.example.com./a",
        " https://api.example.com",
        "https://api.example.com\\@evil.test/a",
    ] {
        assert!(policy.check(url).is_err(), "{url}");
    }
}
#[test]
fn explicitSubdomainWildcardNeverIncludesTheRootOrLookalikeDomain() {
    let policy = policy(&["https://*.music.126.net"]);
    for url in [
        "https://p1.music.126.net/a.jpg",
        "https://a.b.music.126.net/a",
    ] {
        assert!(policy.check(url).is_ok());
    }
    for url in [
        "https://music.126.net/a",
        "https://evilmusic.126.net/a",
        "https://music.126.net.evil.test/a",
        "http://p1.music.126.net/a",
    ] {
        assert!(policy.check(url).is_err());
    }
}
#[test]
fn invalidDeclarationsFailClosedAndMissingNetworkMeansNoAccess() {
    for entry in [
        "*",
        "https://*",
        "https://*.com",
        "https://example.com/a",
        "https://example.com?x=1",
        "https://example.com#x",
        "ftp://example.com",
        "https://user@example.com",
        "https://*.127.0.0.1",
        "https://ex*ample.com",
        "https://example.com\\a",
        "https://example.com.",
    ] {
        assert!(
            NetworkPolicy::new(&PluginManifest {
                Network: vec![entry.into()]
            })
            .is_err(),
            "{entry}"
        );
    }
    let manifest: PluginManifest = serde_json::from_str("{}").unwrap();
    assert!(
        NetworkPolicy::new(&manifest)
            .unwrap()
            .check("https://example.com")
            .is_err()
    );
    assert!(
        serde_json::from_str::<PluginManifest>(r#"{"network":"https://example.com"}"#).is_err()
    );
}
#[test]
fn undeclaredMediaAndImagesCannotBeDelegatedToThePlayer() {
    let policy = policy(&["https://allowed.example.com"]);
    let media = ResultData::Media(fluent_plugin_api::MediaSource::Http {
        url: "https://denied.example.com/audio".into(),
        headers: Default::default(),
    });
    assert_eq!(policy.checkResult(&media).unwrap_err().code, "NETWORK");
    let account = ResultData::Account(fluent_plugin_api::Account {
        display_name: "fixture".into(),
        avatar_url: Some("https://denied.example.com/avatar".into()),
    });
    assert!(policy.checkResult(&account).is_err());
}
