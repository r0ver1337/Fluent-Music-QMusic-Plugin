use super::*;

impl SessionStore {
    pub(super) fn path(&self) -> PathBuf {
        self.directory.join("session.dat")
    }
    pub fn exists(&self) -> Result<bool, ApiError> {
        if fluent_plugin_abi::io::available() {
            return match fluent_plugin_abi::io::info(
                fluent_plugin_abi::io::root(fluent_plugin_abi::files::rootData)
                    .map_err(|_| storage_error("数据目录不可用"))?,
                "session.dat",
            ) {
                Ok(_) => Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(_) => Err(storage_error("无法检查沙箱登录凭证")),
            };
        }
        self.path()
            .try_exists()
            .map_err(|_| storage_error("无法检查本地登录凭证"))
    }
    pub(super) fn lock(&self) -> Result<SessionLock, ApiError> {
        if fluent_plugin_abi::io::available() {
            use fluent_plugin_abi::{
                files::{openCreate, openRead, openWrite, rootData},
                io::{RemoteFile, root},
            };
            let file = RemoteFile::open(
                root(rootData).map_err(|_| storage_error("数据目录不可用"))?,
                "session.lock",
                openRead | openWrite | openCreate,
            )
            .map_err(|_| storage_error("无法打开沙箱凭证锁"))?;
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match file.lock() {
                    Ok(()) => return Ok(SessionLock::Broker(file)),
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => return Err(storage_error("登录凭证正被占用，请重试")),
                }
            }
        }
        fs::create_dir_all(&self.directory)
            .map_err(|_| storage_error("无法创建本地加密凭证目录"))?;
        regular_path(&self.directory, true)?;
        let path = self.directory.join("session.lock");
        regular_path(&path, false)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|_| storage_error("无法打开凭证锁"))?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(SessionLock::Native(file)),
                Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return Err(storage_error("登录凭证正被占用，请重试")),
            }
        }
    }
    pub(super) fn read(&self) -> Result<Option<Record>, ApiError> {
        let bytes = if fluent_plugin_abi::io::available() {
            match fluent_plugin_abi::io::readFile(
                fluent_plugin_abi::files::rootData,
                "session.dat",
                LIMIT,
            ) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err(storage_error("无法读取沙箱登录凭证")),
            }
        } else {
            let path = self.path();
            regular_path(&path, false)?;
            let file = match File::open(path) {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err(storage_error("无法读取本地登录凭证")),
            };
            let mut bytes = Vec::new();
            file.take(LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| storage_error("本地登录凭证读取失败"))?;
            bytes
        };
        if bytes.len() as u64 > LIMIT || !bytes.starts_with(MAGIC) {
            return Err(storage_error("本地登录凭证格式无效，请重新扫码登录"));
        }
        let plain = Zeroizing::new(unprotect(&bytes[MAGIC.len()..])?);
        let record: Record = serde_json::from_slice(&plain)
            .map_err(|_| storage_error("本地登录凭证损坏，请重新扫码登录"))?;
        if record.version != 1
            || uuid::Uuid::parse_str(&record.generation).is_err()
            || record.device_id.is_empty()
            || record.device_id.len() > 1024
            || record.session.expires == 0
            || record
                .session
                .music_id
                .as_deref()
                .is_some_and(|id| !valid_music_id(id))
            || [
                &record.session.open_id,
                &record.session.access,
                &record.session.refresh,
            ]
            .iter()
            .any(|s| s.is_empty() || s.len() > 16384)
        {
            return Err(storage_error("本地登录凭证字段无效，请重新扫码登录"));
        }
        Ok(Some(record))
    }
}

pub(super) fn regular_path(path: &Path, directory: bool) -> Result<(), ApiError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            #[cfg(windows)]
            let linked = {
                use std::os::windows::fs::MetadataExt;
                metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
            };
            #[cfg(not(windows))]
            let linked = metadata.file_type().is_symlink();
            if linked || (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
                return Err(storage_error("拒绝使用非普通凭证文件或链接目录"));
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(storage_error("无法检查凭证文件类型")),
    }
}
