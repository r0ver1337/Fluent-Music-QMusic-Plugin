use super::*;
use serde_json::json;

#[test]
fn only_the_selected_qualitys_key_is_used() {
    let song = json!({"song_play_url_standard":"https://ws.stream.qqmusic.qq.com/song.mgg",
        "song_ekey_standard":"standard", "song_ekey_sq":"unrelated"});
    assert_eq!(select(&song).unwrap().ekey, Some("standard"));
    let plain = json!({"song_play_url":"https://ws.stream.qqmusic.qq.com/song.mp3",
        "song_ekey_sq":"unrelated"});
    assert!(select(&plain).unwrap().ekey.is_none());
    assert!(matches!(
        Adapter::default().source(&plain).unwrap(),
        MediaSource::Http { .. }
    ));
    let hq = json!({"song_play_url_hq":"https://ws.stream.qqmusic.qq.com/song.mgg", "song_ekey_hq":"hq"});
    assert_eq!(select(&hq).unwrap().ekey, Some("hq"));
}

#[test]
fn cannot_override_copyright_or_playability() {
    for value in [json!(0), json!("0"), json!(false)] {
        for field in ["playable", "copyright"] {
            let mut song = json!({"song_play_url_standard":"https://ws.stream.qqmusic.qq.com/song.mgg",
                "song_ekey_standard":"server-key"});
            song[field] = value.clone();
            assert_eq!(select(&song).err().unwrap().code, "UNPLAYABLE");
        }
    }
    assert_eq!(
        select(&json!({"song_ekey_sq":"key-but-no-url"}))
            .err()
            .unwrap()
            .code,
        "UNPLAYABLE"
    );
}

#[test]
fn rejects_non_qq_urls_and_credentials() {
    for url in [
        "https://example.com/x",
        "https://qq.com.evil.test/x",
        "file:///x",
        "https://user:password@ws.stream.qqmusic.qq.com/x",
        "http://127.0.0.1/x",
    ] {
        assert!(select(&json!({"song_play_url":url})).is_err());
    }
}

fn offline_http(
    status: &str,
    headers: String,
    body: Vec<u8>,
) -> (Adapter, String, std::thread::JoinHandle<()>) {
    use std::{io::Write, net::TcpListener};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let response = format!("HTTP/1.1 {status}\r\n{headers}Connection: close\r\n\r\n");
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = vec![];
        let mut buffer = [0; 1024];
        while !request.ends_with(b"\r\n\r\n") {
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0 && request.len() < 8192);
            request.extend(&buffer[..count]);
        }
        socket.write_all(response.as_bytes()).unwrap();
        socket.write_all(&body).unwrap();
    });
    let adapter = Adapter {
        // Only this test client maps a QQ-looking fixture hostname to loopback.
        http: Some(
            Client::builder()
                .no_proxy()
                .resolve("qmc-fixture.qq.com", addr)
                .timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
        ),
        files: VecDeque::new(),
        broker_files: VecDeque::new(),
    };
    (
        adapter,
        format!(
            "http://qmc-fixture.qq.com:{}/audio?synthetic-token",
            addr.port()
        ),
        server,
    )
}

#[test]
fn offline_http_download_converts_only_authorized_response_resources() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let vectors: Value =
        serde_json::from_str(include_str!("../../tests/qmc_vectors.json")).unwrap();
    for case in vectors["cases"].as_array().unwrap() {
        let body = STANDARD
            .decode(case["wav_cipher_b64"].as_str().unwrap())
            .unwrap();
        let headers = format!("Content-Length: {}\r\n", body.len());
        let (mut adapter, url, server) = offline_http("200 OK", headers, body);
        let source = adapter
            .source(&json!({"playable":1, "copyright":1,
            "song_play_url_standard":url, "song_ekey_standard":case["ekey"]}))
            .unwrap();
        server.join().unwrap();
        let file = fluent_plugin_host::materialize(source).unwrap();
        assert_eq!(
            std::fs::read(&file.path).unwrap(),
            STANDARD
                .decode(vectors["wav_plain_b64"].as_str().unwrap())
                .unwrap()
        );
        assert_eq!(adapter.files.len(), 1);
        drop(adapter);
        assert!(!file.path.exists());
    }
}

#[test]
fn rejects_redirect_partial_oversized_or_invalid_media_without_exposing_secrets() {
    for (status, headers, body) in [
        (
            "302 Found",
            "Content-Length: 0\r\nLocation: http://127.0.0.1/secret\r\n".into(),
            vec![],
        ),
        (
            "206 Partial Content",
            "Content-Length: 0\r\n".into(),
            vec![],
        ),
        (
            "200 OK",
            format!("Content-Length: {}\r\n", MAX_MEDIA_BYTES + 1),
            vec![],
        ),
        ("200 OK", "Content-Length: 4\r\n".into(), b"nope".to_vec()),
    ] {
        let (mut adapter, url, server) = offline_http(status, headers, body);
        let error = adapter
            .source(&json!({"song_play_url_standard":url,
            "song_ekey_standard":"synthetic-bad-key"}))
            .err()
            .unwrap();
        server.join().unwrap();
        assert!(!error.message.contains("synthetic-token"));
        assert!(!error.message.contains("synthetic-bad-key"));
        assert!(!error.message.contains("qmc-fixture"));
        assert!(adapter.files.is_empty());
    }
}

#[test]
fn cache_is_bounded_and_cleaned_on_logout_or_drop() {
    let mut adapter = Adapter::default();
    let mut paths = vec![];
    for _ in 0..3 {
        let file = temporary_audio().unwrap();
        paths.push(file.path().to_path_buf());
        adapter.retain(file);
    }
    assert_eq!(adapter.files.len(), 2);
    assert!(!paths[0].exists());
    assert!(paths[1].exists() && paths[2].exists());
    adapter.clear();
    assert!(!paths[1].exists() && !paths[2].exists());
    let file = temporary_audio().unwrap();
    let path = file.path().to_path_buf();
    adapter.retain(file);
    drop(adapter);
    assert!(!path.exists());
}

#[test]
fn converted_cache_is_readable_through_the_existing_generic_file_api() {
    use base64::Engine;
    let vectors: Value =
        serde_json::from_str(include_str!("../../tests/qmc_vectors.json")).unwrap();
    let case = &vectors["cases"][0];
    let mut file = temporary_audio().unwrap();
    use std::io::Write;
    file.write_all(
        &base64::engine::general_purpose::STANDARD
            .decode(case["wav_cipher_b64"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    qmc::convert(file.as_file_mut(), case["ekey"].as_str().unwrap()).unwrap();
    let mut adapter = Adapter::default();
    let source = adapter.retain(file);
    let serialized = serde_json::to_string(&source).unwrap();
    assert!(!serialized.contains(case["ekey"].as_str().unwrap()));
    let media = fluent_plugin_host::materialize(source).unwrap();
    assert_eq!(
        std::fs::read(&media.path).unwrap(),
        base64::engine::general_purpose::STANDARD
            .decode(vectors["wav_plain_b64"].as_str().unwrap())
            .unwrap()
    );
    let mut decoder_file = std::fs::File::open(&media.path).unwrap();
    adapter.clear();
    // An already-open decoder handle can still read/seek after logout/cache eviction.
    let mut head = [0; 4];
    decoder_file.read_exact(&mut head).unwrap();
    assert_eq!(&head, b"RIFF");
    drop(decoder_file);
    assert!(!media.path.exists());
}

#[cfg(windows)]
#[test]
#[ignore = "subprocess helper for forced-exit cache cleanup"]
fn delete_on_close_child() {
    if std::env::var("FLUENT_QMC_CACHE_CHILD").as_deref() != Ok("1") {
        return;
    }
    let file = temporary_audio().unwrap();
    println!("CACHE_PATH:{}", file.path().display());
    use std::io::Write;
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}

#[cfg(windows)]
#[test]
fn windows_cleans_cache_even_when_host_kills_plugin() {
    use std::{
        io::BufRead,
        process::{Command, Stdio},
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "media::tests::delete_on_close_child",
            "--nocapture",
        ])
        .env("FLUENT_QMC_CACHE_CHILD", "1")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let output = std::io::BufReader::new(child.stdout.take().unwrap());
    let path = output
        .lines()
        .map(Result::unwrap)
        .find_map(|line| {
            line.split_once("CACHE_PATH:")
                .map(|(_, path)| std::path::PathBuf::from(path))
        })
        .unwrap();
    assert!(path.exists());
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(!path.exists());
}
