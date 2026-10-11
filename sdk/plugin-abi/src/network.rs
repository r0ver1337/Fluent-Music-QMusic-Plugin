use crate::types::*;
use serde::{Deserialize, Serialize};
use std::{ffi::c_void, io};
pub mod http;

pub const chunkBytes: usize = 64 * 1024;
pub const maxRequestBytes: usize = 128 * 1024;
pub const maxWireBytes: usize = 1024 * 1024;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct fmNetworkApi {
    pub structSize: u32,
    pub context: *mut c_void,
    pub call: unsafe extern "C" fn(*mut c_void, *const fmStringView, *mut fmHandle) -> fmStatus,
    pub getReply: unsafe extern "C" fn(fmHandle) -> *const fmStringView,
    pub releaseReply: unsafe extern "C" fn(fmHandle),
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkRequest {
    pub Operation: u32,
    pub Handle: u64,
    pub Method: String,
    pub Url: String,
    pub Headers: Vec<(String, String)>,
    pub Body: Vec<u8>,
    pub TimeoutMs: u64,
    pub Length: u64,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkReply {
    pub Failed: bool,
    pub Handle: u64,
    pub Status: u16,
    pub Headers: Vec<(String, String)>,
    pub Body: Vec<u8>,
}
pub struct OwnedNetworkReply {
    pub View: fmStringView,
    Bytes: Vec<u8>,
}
impl OwnedNetworkReply {
    pub fn new(reply: NetworkReply) -> io::Result<Self> {
        let bytes = serde_json::to_vec(&reply)?;
        if bytes.len() > maxWireBytes {
            return Err(io::Error::other("Network reply too large"));
        }
        Ok(Self {
            View: view(&bytes),
            Bytes: bytes,
        })
    }
}
impl Drop for OwnedNetworkReply {
    fn drop(&mut self) {
        self.Bytes.fill(0);
    }
}
pub fn view(bytes: &[u8]) -> fmStringView {
    fmStringView {
        data: bytes.as_ptr(),
        length: bytes.len() as u64,
    }
}
/// # Safety
/// The view and its bytes must be readable in the current process.
pub unsafe fn readView(view: *const fmStringView) -> io::Result<Vec<u8>> {
    if view.is_null() {
        return Err(io::Error::other("Missing network buffer"));
    }
    let view = unsafe { *view };
    if view.length > maxWireBytes as u64 || (view.length > 0 && view.data.is_null()) {
        return Err(io::Error::other("Invalid network buffer"));
    }
    Ok(if view.length == 0 {
        vec![]
    } else {
        unsafe { std::slice::from_raw_parts(view.data, view.length as usize) }.to_vec()
    })
}
pub fn call(request: NetworkRequest) -> io::Result<NetworkReply> {
    let api = crate::io::networkApi()?;
    let bytes = serde_json::to_vec(&request)?;
    if bytes.len() > maxWireBytes {
        return Err(io::Error::other("Network request too large"));
    }
    let mut handle = std::ptr::null_mut();
    if unsafe { (api.call)(api.context, &view(&bytes), &mut handle) } != 0 || handle.is_null() {
        return Err(io::Error::other("Network request failed"));
    }
    let reply = (|| {
        let bytes = unsafe { readView((api.getReply)(handle)) }?;
        let reply: NetworkReply = serde_json::from_slice(&bytes)?;
        if reply.Failed {
            return Err(io::Error::other("Network request failed"));
        }
        Ok(reply)
    })();
    unsafe {
        (api.releaseReply)(handle);
    }
    reply
}
