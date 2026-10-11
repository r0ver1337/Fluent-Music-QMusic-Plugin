#![cfg(windows)]
#![allow(non_snake_case)]
mod support;
use fluent_plugin_api::*;
use fluent_plugin_host::{NativeManifest, PluginProcess};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    sync::mpsc,
    time::Duration,
};

#[test]
fn asynchronousMediaDownloadKeepsPermissionsAndLeasesWithoutBlockingPluginCalls() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
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
            .prepareMedia(MediaSource::Http {
                url: "https://undeclared.invalid/audio".into(),
                headers: Default::default()
            })
            .is_err()
    );
    let task = plugin
        .prepareMedia(MediaSource::Http {
            url: format!("http://{address}/audio"),
            headers: Default::default(),
        })
        .unwrap();
    let (started, request) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut buffer = [0; 4096];
        stream.read(&mut buffer).unwrap();
        started.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(3)).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nRIFFtest")
            .unwrap();
    });
    let download = std::thread::spawn(move || task.finish(|| true, true).unwrap());
    request.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(matches!(
        plugin.call(Operation::Recommendations).unwrap(),
        ResultData::Tracks(_)
    ));
    release.send(()).unwrap();
    let file = download.join().unwrap();
    let path = file.path.clone();
    drop(plugin);
    server.join().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"RIFFtest");
    drop(file);
    assert!(!path.exists());
}

#[test]
fn cancelledManagedMediaTaskReleasesItsLease() {
    let runtime = tempfile::tempdir().unwrap();
    let path = support::install(runtime.path());
    let mut plugin = PluginProcess::startWithRuntime(
        &path,
        runtime.path(),
        Path::new(env!("CARGO_BIN_EXE_pluginloader")),
        true,
    )
    .unwrap();
    let ResultData::Tracks(page) = plugin.call(Operation::Recommendations).unwrap() else {
        panic!("tracks")
    };
    let ResultData::Media(source) = plugin
        .call(Operation::Resolve {
            track: page.items[0].clone(),
        })
        .unwrap()
    else {
        panic!("media")
    };
    let task = plugin.prepareMedia(source).unwrap();
    assert_eq!(task.finish(|| false, true).err().unwrap().code, "CANCELLED");
}
