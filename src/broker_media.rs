use crate::{protocol::fail, qmc};
use fluent_plugin_abi::io::RemoteFile;
use fluent_plugin_api::{ApiError, MediaSource};
use std::{io::Read, time::Instant};

pub struct BrokerMedia {
    pub handle: u64,
}
impl Drop for BrokerMedia {
    fn drop(&mut self) {
        fluent_plugin_abi::io::releaseMedia(self.handle);
    }
}
pub fn convert(
    response: crate::http::Response,
    ekey: &str,
    deadline: Instant,
    limit: u64,
) -> Result<BrokerMedia, ApiError> {
    let mut file = RemoteFile::mediaOutput().map_err(|_| fail("MEDIA", "无法创建沙箱音频输出"))?;
    let copied = std::io::copy(&mut response.take(limit + 1), &mut file)
        .map_err(|_| fail("MEDIA", "沙箱音频写入失败"))?;
    if copied > limit {
        return Err(fail("MEDIA", "音频超过 512 MiB 限制"));
    }
    qmc::convert_until(&mut file, ekey, deadline)?;
    let handle = file
        .finishMedia()
        .map_err(|_| fail("MEDIA", "无法完成沙箱音频输出"))?;
    Ok(BrokerMedia { handle })
}
impl BrokerMedia {
    pub fn source(&self) -> MediaSource {
        MediaSource::ManagedFile { id: self.handle }
    }
}
