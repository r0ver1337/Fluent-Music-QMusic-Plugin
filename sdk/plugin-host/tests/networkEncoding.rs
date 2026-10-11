#![allow(non_snake_case)]
mod support;
use fluent_plugin_api::Operation;
use fluent_plugin_host::{NativeManifest, PluginProcess};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    time::Duration,
};

#[test]
fn actualDllFormEncodingAndDefaultUserAgentArePreservedWithoutDuplicateHeaders() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for index in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = vec![];
            let headerEnd = loop {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                bytes.push(byte[0]);
                assert!(bytes.len() < 8192);
                if bytes.ends_with(b"\r\n\r\n") {
                    break bytes.len();
                }
            };
            let header = String::from_utf8(bytes).unwrap().to_ascii_lowercase();
            assert_eq!(header.matches("user-agent:").count(), 1);
            if index == 0 {
                assert!(header.starts_with("get /get "));
                assert!(header.contains("user-agent: fluentfixture/2\r\n"));
            } else {
                assert!(header.starts_with("post /post "));
                assert!(header.contains("user-agent: explicitfixture/2\r\n"));
                assert!(header.contains("content-type: application/x-www-form-urlencoded\r\n"));
                let length: usize = header
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                assert_eq!(
                    String::from_utf8(body).unwrap(),
                    "word=%E4%B8%AD%E6%96%87&other=a%2Bb"
                );
            }
            assert!(headerEnd > 0);
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\nSet-Cookie: a=fixture\r\nSet-Cookie: b=fixture\r\nConnection: close\r\n\r\nfixture-body").unwrap();
        }
    });
    let runtime = tempfile::tempdir().unwrap();
    let path = support::install(runtime.path());
    let mut manifest: NativeManifest =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    manifest.Manifest.Network = vec![format!("http://{address}")];
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let mut plugin = PluginProcess::startWithRuntime(
        &path,
        runtime.path(),
        Path::new(env!("CARGO_BIN_EXE_pluginloader")),
        true,
    )
    .unwrap();
    for query in [
        format!("http://{address}/get"),
        format!("form:http://{address}/post"),
    ] {
        assert!(plugin.call(Operation::Search { query, page: 0 }).is_ok());
    }
    server.join().unwrap();
}
