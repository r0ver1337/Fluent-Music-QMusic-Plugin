#![allow(non_snake_case)]
use super::*;

/// Read a bounded newline-delimited JSON frame, including protection against a missing newline.
pub fn readFrame(reader: &mut impl std::io::BufRead) -> std::io::Result<Option<Vec<u8>>> {
    let mut out = Vec::new();
    loop {
        let bytes = reader.fill_buf()?;
        if bytes.is_empty() {
            return if out.is_empty() {
                Ok(None)
            } else {
                Err(std::io::Error::other("Truncated plugin frame"))
            };
        }
        let count = bytes
            .iter()
            .position(|b| *b == b'\n')
            .map_or(bytes.len(), |i| i + 1);
        if out.len() + count > MAX_MESSAGE_BYTES {
            return Err(std::io::Error::other("Oversized plugin frame"));
        }
        let done = bytes[count - 1] == b'\n';
        out.extend_from_slice(&bytes[..count]);
        reader.consume(count);
        if done {
            return Ok(Some(out));
        }
    }
}

pub fn serve(
    mut handler: impl FnMut(Operation) -> Result<ResultData, ApiError>,
) -> std::io::Result<()> {
    use std::io::Write;
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    while let Some(frame) = readFrame(&mut input)? {
        let request: Request = match serde_json::from_slice(&frame) {
            Ok(r) => r,
            Err(_) => {
                writeln!(
                    output,
                    "{}",
                    serde_json::to_string(&Response::new(
                        0,
                        Err(ApiError::new("PROTOCOL", "请求格式无效"))
                    ))?
                )?;
                output.flush()?;
                continue;
            }
        };
        let result = if request.api_version != API_VERSION {
            Err(ApiError::new("VERSION", "不支持的插件 API 版本"))
        } else {
            handler(request.operation)
        };
        let serialized = serde_json::to_vec(&Response::new(request.id, result))?;
        if serialized.len() >= MAX_MESSAGE_BYTES {
            return Err(std::io::Error::other("Oversized response"));
        }
        output.write_all(&serialized)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}
