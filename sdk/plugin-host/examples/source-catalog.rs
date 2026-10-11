#![allow(non_snake_case)]
use fluent_plugin_api::{Capability, Operation, RecommendationSource, ResultData};
use fluent_plugin_host::{NativeManifest, PluginProcess, discover};
use std::path::{Component, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let directory = PathBuf::from(arguments.next().ok_or("Expected plugin directory")?);
    let loader =
        PathBuf::from(arguments.next().ok_or("Expected pluginloader.exe path")?).canonicalize()?;
    let publicSource = match arguments.next() {
        None => None,
        Some(flag) if flag == "--public-browse" => Some(
            arguments
                .next()
                .ok_or("Expected public source ID")?
                .into_string()
                .map_err(|_| "Invalid source ID")?,
        ),
        _ => return Err("Unexpected argument".into()),
    };
    if arguments.next().is_some() {
        return Err("Unexpected argument".into());
    }
    let runtime = tempfile::tempdir()?;
    let mut catalogues = Vec::new();
    let mut checkedPublic = false;
    for source in discover(&directory)? {
        let bytes = std::fs::read(&source)?;
        let manifest: NativeManifest = serde_json::from_slice(&bytes)?;
        if manifest.Id.is_empty()
            || !manifest.Id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
            })
            || !std::path::Path::new(&manifest.Library)
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
            || !manifest.Library.ends_with(".dll")
        {
            return Err("Invalid DLL manifest".into());
        }
        let plugin = runtime.path().join("plugins").join(&manifest.Id);
        let library = plugin.join(&manifest.Library);
        std::fs::create_dir_all(library.parent().ok_or("Invalid library path")?)?;
        std::fs::copy(
            source
                .parent()
                .ok_or("Invalid plugin directory")?
                .join(&manifest.Library),
            &library,
        )?;
        let path = plugin.join("plugin.json");
        std::fs::write(&path, &bytes)?;
        let mut process = PluginProcess::startWithRuntime(&path, runtime.path(), &loader, true)?;
        let qualities = if process.info.supports(&Capability::TrackQualities) {
            let ResultData::TrackQualities(list) =
                process.call(Operation::TrackQualities { track: None })?
            else {
                return Err("Plugin did not return a quality catalogue".into());
            };
            list.validate()?;
            if !list.Items.is_empty() {
                return Err("Global quality catalogue contains track resources".into());
            }
            list.Levels
        } else {
            Vec::new()
        };
        let publicBrowsing = if publicSource.as_deref() == Some(process.info.id.as_str()) {
            let result = checkPublicBrowsing(&mut process)?;
            checkedPublic = true;
            Some(result)
        } else {
            None
        };
        catalogues.push(serde_json::json!({
            "id": process.info.id,
            "name": process.info.name,
            "capabilities": process.info.capabilities,
            "extensions": process.info.extensions,
            "qualityLevels": qualities,
            "publicBrowsing": publicBrowsing,
        }));
    }
    if publicSource.is_some() && !checkedPublic {
        return Err("Requested public source was not found".into());
    }
    println!("{}", serde_json::to_string_pretty(&catalogues)?);
    Ok(())
}

fn checkPublicBrowsing(
    process: &mut PluginProcess,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let ResultData::RecommendationFeed(feed) = process.call(Operation::Homepage)? else {
        return Err("Expected public recommendation feed".into());
    };
    if feed.personalized || feed.playlist_source != RecommendationSource::Public {
        return Err("Expected explicitly public browsing results".into());
    }
    let playlist = feed
        .playlists
        .first()
        .ok_or("Public feed has no playlists")?;
    let ResultData::Tracks(detail) = process.call(Operation::Playlist {
        id: playlist.id.clone(),
        page: 0,
        cursor: None,
    })?
    else {
        return Err("Expected public playlist tracks".into());
    };
    let ResultData::Tracks(search) = process.call(Operation::Search {
        query: "测试".into(),
        page: 0,
    })?
    else {
        return Err("Expected public search tracks".into());
    };
    if detail.items.is_empty() || search.items.is_empty() {
        return Err("Public metadata verification returned an empty result".into());
    }
    Ok(serde_json::json!({
        "playlistCount": feed.playlists.len(),
        "playlistCoverCount": feed.playlists.iter().filter(|playlist| playlist.cover_url.is_some()).count(),
        "homepageTrackCount": feed.tracks.len(),
        "homepageTrackCoverCount": feed.tracks.iter().filter(|track| track.cover_url.is_some()).count(),
        "playlistPageTrackCount": detail.items.len(),
        "playlistPageTrackCoverCount": detail.items.iter().filter(|track| track.cover_url.is_some()).count(),
        "searchTrackCount": search.items.len(),
        "searchTrackCoverCount": search.items.iter().filter(|track| track.cover_url.is_some()).count(),
        "accountOrAudioRequested": false,
    }))
}
