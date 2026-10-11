#![allow(non_snake_case)]
use crate::{
    fileBroker::{DirectorySlot, FileSlot},
    windowsFs::denied,
};
use std::{collections::HashMap, fs::Metadata, io};
pub fn metadata(
    files: &HashMap<u64, FileSlot>,
    directories: &HashMap<u64, DirectorySlot>,
    handle: u64,
    path: &str,
) -> io::Result<Metadata> {
    if path.is_empty() {
        if let Some(file) = files.get(&handle) {
            return file.File.metadata();
        }
        return directories
            .get(&handle)
            .ok_or_else(denied)?
            .Directory
            .File
            .metadata();
    }
    let directory = &directories.get(&handle).ok_or_else(denied)?.Directory;
    match directory.open(path, 1) {
        Ok(file) => file.metadata(),
        Err(error) => match directory.openDirectory(path, false) {
            Ok(directory) => directory.File.metadata(),
            Err(_) => Err(error),
        },
    }
}
