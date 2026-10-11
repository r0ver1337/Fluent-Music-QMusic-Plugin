use super::*;

/// Convert the downloaded file in place, using at most a 64 KiB audio buffer.
pub trait AudioStorage: Read + Write + Seek {
    fn file_length(&self) -> std::io::Result<u64>;
    fn set_file_length(&self, length: u64) -> std::io::Result<()>;
}
impl AudioStorage for std::fs::File {
    fn file_length(&self) -> std::io::Result<u64> {
        Ok(self.metadata()?.len())
    }
    fn set_file_length(&self, length: u64) -> std::io::Result<()> {
        self.set_len(length)
    }
}
impl AudioStorage for fluent_plugin_abi::io::RemoteFile {
    fn file_length(&self) -> std::io::Result<u64> {
        self.length()
    }
    fn set_file_length(&self, length: u64) -> std::io::Result<()> {
        self.setLength(length)
    }
}
pub fn convert_until(
    file: &mut impl AudioStorage,
    ekey: &str,
    deadline: Instant,
) -> Result<&'static str, ApiError> {
    let io_error = || fail("MEDIA", "QQ 临时音频读取或格式转换失败");
    let len = file.file_length().map_err(|_| io_error())?;
    let mut head = vec![0; len.min(64) as usize];
    file.seek(SeekFrom::Start(0)).map_err(|_| io_error())?;
    file.read_exact(&mut head).map_err(|_| io_error())?;
    // Prefer the keyed result: encrypted bytes can accidentally look like MP3 sync.
    // Also tolerate an incidental EKey on a resource that is already plain audio.
    let raw_format = container(&head);
    let stream = match Stream::from_ekey(ekey) {
        Ok(stream) => stream,
        Err(error) => return raw_format.ok_or(error),
    };
    stream.apply(&mut head, 0);
    let format = match container(&head) {
        Some(format) => format,
        None => {
            return raw_format.ok_or_else(|| {
                fail(
                    "MEDIA_FORMAT",
                    "QQ 音频转换后无法识别；密钥与资源可能不匹配或格式暂不支持",
                )
            });
        }
    };
    let mut tail = vec![0; len.min((MAX_EKEY_BYTES + 8) as u64) as usize];
    file.seek(SeekFrom::Start(len - tail.len() as u64))
        .map_err(|_| io_error())?;
    file.read_exact(&mut tail).map_err(|_| io_error())?;
    let payload_len = len - footer_size(&tail)? as u64;
    if payload_len == 0 {
        return Err(fail("MEDIA_FORMAT", "QQ 音频内容为空"));
    }
    let mut buffer = [0; 64 * 1024];
    let mut offset = 0;
    while offset < payload_len {
        if Instant::now() >= deadline {
            return Err(fail("MEDIA", "QQ 音频格式转换超时；请重试或使用较小的资源"));
        }
        let count = (payload_len - offset).min(buffer.len() as u64) as usize;
        file.seek(SeekFrom::Start(offset)).map_err(|_| io_error())?;
        file.read_exact(&mut buffer[..count])
            .map_err(|_| io_error())?;
        stream.apply(&mut buffer[..count], offset);
        file.seek(SeekFrom::Start(offset)).map_err(|_| io_error())?;
        file.write_all(&buffer[..count]).map_err(|_| io_error())?;
        offset += count as u64;
    }
    file.set_file_length(payload_len).map_err(|_| io_error())?;
    file.flush().map_err(|_| io_error())?;
    Ok(format)
}

#[cfg(test)]
pub fn convert(file: &mut std::fs::File, ekey: &str) -> Result<&'static str, ApiError> {
    convert_until(
        file,
        ekey,
        Instant::now() + std::time::Duration::from_secs(60),
    )
}
