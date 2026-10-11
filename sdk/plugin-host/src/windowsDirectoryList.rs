#![allow(non_snake_case)]
use crate::windowsFs::{Directory, denied};
use std::{io, os::windows::io::AsRawHandle};
use windows_sys::Win32::{Foundation::*, Storage::FileSystem::*};
impl Directory {
    pub fn list(&self, offset: u64) -> io::Result<Vec<fluent_plugin_abi::files::FileEntry>> {
        let mut storage = vec![0u64; 8192];
        let mut result = vec![];
        let mut index = 0;
        let mut first = true;
        loop {
            let class = if first {
                FileIdBothDirectoryRestartInfo
            } else {
                FileIdBothDirectoryInfo
            };
            first = false;
            if unsafe {
                GetFileInformationByHandleEx(
                    self.File.as_raw_handle(),
                    class,
                    storage.as_mut_ptr().cast(),
                    (storage.len() * 8) as u32,
                )
            } == 0
            {
                if unsafe { GetLastError() } == ERROR_NO_MORE_FILES {
                    break;
                }
                return Err(io::Error::last_os_error());
            }
            let mut position = 0;
            loop {
                let nameOffset = std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
                if position + nameOffset > storage.len() * 8 {
                    return Err(denied());
                }
                let entry = unsafe {
                    &*storage
                        .as_ptr()
                        .cast::<u8>()
                        .add(position)
                        .cast::<FILE_ID_BOTH_DIR_INFO>()
                };
                let bytes = entry.FileNameLength as usize;
                if !bytes.is_multiple_of(2) || position + nameOffset + bytes > storage.len() * 8 {
                    return Err(denied());
                }
                let name = String::from_utf16_lossy(unsafe {
                    std::slice::from_raw_parts(entry.FileName.as_ptr(), bytes / 2)
                });
                if name != "."
                    && name != ".."
                    && !name.to_ascii_lowercase().starts_with(".fmmedia-")
                    && entry.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
                {
                    if index >= offset && result.len() < 128 {
                        result.push(fluent_plugin_abi::files::FileEntry {
                            Name: name,
                            Kind: if entry.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                                2
                            } else {
                                1
                            },
                            Length: entry.EndOfFile.max(0) as u64,
                        });
                    }
                    index += 1;
                }
                if entry.NextEntryOffset == 0 {
                    break;
                }
                position += entry.NextEntryOffset as usize;
                if position >= storage.len() * 8 || !position.is_multiple_of(8) {
                    return Err(denied());
                }
            }
            if result.len() == 128 {
                break;
            }
        }
        Ok(result)
    }
}
