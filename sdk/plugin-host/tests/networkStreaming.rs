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
fn actualDllStreamsMoreThanThreeChunksWithoutTruncation() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut byte = [0];
        let mut header = vec![];
        while !header.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            header.push(byte[0]);
            assert!(header.len() < 8192);
        }
        let body: Vec<u8> = (0..3 * fluent_plugin_abi::network::chunkBytes + 17)
            .map(|index| (index % 251) as u8)
            .collect();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nSet-Cookie: a=fixture\r\nSet-Cookie: b=fixture\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        stream.write_all(&body).unwrap();
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
    assert!(
        plugin
            .call(Operation::Search {
                query: format!("large:http://{address}/stream"),
                page: 0
            })
            .is_ok()
    );
    server.join().unwrap();
}
