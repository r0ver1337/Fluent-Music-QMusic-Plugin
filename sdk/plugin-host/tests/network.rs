#![allow(non_snake_case)]
mod support;
use fluent_plugin_api::{Operation, ResultData};
use fluent_plugin_host::{NativeManifest, PluginProcess};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    time::Duration,
};

fn start(path: &Path, runtime: &Path) -> PluginProcess {
    PluginProcess::startWithRuntime(
        path,
        runtime,
        Path::new(env!("CARGO_BIN_EXE_pluginloader")),
        true,
    )
    .unwrap()
}
#[test]
fn actualDllAllowsDeclaredRequestsButDeniesUndeclaredRawAndRedirectTraffic() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for index in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = [0; 4096];
            let size = stream.read(&mut bytes).unwrap();
            let text = String::from_utf8_lossy(&bytes[..size]);
            assert!(text.starts_with(if index == 0 {
                "GET /allowed?fixture=1 "
            } else {
                "GET /redirect "
            }));
            let reply = if index == 0 {
                "HTTP/1.1 200 OK\r\nContent-Length: 12\r\nSet-Cookie: first=fixture\r\nSet-Cookie: second=fixture\r\nConnection: close\r\n\r\nfixture-body".to_owned()
            } else {
                format!(
                    "HTTP/1.1 302 Found\r\nContent-Length: 0\r\nLocation: http://{address}/must-not-follow\r\nConnection: close\r\n\r\n"
                )
            };
            stream.write_all(reply.as_bytes()).unwrap();
        }
    });
    let runtime = tempfile::tempdir().unwrap();
    let path = support::install(runtime.path());
    let mut manifest: NativeManifest =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    manifest.Manifest.Network = vec![format!("http://{address}")];
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let mut plugin = start(&path, runtime.path());
    assert!(
        plugin
            .call(Operation::Search {
                query: format!("raw:{address}"),
                page: 0
            })
            .is_ok()
    );
    assert!(matches!(
        plugin
            .call(Operation::Search {
                query: format!("http://{address}/allowed?fixture=1"),
                page: 0
            })
            .unwrap(),
        ResultData::Tracks(_)
    ));
    for query in [
        format!("http://{address}/redirect"),
        "https://undeclared.example.invalid/a".into(),
        format!("hostHeader:http://{address}/spoof"),
    ] {
        assert_eq!(
            plugin
                .call(Operation::Search { query, page: 0 })
                .err()
                .unwrap()
                .code,
            "NETWORK"
        );
    }
    server.join().unwrap();
}
#[test]
fn missingDeclarationsAndOldOrMissingManifestAreRejected() {
    let runtime = tempfile::tempdir().unwrap();
    let path = support::install(runtime.path());
    let mut plugin = start(&path, runtime.path());
    assert_eq!(
        plugin
            .call(Operation::Search {
                query: "https://example.com/".into(),
                page: 0
            })
            .err()
            .unwrap()
            .code,
        "NETWORK"
    );
    drop(plugin);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    manifest["abiVersion"] = 1.into();
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(
        PluginProcess::startWithRuntime(
            &path,
            runtime.path(),
            Path::new(env!("CARGO_BIN_EXE_pluginloader")),
            true
        )
        .is_err()
    );
    manifest["abiVersion"] = 2.into();
    manifest.as_object_mut().unwrap().remove("manifest");
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(
        PluginProcess::startWithRuntime(
            &path,
            runtime.path(),
            Path::new(env!("CARGO_BIN_EXE_pluginloader")),
            true
        )
        .is_err()
    );
}
