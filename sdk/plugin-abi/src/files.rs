use crate::{arena::Arena, types::*};
use serde::{Deserialize, Serialize};
use std::ffi::c_void;

pub const chunkBytes: usize = 64 * 1024;
pub const rootPlugin: u32 = 1;
pub const rootData: u32 = 2;
pub const rootTemp: u32 = 3;
pub const openRead: u32 = 1;
pub const openWrite: u32 = 2;
pub const openCreate: u32 = 4;
pub const openTruncate: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct fmFileRequest {
    pub structSize: u32,
    pub operation: u32,
    pub handle: u64,
    pub targetHandle: u64,
    pub flags: u32,
    pub offset: u64,
    pub length: u64,
    pub path: fmStringView,
    pub targetPath: fmStringView,
    pub bytes: fmStringView,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct fmFileEntry {
    pub name: fmStringView,
    pub kind: u32,
    pub length: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct fmFileReply {
    pub structSize: u32,
    pub status: i32,
    pub handle: u64,
    pub length: u64,
    pub kind: u32,
    pub bytes: fmStringView,
    pub entries: fmSlice,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct fmFileApi {
    pub structSize: u32,
    pub context: *mut c_void,
    pub call: unsafe extern "C" fn(*mut c_void, *const fmFileRequest, *mut fmHandle) -> fmStatus,
    pub getReply: unsafe extern "C" fn(fmHandle) -> *const fmFileReply,
    pub releaseReply: unsafe extern "C" fn(fmHandle),
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRequest {
    pub Operation: u32,
    pub Handle: u64,
    pub TargetHandle: u64,
    pub Flags: u32,
    pub Offset: u64,
    pub Length: u64,
    pub Path: String,
    pub TargetPath: String,
    pub Bytes: Vec<u8>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub Name: String,
    pub Kind: u32,
    pub Length: u64,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReply {
    pub Status: i32,
    pub Handle: u64,
    pub Length: u64,
    pub Kind: u32,
    pub Bytes: Vec<u8>,
    pub Entries: Vec<FileEntry>,
}
#[allow(dead_code)]
pub struct OwnedFileReply {
    pub View: fmFileReply,
    Arena: Arena,
}
impl OwnedFileReply {
    pub fn new(r: FileReply) -> Self {
        let mut arena = Arena::default();
        let entries = r
            .Entries
            .iter()
            .map(|e| fmFileEntry {
                name: arena.text(&e.Name),
                kind: e.Kind,
                length: e.Length,
            })
            .collect();
        let bytes = fmStringView {
            data: r.Bytes.as_ptr(),
            length: r.Bytes.len() as u64,
        };
        arena.keep(r.Bytes);
        Self {
            View: fmFileReply {
                structSize: size_of::<fmFileReply>() as u32,
                status: r.Status,
                handle: r.Handle,
                length: r.Length,
                kind: r.Kind,
                bytes,
                entries: arena.array(entries),
            },
            Arena: arena,
        }
    }
}
/// # Safety
/// The request and its byte/string views must be readable in the current worker.
pub unsafe fn decodeFileRequest(p: *const fmFileRequest) -> Result<FileRequest, AbiFault> {
    if p.is_null() || unsafe { (*p).structSize } < size_of::<fmFileRequest>() as u32 {
        return Err(AbiFault);
    }
    let r = unsafe { *p };
    if r.bytes.length > chunkBytes as u64 || (r.bytes.length > 0 && r.bytes.data.is_null()) {
        return Err(AbiFault);
    }
    let bytes = if r.bytes.length == 0 {
        vec![]
    } else {
        unsafe { std::slice::from_raw_parts(r.bytes.data, r.bytes.length as usize) }.to_vec()
    };
    Ok(FileRequest {
        Operation: r.operation,
        Handle: r.handle,
        TargetHandle: r.targetHandle,
        Flags: r.flags,
        Offset: r.offset,
        Length: r.length,
        Path: unsafe { crate::decode::text(r.path) }.map_err(|_| AbiFault)?,
        TargetPath: unsafe { crate::decode::text(r.targetPath) }.map_err(|_| AbiFault)?,
        Bytes: bytes,
    })
}
pub fn requestView(r: &FileRequest) -> fmFileRequest {
    fn text(s: &str) -> fmStringView {
        fmStringView {
            data: s.as_ptr(),
            length: s.len() as u64,
        }
    }
    fmFileRequest {
        structSize: size_of::<fmFileRequest>() as u32,
        operation: r.Operation,
        handle: r.Handle,
        targetHandle: r.TargetHandle,
        flags: r.Flags,
        offset: r.Offset,
        length: r.Length,
        path: text(&r.Path),
        targetPath: text(&r.TargetPath),
        bytes: fmStringView {
            data: r.Bytes.as_ptr(),
            length: r.Bytes.len() as u64,
        },
    }
}
