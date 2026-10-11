#![allow(non_snake_case)]
#[cfg(not(windows))]
compile_error!("The native plugin sandbox currently requires Windows.");
use fluent_plugin_api::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
#[cfg(windows)]
mod access;
mod accessPrompt;
pub use accessPrompt::{AccessApproval, AccessPrompt, setAccessApproval};
#[cfg(windows)]
mod fileBroker;
#[cfg(windows)]
mod fileInfo;
#[cfg(windows)]
mod manifest;
#[cfg(windows)]
mod native;
mod networkBroker;
mod networkPolicy;
pub use networkPolicy::{NetworkPolicy, PluginManifest};
#[cfg(windows)]
mod sandboxProfile;
#[cfg(windows)]
mod sandboxWindows;
mod transport;
mod mediaTask;
pub use mediaTask::MediaTask;
#[cfg(windows)]
mod windowsDirectoryList;
#[cfg(windows)]
mod windowsFs;
#[cfg(windows)]
mod windowsRename;
mod wire;
#[cfg(windows)]
mod worker;
mod workerNetwork;
#[cfg(windows)]
pub use manifest::NativeManifest;
#[cfg(windows)]
pub use native::{NativeProcess as PluginProcess, NativeTerminator as PluginTerminator};
#[cfg(windows)]
pub use worker::runWorker;

pub fn discover(directory: &Path) -> Result<Vec<PathBuf>, ApiError> {
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut paths = vec![];
    for entry in std::fs::read_dir(directory)
        .map_err(|_| ApiError::new("PLUGIN_HOST", "无法读取插件目录"))?
    {
        let path = entry
            .map_err(|_| ApiError::new("PLUGIN_HOST", "无法读取插件文件"))?
            .path();
        if path.is_dir() && path.join("plugin.json").is_file() {
            paths.push(path.join("plugin.json"));
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn materialize(source: MediaSource) -> Result<MediaFile, ApiError> {
    match source {
        MediaSource::ManagedFile { .. } => {
            Err(ApiError::new("MEDIA", "媒体句柄必须由对应插件宿主解析"))
        }
        MediaSource::File { path } => {
            let path = PathBuf::from(path);
            if !path.is_file() {
                return Err(ApiError::new("MEDIA", "插件返回的音频文件不存在"));
            }
            Ok(MediaFile {
                path,
                _temporary: None,
                _mediaLease: None,
            })
        }
        MediaSource::Http { url, headers } => {
            let mut temp = tempfile::NamedTempFile::new()
                .map_err(|_| ApiError::new("MEDIA", "无法创建临时音频文件"))?;
            transport::downloadHttp(url, headers, temp.as_file_mut())?;
            Ok(MediaFile {
                path: temp.path().into(),
                _temporary: Some(temp),
                _mediaLease: None,
            })
        }
    }
}
pub struct MediaFile {
    pub path: PathBuf,
    _temporary: Option<tempfile::NamedTempFile>,
    _mediaLease: Option<Arc<fileBroker::MediaArtifact>>,
}
impl AsRef<Path> for MediaFile {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missingPluginsAreValid() {
        assert!(discover(Path::new("missing-directory")).unwrap().is_empty());
    }
    #[test]
    fn nonHttpMediaIsRejected() {
        assert!(
            materialize(MediaSource::Http {
                url: "file:///secret".into(),
                headers: Default::default()
            })
            .is_err()
        );
    }
}
