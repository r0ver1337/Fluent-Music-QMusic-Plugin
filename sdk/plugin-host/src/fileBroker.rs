#![allow(non_snake_case, non_upper_case_globals)]
use crate::windowsFs::{Directory, denied};
use fluent_plugin_abi::files::*;
use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::Arc,
};

#[allow(dead_code)]
pub struct MediaArtifact {
    pub File: File,
    pub Path: std::path::PathBuf,
    Directory: Arc<Directory>,
}
pub(super) struct FileSlot {
    pub(super) File: File,
    Writable: bool,
    Media: bool,
}
pub(super) struct DirectorySlot {
    pub(super) Directory: Arc<Directory>,
    Scope: u32,
    Writable: bool,
}
pub struct FileBroker {
    Directories: HashMap<u64, DirectorySlot>,
    Files: HashMap<u64, FileSlot>,
    Media: HashMap<u64, Arc<MediaArtifact>>,
    MediaDirectory: Arc<Directory>,
    Next: u64,
    PluginId: String,
    AccessPrompts: u32,
}
impl FileBroker {
    pub fn new(runtime: &Path, plugin: &Path, id: &str) -> io::Result<Self> {
        if id.is_empty()
            || id.len() > 64
            || !id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
        {
            return Err(denied());
        }
        let runtime = Directory::trusted(runtime)?;
        let data = runtime
            .openDirectory("data", true)?
            .openDirectory(id, true)?;
        let temp = runtime
            .openDirectory("temp", true)?
            .openDirectory(id, true)?;
        let plugin = Directory::trustedReadOnly(plugin)?;
        let mediaName = format!(".fmMedia-{}-{}", std::process::id(), unique());
        // Internal media files are not exposed through ordinary temp-directory handles.
        let mediaDirectory = temp.internalDirectory(&mediaName)?;
        let mut directories = HashMap::new();
        for (handle, directory, writable) in [(1, plugin, false), (2, data, true), (3, temp, true)]
        {
            directories.insert(
                handle,
                DirectorySlot {
                    Directory: directory,
                    Scope: handle as u32,
                    Writable: writable,
                },
            );
        }
        Ok(Self {
            Directories: directories,
            Files: HashMap::new(),
            Media: HashMap::new(),
            MediaDirectory: mediaDirectory,
            Next: 100,
            PluginId: id.into(),
            AccessPrompts: 0,
        })
    }
    fn next(&mut self) -> u64 {
        self.Next += 1;
        self.Next
    }
    fn directory(&self, id: u64, write: bool) -> io::Result<&DirectorySlot> {
        let directory = self.Directories.get(&id).ok_or_else(denied)?;
        if write && !directory.Writable {
            return Err(denied());
        }
        Ok(directory)
    }
    pub fn call(&mut self, request: FileRequest) -> FileReply {
        let operation = request.Operation;
        match self.execute(request) {
            Ok(r) => r,
            Err(e) => {
                if std::env::var_os("FM_SANDBOX_DIAGNOSTICS").is_some() {
                    eprintln!(
                        "File API {operation}: {:?} / {:?}",
                        e.kind(),
                        e.raw_os_error()
                    );
                }
                FileReply {
                    Status: match e.kind() {
                        io::ErrorKind::PermissionDenied => 1,
                        io::ErrorKind::NotFound => 2,
                        io::ErrorKind::InvalidInput => 3,
                        io::ErrorKind::WouldBlock => 5,
                        _ => 4,
                    },
                    ..Default::default()
                }
            }
        }
    }
    fn execute(&mut self, r: FileRequest) -> io::Result<FileReply> {
        if self.Files.len() + self.Directories.len() + self.Media.len() >= 4096
            && matches!(r.Operation, 1 | 2 | 3 | 14 | 16 | 20 | 21)
        {
            return Err(denied());
        }
        let mut reply = FileReply::default();
        match r.Operation {
            0 => {
                if !(1..=3).contains(&r.Flags) {
                    return Err(denied());
                }
                reply.Handle = r.Flags as u64;
            }
            1 | 2 => {
                let root = self.directory(r.Handle, r.Operation == 2)?;
                let directory = root.Directory.openDirectory(&r.Path, r.Operation == 2)?;
                let slot = DirectorySlot {
                    Directory: directory,
                    Scope: root.Scope,
                    Writable: root.Writable,
                };
                let id = self.next();
                self.Directories.insert(id, slot);
                reply.Handle = id;
            }
            3 => {
                let write = r.Flags & (openWrite | openCreate | openTruncate) != 0;
                let root = self.directory(r.Handle, write)?;
                let file = root.Directory.open(&r.Path, r.Flags)?;
                let id = self.next();
                self.Files.insert(
                    id,
                    FileSlot {
                        File: file,
                        Writable: write,
                        Media: false,
                    },
                );
                reply.Handle = id;
            }
            4 | 5 | 7 | 8 | 18 => {
                let file = self.Files.get_mut(&r.Handle).ok_or_else(denied)?;
                if r.Operation != 4 && !file.Writable {
                    return Err(denied());
                }
                match r.Operation {
                    4 => {
                        if r.Length > chunkBytes as u64 {
                            return Err(denied());
                        }
                        let mut bytes = vec![0; r.Length as usize];
                        file.File.seek(SeekFrom::Start(r.Offset))?;
                        let count = file.File.read(&mut bytes)?;
                        bytes.truncate(count);
                        reply.Length = count as u64;
                        reply.Bytes = bytes;
                    }
                    5 => {
                        if r.Bytes.len() > chunkBytes
                            || r.Offset.checked_add(r.Bytes.len() as u64).is_none()
                        {
                            return Err(denied());
                        }
                        file.File.seek(SeekFrom::Start(r.Offset))?;
                        file.File.write_all(&r.Bytes)?;
                        reply.Length = r.Bytes.len() as u64;
                    }
                    7 => file.File.set_len(r.Length)?,
                    8 => file.File.sync_all()?,
                    18 => file.File.try_lock().map_err(|e| match e {
                        std::fs::TryLockError::WouldBlock => {
                            io::Error::from(io::ErrorKind::WouldBlock)
                        }
                        std::fs::TryLockError::Error(e) => e,
                    })?,
                    _ => unreachable!(),
                }
            }
            6 => {
                let metadata =
                    crate::fileInfo::metadata(&self.Files, &self.Directories, r.Handle, &r.Path)?;
                reply.Length = metadata.len();
                reply.Kind = if metadata.is_dir() { 2 } else { 1 };
            }
            9 | 10 => {
                let root = self.directory(r.Handle, true)?;
                let target = self.directory(r.TargetHandle, true)?;
                if root.Scope != target.Scope {
                    return Err(denied());
                }
                root.Directory.rename(
                    &r.Path,
                    &target.Directory,
                    &r.TargetPath,
                    r.Operation == 10,
                )?;
            }
            11 => self.directory(r.Handle, true)?.Directory.remove(&r.Path)?,
            12 => {
                self.Files.remove(&r.Handle).ok_or_else(denied)?;
            }
            13 => {
                if r.Handle <= 3 {
                    return Err(denied());
                }
                self.Directories.remove(&r.Handle).ok_or_else(denied)?;
            }
            14 => {
                let id = self.next();
                let file = self.MediaDirectory.media(&format!("{id}.audio"))?;
                self.Files.insert(
                    id,
                    FileSlot {
                        File: file,
                        Writable: true,
                        Media: true,
                    },
                );
                reply.Handle = id;
            }
            15 => {
                let slot = self.Files.get(&r.Handle).ok_or_else(denied)?;
                if !slot.Media {
                    return Err(denied());
                }
                slot.File.sync_all()?;
                let path = Directory::filePath(&slot.File)?;
                let slot = self.Files.remove(&r.Handle).unwrap();
                self.Media.insert(
                    r.Handle,
                    Arc::new(MediaArtifact {
                        File: slot.File,
                        Path: path,
                        Directory: self.MediaDirectory.clone(),
                    }),
                );
                reply.Handle = r.Handle;
            }
            16 => {
                if self.Media.len() > 4096 {
                    return Err(denied());
                }
                let media = self.Media.get(&r.Handle).cloned().ok_or_else(denied)?;
                let id = self.next();
                self.Media.insert(id, media);
                reply.Handle = id;
            }
            17 => {
                self.Media.remove(&r.Handle).ok_or_else(denied)?;
            }
            19 => {
                reply.Entries = self.directory(r.Handle, false)?.Directory.list(r.Offset)?;
                reply.Length = reply.Entries.len() as u64;
            }
            20 | 21 => {
                if r.Flags & !3 != 0 || r.Flags & openRead == 0 || self.AccessPrompts >= 3 {
                    return Err(denied());
                }
                self.AccessPrompts += 1;
                let writable = r.Flags & openWrite != 0;
                let path =
                    crate::access::choose(&self.PluginId, r.Operation == 21, writable, &r.Path)
                        .ok_or_else(denied)?;
                let id = self.next();
                if r.Operation == 21 {
                    let directory = if writable {
                        Directory::trusted(&path)?
                    } else {
                        Directory::trustedReadOnly(&path)?
                    };
                    self.Directories.insert(
                        id,
                        DirectorySlot {
                            Directory: directory,
                            Scope: id as u32,
                            Writable: writable,
                        },
                    );
                } else {
                    let parent = path.parent().ok_or_else(denied)?;
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .ok_or_else(denied)?;
                    let file = Directory::trustedReadOnly(parent)?.open(name, r.Flags)?;
                    self.Files.insert(
                        id,
                        FileSlot {
                            File: file,
                            Writable: writable,
                            Media: false,
                        },
                    );
                }
                reply.Handle = id;
            }
            _ => return Err(denied()),
        }
        Ok(reply)
    }
    pub fn acquireMedia(&mut self, id: u64) -> io::Result<Arc<MediaArtifact>> {
        self.Media.remove(&id).ok_or_else(denied)
    }
    pub fn hostOutput(&mut self) -> io::Result<MediaArtifact> {
        let id = self.next();
        let file = self.MediaDirectory.media(&format!("host-{id}.audio"))?;
        let path = Directory::filePath(&file)?;
        Ok(MediaArtifact {
            File: file,
            Path: path,
            Directory: self.MediaDirectory.clone(),
        })
    }
}
fn unique() -> u64 {
    static next: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
