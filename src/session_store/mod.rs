//! Plugin-owned login storage. Only DPAPI ciphertext ever reaches disk.
//! A per-login generation prevents stale processes from resurrecting a logged-out session.
use crate::protocol::{Config, Session, fail, hash, valid_music_id};
use fluent_plugin_api::ApiError;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const MAGIC: &[u8] = b"FMQQSESSION\x01";
const LIMIT: u64 = 64 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    binding: String,
    device_id: String,
    generation: String,
    session: Session,
}

pub struct RestoredSession {
    pub session: Session,
    pub device_id: String,
    pub generation: String,
}

pub struct SessionStore {
    directory: PathBuf,
    enabled: bool,
}
#[allow(dead_code)]
enum SessionLock {
    Native(File),
    Broker(fluent_plugin_abi::io::RemoteFile),
}
fn storage_error(message: &str) -> ApiError {
    fail("SESSION_STORAGE", message)
}
fn revoked() -> ApiError {
    fail("SESSION_REVOKED", "登录已在另一实例注销或更换，请重新登录")
}
fn binding(config: &Config) -> String {
    // Namespace/mismatch check only; cryptographic protection comes from current-user DPAPI.
    hash(
        &serde_json::json!([
            config.app_id,
            hash(&config.key),
            config.package,
            config.legacy
        ])
        .to_string(),
    )
}

mod protection;
mod storage;
use protection::{protect, unprotect};
use storage::regular_path;
#[cfg(all(test, windows))]
mod tests;

impl SessionStore {
    pub fn from_environment() -> Result<Option<Self>, ApiError> {
        let enabled = match std::env::var("QQMUSIC_PERSIST_SESSION").as_deref() {
            Ok("0") => false,
            Ok("1") | Err(_) => true,
            _ => return Err(storage_error("QQMUSIC_PERSIST_SESSION 只能为 0 或 1")),
        };
        if !cfg!(windows) {
            return Ok(None); // No plaintext fallback on unsupported platforms.
        }
        if fluent_plugin_abi::io::available() {
            return Ok(Some(Self {
                directory: PathBuf::new(),
                enabled,
            }));
        }
        let directory = match std::env::var_os("QQMUSIC_DATA_DIR").filter(|p| !p.is_empty()) {
            Some(path) => PathBuf::from(path),
            None => {
                let local = std::env::var_os("LOCALAPPDATA")
                    .filter(|p| !p.is_empty())
                    .ok_or_else(|| storage_error("无法确定本地加密凭证目录"))?;
                let install = std::env::current_dir()
                    .and_then(|p| p.canonicalize())
                    .map_err(|_| storage_error("无法确定插件安装目录"))?;
                // Isolate installations, including test copies; EXE renames do not change this scope.
                PathBuf::from(local)
                    .join("FluentMusic/plugins/qqmusic")
                    .join(hash(&install.to_string_lossy().to_lowercase()))
            }
        };
        Ok(Some(Self { directory, enabled }))
    }
    #[cfg(test)]
    pub fn at(directory: PathBuf) -> Self {
        Self {
            directory,
            enabled: true,
        }
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn load(&self, config: &Config) -> Result<Option<RestoredSession>, ApiError> {
        if !self.enabled {
            return Ok(None);
        }
        if !self.exists()? {
            return Ok(None);
        }
        let _lock = self.lock()?;
        let Some(record) = self.read()? else {
            return Ok(None);
        };
        if record.binding != binding(config)
            || (config.explicit_device_id && config.device_id != record.device_id)
        {
            return Err(storage_error(
                "本地凭证与应用或设备配置不匹配，请重新扫码登录",
            ));
        }
        // Expired access tokens remain useful: refresh through the authorized server, never extend locally.
        Ok(Some(RestoredSession {
            session: record.session,
            device_id: record.device_id,
            generation: record.generation,
        }))
    }
    pub fn check(&self, config: &Config, generation: &str) -> Result<(), ApiError> {
        let _lock = self.lock()?;
        let record = self.read()?.ok_or_else(revoked)?;
        if record.generation != generation || record.binding != binding(config) {
            return Err(revoked());
        }
        Ok(())
    }
    pub fn save(
        &self,
        config: &Config,
        session: &Session,
        expected: Option<&str>,
    ) -> Result<String, ApiError> {
        if !self.enabled {
            return Err(storage_error("登录凭证持久化已禁用"));
        }
        let _lock = self.lock()?;
        if let Some(expected) = expected {
            let record = self.read()?.ok_or_else(revoked)?;
            if record.generation != expected || record.binding != binding(config) {
                return Err(revoked());
            }
        }
        let generation = expected
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let record = Record {
            version: 1,
            binding: binding(config),
            device_id: config.device_id.clone(),
            generation: generation.clone(),
            session: session.clone(),
        };
        let plain = Zeroizing::new(
            serde_json::to_vec(&record).map_err(|_| storage_error("登录凭证编码失败"))?,
        );
        let sealed = protect(&plain)?;
        if sealed.len() + MAGIC.len() > LIMIT as usize {
            return Err(storage_error("登录凭证超过存储限制"));
        }
        if fluent_plugin_abi::io::available() {
            let mut bytes = MAGIC.to_vec();
            bytes.extend_from_slice(&sealed);
            fluent_plugin_abi::io::writeAtomic("session.dat", &bytes)
                .map_err(|_| storage_error("沙箱登录凭证写入失败"))?;
            return Ok(generation);
        }
        regular_path(&self.path(), false)?;
        let mut temp = tempfile::Builder::new()
            .prefix(".session-")
            .tempfile_in(&self.directory)
            .map_err(|_| storage_error("无法创建加密凭证临时文件"))?;
        temp.write_all(MAGIC)
            .and_then(|_| temp.write_all(&sealed))
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|_| storage_error("加密登录凭证写入失败"))?;
        temp.persist(self.path())
            .map_err(|_| storage_error("加密登录凭证替换失败"))?;
        Ok(generation)
    }
    pub fn clear(&self) -> Result<(), ApiError> {
        if !self.exists()? {
            return Ok(());
        }
        let _lock = self.lock()?;
        if fluent_plugin_abi::io::available() {
            let directory = fluent_plugin_abi::io::root(fluent_plugin_abi::files::rootData)
                .map_err(|_| storage_error("数据目录不可用"))?;
            return match fluent_plugin_abi::io::remove(directory, "session.dat") {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err(storage_error("沙箱登录凭证删除失败")),
            };
        }
        regular_path(&self.path(), false)?;
        match fs::remove_file(self.path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(storage_error(
                "本次会话已退出，但本地凭证删除失败；请重试注销",
            )),
        }
    }
}
