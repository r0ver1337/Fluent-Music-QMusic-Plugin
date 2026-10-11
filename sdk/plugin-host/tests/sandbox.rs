#![cfg(windows)]
#![allow(non_snake_case)]
use fluent_plugin_api::*;
use fluent_plugin_host::PluginProcess;
use std::path::Path;
mod support;
use support::install;
#[test]
fn actualSandboxDeniesRawExternalReadsAndPreservesMediaLease() {
    let runtime = tempfile::tempdir().unwrap();
    let path = install(runtime.path());
    let mut plugin = PluginProcess::startWithRuntime(
        &path,
        runtime.path(),
        Path::new(env!("CARGO_BIN_EXE_pluginloader")),
        true,
    )
    .unwrap();
    assert_eq!(plugin.info.name, "raw-file-access-denied");
    assert_eq!(
        std::fs::read(runtime.path().join("data/sandbox-test/probe.txt")).unwrap(),
        b"private data"
    );
    assert_eq!(
        std::fs::read(runtime.path().join("temp/sandbox-test/probe.txt")).unwrap(),
        b"private data"
    );
    let ResultData::Tracks(page) = plugin.call(Operation::Recommendations).unwrap() else {
        panic!("tracks");
    };
    let ResultData::Media(source) = plugin
        .call(Operation::Resolve {
            track: page.items[0].clone(),
        })
        .unwrap()
    else {
        panic!("media");
    };
    let media = plugin.materialize(source).unwrap();
    let path = media.path.clone();
    plugin.call(Operation::Logout).unwrap();
    drop(plugin);
    assert_eq!(&std::fs::read(&path).unwrap()[..4], b"RIFF");
    drop(media);
    assert!(!path.exists());
}
#[test]
fn disabledSandboxRunsWorkerWithoutFileIsolationButKeepsBrokerChecks() {
    let runtime = tempfile::tempdir().unwrap();
    let path = install(runtime.path());
    let mut plugin = PluginProcess::startWithRuntime(
        &path,
        runtime.path(),
        Path::new(env!("CARGO_BIN_EXE_pluginloader")),
        false,
    )
    .unwrap();
    assert_eq!(plugin.info.name, "raw-file-access-allowed");
    assert_eq!(
        plugin
            .call(Operation::Search {
                query: "https://undeclared.example.invalid/a".into(),
                page: 0
            })
            .err()
            .unwrap()
            .code,
        "NETWORK"
    );
    assert_eq!(
        std::fs::read(runtime.path().join("data/sandbox-test/probe.txt")).unwrap(),
        b"private data"
    );
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
    let media = plugin.materialize(source).unwrap();
    plugin.call(Operation::Logout).unwrap();
    drop(plugin);
    assert_eq!(&std::fs::read(&media.path).unwrap()[..4], b"RIFF");
}
#[test]
fn oldExecutableManifestIsRejected() {
    let runtime = tempfile::tempdir().unwrap();
    let path = runtime.path().join("plugin.json");
    std::fs::write(
        &path,
        br#"{"api_version":1,"id":"old","executable":"old.exe"}"#,
    )
    .unwrap();
    assert!(
        PluginProcess::startWithRuntime(&path, runtime.path(), Path::new("missing.exe"), true)
            .is_err()
    );
}

#[test]
fn typedLyricsSurviveDllReleaseWorkerCopyAndSandboxIpc() {
    let runtime = tempfile::tempdir().unwrap();
    let path = install(runtime.path());
    let mut plugin = PluginProcess::startWithRuntime(
        &path,
        runtime.path(),
        Path::new(env!("CARGO_BIN_EXE_pluginloader")),
        true,
    )
    .unwrap();
    assert!(plugin.info.supports(&Capability::Lyrics));
    let ResultData::Tracks(page) = plugin.call(Operation::Recommendations).unwrap() else {
        panic!("tracks")
    };
    let ResultData::Lyrics(lyrics) = plugin
        .call(Operation::Lyrics {
            track: page.items[0].clone(),
        })
        .unwrap()
    else {
        panic!("lyrics")
    };
    plugin.call(Operation::Logout).unwrap();
    drop(plugin);
    let original = lyrics.Original.unwrap();
    assert_eq!(original.OffsetMs, -50);
    assert_eq!(original.Lines[0].Words[1].StartMs, 1500);
    assert_eq!(original.Lines[0].Words[1].Text, "🎵");
    let translation = lyrics.Translation.unwrap();
    assert_eq!(translation.Lines[0].StartMs, Some(1100));
    assert_eq!(translation.Lines[0].DurationMs, None);
    assert_eq!(translation.Lines[0].Text, "Hello");
}

#[test]
fn qualityCataloguePermissionsAndSelectedMediaSurviveSandboxIpc() {
    let runtime = tempfile::tempdir().unwrap();
    let path = install(runtime.path());
    let mut plugin = PluginProcess::startWithRuntime(
        &path,
        runtime.path(),
        Path::new(env!("CARGO_BIN_EXE_pluginloader")),
        true,
    )
    .unwrap();
    assert!(plugin.info.supports(&Capability::TrackQualities));
    let ResultData::TrackQualities(catalogue) = plugin
        .call(Operation::TrackQualities { track: None })
        .unwrap()
    else {
        panic!("catalogue")
    };
    assert_eq!(catalogue.Levels.len(), 2);
    assert!(catalogue.Items.is_empty());
    let ResultData::Tracks(page) = plugin.call(Operation::Recommendations).unwrap() else {
        panic!("tracks")
    };
    let track = page.items[0].clone();
    let ResultData::TrackQualities(list) = plugin
        .call(Operation::TrackQualities {
            track: Some(track.clone()),
        })
        .unwrap()
    else {
        panic!("qualities")
    };
    assert_eq!(list.Items[0].FileSizeBytes, Some(32_000_000));
    assert!(!list.Items[0].Playable);
    assert_eq!(list.Items[1].FileSizeBytes, None);
    assert_eq!(
        plugin
            .call(Operation::ResolveWithQuality {
                track: track.clone(),
                qualityId: "lossless".into()
            })
            .err()
            .unwrap()
            .code,
        "QUALITY_UNAVAILABLE"
    );
    let ResultData::Media(source) = plugin
        .call(Operation::ResolveWithQuality {
            track,
            qualityId: "standard".into(),
        })
        .unwrap()
    else {
        panic!("media")
    };
    let media = plugin.materialize(source).unwrap();
    plugin.call(Operation::Logout).unwrap();
    drop(plugin);
    assert_eq!(list.Items[1].Name, "标准");
    assert_eq!(&std::fs::read(&media.path).unwrap()[..4], b"RIFF");
}
