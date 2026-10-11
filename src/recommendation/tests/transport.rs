use super::*;
#[test]
fn independent_guest_transport_signs_m1_and_contains_no_account_credentials() {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let replies = [
            json!({"code":0,"music.getSession.session.GetSession":{"code":0,"data":{"session":{"uid":12345,"sid":"synthetic-sid","userip":"192.0.2.1"}}}}),
            json!({"code":0,"music.qqmusiclite.MtHomePageSvr.GetRecommendFullPage":{"code":0,"data":{"code":0,"v_shelf":[{"v_niche":[{"v_card":[{"jumptype":10046,"id":"123","title":"song"}]}]}]}}}),
            json!({"code":0,"music.playlist.PlaylistSquare.GetRecommendWhole":{"code":0,"data":{"HeadRsp":{"List":[{"Playlist":{"basic":{"tid":456,"title":"list","cover":{"medium_url":"https://y.gtimg.cn/a"}}}}]},"FeedRsp":{"List":[]}}}}),
        ];
        for (index, reply) in replies.into_iter().enumerate() {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut raw = Vec::new();
            let mut buffer = [0; 4096];
            let (end, length) = loop {
                let n = socket.read(&mut buffer).unwrap();
                assert_ne!(n, 0);
                raw.extend_from_slice(&buffer[..n]);
                if let Some(end) = raw.windows(4).position(|v| v == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&raw[..end]).to_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|v| {
                            v.strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap();
                    if raw.len() >= end + 4 + length {
                        break (end, length);
                    }
                }
            };
            let headers = String::from_utf8_lossy(&raw[..end]).to_lowercase();
            assert!(headers.starts_with(if index == 0 {
                "post /normal "
            } else {
                "post /safe "
            }));
            assert_eq!(headers.contains("\r\nsign:"), index != 0);
            assert_eq!(headers.contains("\r\nmask:"), index != 0);
            assert!(!headers.contains("cookie:") && !headers.contains("authorization:"));
            let packed = &raw[end + 4..end + 4 + length];
            let body = decompress(&packed[5..], false).unwrap();
            let value: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["comm"]["ct"], "11");
            assert!(
                value["comm"]
                    .as_object()
                    .unwrap()
                    .values()
                    .all(Value::is_string)
            );
            for key in [
                "qqmusic_access_token",
                "authst",
                "qq",
                "wxopenid",
                "openAppID",
            ] {
                assert!(value["comm"].get(key).is_none());
            }
            let response = serde_json::to_vec(&reply).unwrap();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response.len()
            )
            .unwrap();
            socket.write_all(&response).unwrap();
        }
    });
    let mut client = RecommendationClient::test_client(&base);
    let feed = client
        .feed(Instant::now() + Duration::from_secs(10))
        .unwrap();
    assert_eq!(feed.playlists.len(), 1);
    assert_eq!(feed.tracks.len(), 1);
    assert_eq!(feed.playlist_source, RecommendationSource::Homepage);
    assert!(!feed.personalized);
    worker.join().unwrap();
}
