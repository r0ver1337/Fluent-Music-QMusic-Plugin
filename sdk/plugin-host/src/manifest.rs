#![allow(non_snake_case)]
use fluent_plugin_api::ApiError;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    os::windows::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeManifest {
    pub AbiVersion: u32,
    pub Id: String,
    pub Library: String,
    pub Manifest: crate::PluginManifest,
    #[serde(default)]
    pub Options: BTreeMap<String, String>,
}
fn error(message: &str) -> ApiError {
    ApiError::new("PLUGIN_HOST", message)
}
pub fn libraryPath(manifest: &NativeManifest, root: &Path) -> Result<PathBuf, ApiError> {
    if manifest.AbiVersion != fluent_plugin_abi::abiVersion
        || manifest.Id.is_empty()
        || manifest.Id.len() > 64
        || !manifest
            .Id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c))
    {
        return Err(error("插件 ABI 版本或 ID 无效"));
    }
    let path = Path::new(&manifest.Library);
    if path.as_os_str().is_empty()
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
        || path
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("dll"))
    {
        return Err(error("插件必须是目录内的 DLL"));
    }
    let mut current = root.canonicalize().map_err(|_| error("插件目录无效"))?;
    for component in path.components() {
        current.push(component.as_os_str());
        let metadata = std::fs::symlink_metadata(&current).map_err(|_| error("插件 DLL 不存在"))?;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(error("拒绝加载链接插件"));
        }
    }
    Ok(current)
}
pub fn validatePluginTree(root: &Path) -> std::io::Result<()> {
    let root = crate::windowsFs::Directory::trustedReadOnly(root)?;
    let mut pending = vec![root];
    let mut count = 0;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(crate::windowsFs::Directory::filePath(&directory.File)?)? {
            let entry = entry?;
            count += 1;
            if count > 65536 {
                return Err(crate::windowsFs::denied());
            }
            let metadata = std::fs::symlink_metadata(entry.path())?;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(crate::windowsFs::denied());
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| crate::windowsFs::denied())?;
            if metadata.is_dir() {
                pending.push(directory.openDirectory(&name, false)?);
            } else {
                directory.open(&name, 1)?;
            }
        }
    }
    Ok(())
}
