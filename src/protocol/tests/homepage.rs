use super::*;
#[test]
fn homepage_get_matches_openqqmusic_params_and_signs_the_full_query() {
    use crate::discovery::testing::Server;
    let server = Server::new(vec![(
        "200 OK",
        br#"{"ret":0,"rc_item_arr":[]}"#.to_vec(),
        "",
    )]);
    let mut config = config();
    config.gateway = format!("{}/opi", server.base);
    config.common.insert("client_ip".into(), "192.0.2.5".into());
    let mut client = Client::test_client(config);
    let session = Session {
        open_id: "test-id".into(),
        access: "test-access 汉+&".into(),
        refresh: "test-refresh".into(),
        expires: u64::MAX,
        music_id: Some("synthetic-music-id".into()),
    };
    client
        .homepage_playlists(Some(&session), Instant::now() + Duration::from_secs(3))
        .unwrap();
    server.thread.join().unwrap();
    let requests = server.requests.lock().unwrap();
    let request = &requests[0];
    assert!(request.starts_with("GET /opi?"));
    let query = request
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .split_once('?')
        .unwrap()
        .1;
    let mut flat: BTreeMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    for (key, value) in [
        ("opi_cmd", HOMEPAGE),
        ("sn", "testdevice"),
        ("type", "0"),
        ("need_songinfo", "1"),
        ("device_id", "testdevice"),
        ("cv", "40071"),
        ("bus_cv", "6090007"),
        ("client_ip", "192.0.2.5"),
        ("login_type", "5"),
        ("user_login_type", "6"),
        ("qqmusic_open_id", "test-id"),
        ("qqmusic_access_token", "test-access 汉+&"),
    ] {
        assert_eq!(flat[key], value);
    }
    let original = flat.remove("sign").unwrap();
    assert_eq!(signed(flat, &client.config.key)["sign"], original);
    assert!(query.contains("test-access+%E6%B1%89%2B%26"));
    for forbidden in [
        "test-refresh",
        "test-key",
        "GetAlgorithmPlConf",
        "aiPlayList",
        "M-Encoding",
        "application/json",
        "synthetic-music-id",
    ] {
        assert!(!request.contains(forbidden));
    }
}
#[test]
fn homepage_defaults_fill_missing_fields_but_preserve_explicit_configuration() {
    let mut config = config();
    config.device_id = "-device-with-hyphens".into();
    config.common.insert("client_ip".into(), "192.0.2.9".into());
    let values: BTreeMap<_, _> = config.homepage_values().into_iter().collect();
    assert_eq!(values["sn"], "devicewithhyphens");
    assert_eq!(values["type"], "0");
    assert_eq!(values["need_songinfo"], "1");
    assert_eq!(values["cv"], "40071");
    assert!(
        !config
            .parameters("other", None, 1700000000)
            .contains_key("cv")
    );
    config.homepage_types = "100,200".into();
    for (key, value) in [
        ("cv", "50000"),
        ("bus_cv", "7000000"),
        ("device_id", "explicit-device"),
    ] {
        config.common.insert(key.into(), value.into());
    }
    let values: BTreeMap<_, _> = config.homepage_values().into_iter().collect();
    assert_eq!(values["type"], "100,200");
    assert_eq!(values["cv"], "50000");
    assert_eq!(values["bus_cv"], "7000000");
    assert_eq!(values["device_id"], "explicit-device");
    assert_eq!(values["client_ip"], "192.0.2.9");
}
#[test]
fn homepage_get_keeps_one_deadline_across_clock_retry_and_rejects_redirects() {
    use crate::discovery::testing::Server;
    let server = Server::new(vec![
        (
            "200 OK",
            serde_json::to_vec(&json!({"ret":10203,"subRet":1200,"ts":(now()+1000)*1000})).unwrap(),
            "",
        ),
        ("200 OK", br#"{"ret":0,"rc_item_arr":[]}"#.to_vec(), ""),
    ]);
    let mut config = config();
    config.gateway = format!("{}/opi", server.base);
    let mut client = Client::test_client(config);
    let data = client
        .homepage_playlists(None, Instant::now() + Duration::from_secs(3))
        .unwrap();
    assert_eq!(data["ret"], 0);
    assert!((999..=1000).contains(&client.offset_seconds));
    server.thread.join().unwrap();
    let requests = server.requests.lock().unwrap();
    let params = |i: usize| {
        let query = requests[i]
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .split_once('?')
            .unwrap()
            .1;
        url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect::<BTreeMap<String, String>>()
    };
    let first = params(0);
    let second = params(1);
    assert_ne!(first["sign"], second["sign"]);
    assert_eq!(first["type"], "0");
    assert_eq!(second["need_songinfo"], "1");
    assert!(!second.contains_key("qqmusic_access_token"));
    assert!(
        !requests
            .iter()
            .any(|r| r.contains("cookie:") || r.contains("authorization:"))
    );
    assert_eq!(
        client
            .homepage_playlists(None, Instant::now())
            .err()
            .unwrap()
            .code,
        "TIMEOUT"
    );
    drop(requests);
    let server = Server::new(vec![(
        "302 Found",
        vec![],
        "Location: https://example.invalid/secret\r\n",
    )]);
    client.config.gateway = format!("{}/opi", server.base);
    let error = client
        .homepage_playlists(None, Instant::now() + Duration::from_secs(2))
        .err()
        .unwrap();
    assert_eq!(error.code, "HTTP");
    assert!(!error.message.contains("secret"));
    server.thread.join().unwrap();
}
