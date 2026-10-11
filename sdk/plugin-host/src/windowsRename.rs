#![allow(non_snake_case)]
use crate::windowsFs::{Directory, validate};
use std::{ffi::c_void, io, os::windows::io::AsRawHandle, sync::Arc};
use windows_sys::Win32::{Foundation::*, Storage::FileSystem::*};
#[repr(C)]
struct IoStatus {
    Status: usize,
    Information: usize,
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtSetInformationFile(
        handle: HANDLE,
        status: *mut IoStatus,
        information: *const c_void,
        length: u32,
        class: u32,
    ) -> i32;
    fn RtlNtStatusToDosError(status: i32) -> u32;
}
impl Directory {
    pub fn rename(
        self: &Arc<Self>,
        path: &str,
        target: &Arc<Self>,
        targetPath: &str,
        replace: bool,
    ) -> io::Result<()> {
        let (parent, name) = self.parent(path)?;
        let (target, targetName) = target.parent(targetPath)?;
        let file = parent.child(
            &name,
            DELETE | FILE_READ_ATTRIBUTES,
            1,
            0x40,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        )?;
        validate(&file, false)?;
        match target.open(&targetName, 1) {
            Ok(existing) => validate(&existing, false)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let name: Vec<u16> = targetName.encode_utf16().collect();
        let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let size = offset + name.len() * 2;
        let mut storage = vec![0u64; size.div_ceil(8)];
        let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        unsafe {
            (*info).Anonymous.ReplaceIfExists = replace;
            (*info).RootDirectory = target.File.as_raw_handle();
            (*info).FileNameLength = (name.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(
                name.as_ptr(),
                storage.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
                name.len(),
            );
            let mut status = IoStatus {
                Status: 0,
                Information: 0,
            };
            let code = NtSetInformationFile(
                file.as_raw_handle(),
                &mut status,
                info.cast(),
                size as u32,
                10,
            );
            if code < 0 {
                return Err(io::Error::from_raw_os_error(
                    RtlNtStatusToDosError(code) as i32
                ));
            }
        }
        Ok(())
    }
}
