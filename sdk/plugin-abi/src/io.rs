use crate::{files::*, types::*};
use std::{
    io::{self, Read, Seek, SeekFrom, Write},
    sync::OnceLock,
};

struct Api(fmHostApi);
unsafe impl Send for Api {}
unsafe impl Sync for Api {}
static hostApi: OnceLock<Api> = OnceLock::new();
pub fn available() -> bool {
    hostApi.get().is_some()
}
pub(crate) fn networkApi() -> io::Result<crate::network::fmNetworkApi> {
    hostApi
        .get()
        .map(|api| api.0.networkApi)
        .ok_or_else(|| io::Error::other("Network API unavailable"))
}
/// # Safety
/// The host table and callback context must outlive all SDK files and callbacks.
pub unsafe fn installApi(api: *const fmHostApi) -> Result<(), AbiFault> {
    if api.is_null()
        || unsafe { (*api).structSize } < size_of::<fmHostApi>() as u32
        || unsafe { (*api).abiVersion } != abiVersion
        || unsafe { (*api).networkApi.structSize }
            < size_of::<crate::network::fmNetworkApi>() as u32
    {
        return Err(AbiFault);
    }
    hostApi.set(Api(unsafe { *api })).map_err(|_| AbiFault)
}
pub fn callFile(request: FileRequest) -> io::Result<FileReply> {
    let api = &hostApi
        .get()
        .ok_or_else(|| io::Error::other("File API unavailable"))?
        .0
        .fileApi;
    let mut handle = std::ptr::null_mut();
    let status = unsafe { (api.call)(api.context, &requestView(&request), &mut handle) };
    if status != 0 || handle.is_null() {
        return Err(io::Error::other("File API failed"));
    }
    let result = (|| {
        let pointer = unsafe { (api.getReply)(handle) };
        if pointer.is_null() || unsafe { (*pointer).structSize } < size_of::<fmFileReply>() as u32 {
            return Err(io::Error::other("Invalid file reply"));
        }
        let r = unsafe { *pointer };
        if r.status != 0 {
            return Err(io::Error::new(
                match r.status {
                    1 => io::ErrorKind::PermissionDenied,
                    2 => io::ErrorKind::NotFound,
                    5 => io::ErrorKind::WouldBlock,
                    _ => io::ErrorKind::Other,
                },
                "Sandbox file operation rejected",
            ));
        }
        if r.bytes.length > chunkBytes as u64 || (r.bytes.length != 0 && r.bytes.data.is_null()) {
            return Err(io::Error::other("Invalid file buffer"));
        }
        let bytes = if r.bytes.length == 0 {
            vec![]
        } else {
            unsafe { std::slice::from_raw_parts(r.bytes.data, r.bytes.length as usize) }.to_vec()
        };
        let entries = unsafe { crate::decode::array::<fmFileEntry>(r.entries) }
            .map_err(|_| io::Error::other("Invalid file entries"))?
            .into_iter()
            .map(|e| {
                Ok(FileEntry {
                    Name: unsafe { crate::decode::text(e.name) }
                        .map_err(|_| io::Error::other("Invalid filename"))?,
                    Kind: e.kind,
                    Length: e.length,
                })
            })
            .collect::<io::Result<_>>()?;
        Ok(FileReply {
            Status: 0,
            Handle: r.handle,
            Length: r.length,
            Kind: r.kind,
            Bytes: bytes,
            Entries: entries,
        })
    })();
    unsafe {
        (api.releaseReply)(handle);
    }
    result
}
pub fn root(kind: u32) -> io::Result<u64> {
    Ok(callFile(FileRequest {
        Flags: kind,
        ..Default::default()
    })?
    .Handle)
}
pub fn info(directory: u64, path: &str) -> io::Result<FileReply> {
    callFile(FileRequest {
        Operation: 6,
        Handle: directory,
        Path: path.into(),
        ..Default::default()
    })
}
pub fn remove(directory: u64, path: &str) -> io::Result<()> {
    callFile(FileRequest {
        Operation: 11,
        Handle: directory,
        Path: path.into(),
        ..Default::default()
    })?;
    Ok(())
}
pub fn readFile(kind: u32, path: &str, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = vec![];
    RemoteFile::open(root(kind)?, path, openRead)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("File exceeds read limit"));
    }
    Ok(bytes)
}
pub fn writeAtomic(path: &str, bytes: &[u8]) -> io::Result<()> {
    let directory = root(rootData)?;
    let name = format!(".write-{}-{}.tmp", std::process::id(), sequence());
    let result = (|| {
        let mut file = RemoteFile::open(
            directory,
            &name,
            openRead | openWrite | openCreate | openTruncate,
        )?;
        file.write_all(bytes)?;
        file.flush()?;
        drop(file);
        callFile(FileRequest {
            Operation: 10,
            Handle: directory,
            TargetHandle: directory,
            Path: name.clone(),
            TargetPath: path.into(),
            ..Default::default()
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = remove(directory, &name);
    }
    result
}
fn sequence() -> u64 {
    static next: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub struct RemoteFile {
    pub Handle: u64,
    Position: u64,
}
impl RemoteFile {
    pub fn open(directory: u64, path: &str, flags: u32) -> io::Result<Self> {
        let r = callFile(FileRequest {
            Operation: 3,
            Handle: directory,
            Path: path.into(),
            Flags: flags,
            ..Default::default()
        })?;
        Ok(Self {
            Handle: r.Handle,
            Position: 0,
        })
    }
    pub fn mediaOutput() -> io::Result<Self> {
        let r = callFile(FileRequest {
            Operation: 14,
            ..Default::default()
        })?;
        Ok(Self {
            Handle: r.Handle,
            Position: 0,
        })
    }
    pub fn requestAccess(purpose: &str, writable: bool) -> io::Result<Self> {
        let r = callFile(FileRequest {
            Operation: 20,
            Flags: openRead | if writable { openWrite } else { 0 },
            Path: purpose.into(),
            ..Default::default()
        })?;
        Ok(Self {
            Handle: r.Handle,
            Position: 0,
        })
    }
    pub fn finishMedia(mut self) -> io::Result<u64> {
        self.flush()?;
        let r = callFile(FileRequest {
            Operation: 15,
            Handle: self.Handle,
            ..Default::default()
        })?;
        self.Handle = 0;
        Ok(r.Handle)
    }
    pub fn length(&self) -> io::Result<u64> {
        Ok(callFile(FileRequest {
            Operation: 6,
            Handle: self.Handle,
            ..Default::default()
        })?
        .Length)
    }
    pub fn setLength(&self, length: u64) -> io::Result<()> {
        callFile(FileRequest {
            Operation: 7,
            Handle: self.Handle,
            Length: length,
            ..Default::default()
        })?;
        Ok(())
    }
    pub fn lock(&self) -> io::Result<()> {
        callFile(FileRequest {
            Operation: 18,
            Handle: self.Handle,
            ..Default::default()
        })?;
        Ok(())
    }
}
impl Read for RemoteFile {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let r = callFile(FileRequest {
            Operation: 4,
            Handle: self.Handle,
            Offset: self.Position,
            Length: buffer.len().min(chunkBytes) as u64,
            ..Default::default()
        })?;
        if r.Bytes.len() > buffer.len() {
            return Err(io::Error::other("Invalid read length"));
        }
        buffer[..r.Bytes.len()].copy_from_slice(&r.Bytes);
        self.Position += r.Bytes.len() as u64;
        Ok(r.Bytes.len())
    }
}
impl Write for RemoteFile {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let r = callFile(FileRequest {
            Operation: 5,
            Handle: self.Handle,
            Offset: self.Position,
            Bytes: buffer[..buffer.len().min(chunkBytes)].to_vec(),
            ..Default::default()
        })?;
        if r.Length > buffer.len() as u64 {
            return Err(io::Error::other("Invalid write length"));
        }
        self.Position += r.Length;
        Ok(r.Length as usize)
    }
    fn flush(&mut self) -> io::Result<()> {
        callFile(FileRequest {
            Operation: 8,
            Handle: self.Handle,
            ..Default::default()
        })?;
        Ok(())
    }
}
impl Seek for RemoteFile {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.Position = match position {
            SeekFrom::Start(p) => p,
            SeekFrom::Current(p) => self
                .Position
                .checked_add_signed(p)
                .ok_or_else(|| io::Error::other("Invalid seek"))?,
            SeekFrom::End(p) => self
                .length()?
                .checked_add_signed(p)
                .ok_or_else(|| io::Error::other("Invalid seek"))?,
        };
        Ok(self.Position)
    }
}
impl Drop for RemoteFile {
    fn drop(&mut self) {
        if self.Handle != 0 {
            let _ = callFile(FileRequest {
                Operation: 12,
                Handle: self.Handle,
                ..Default::default()
            });
        }
    }
}
pub fn releaseMedia(handle: u64) {
    let _ = callFile(FileRequest {
        Operation: 17,
        Handle: handle,
        ..Default::default()
    });
}
