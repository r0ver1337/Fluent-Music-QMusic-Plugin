#![allow(non_snake_case)]
use std::{
    ffi::c_void,
    fs::File,
    io,
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::Path,
    sync::Arc,
};
use windows_sys::Win32::{Foundation::*, Storage::FileSystem::*};

#[repr(C)]
struct UnicodeString {
    Length: u16,
    MaximumLength: u16,
    Buffer: *mut u16,
}
#[repr(C)]
struct ObjectAttributes {
    Length: u32,
    RootDirectory: HANDLE,
    ObjectName: *mut UnicodeString,
    Attributes: u32,
    SecurityDescriptor: *mut c_void,
    SecurityQualityOfService: *mut c_void,
}
#[repr(C)]
struct IoStatus {
    Status: usize,
    Information: usize,
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtCreateFile(
        handle: *mut HANDLE,
        access: u32,
        attributes: *mut ObjectAttributes,
        status: *mut IoStatus,
        allocation: *const i64,
        fileAttributes: u32,
        share: u32,
        disposition: u32,
        options: u32,
        ea: *const c_void,
        eaLength: u32,
    ) -> i32;
    fn RtlNtStatusToDosError(status: i32) -> u32;
}
pub struct Directory {
    pub File: File,
    Writable: bool,
}
pub fn segments(path: &str) -> io::Result<Vec<&str>> {
    if path.is_empty() || path.len() > 4096 || path.starts_with(['/', '\\']) {
        return Err(denied());
    }
    let items: Vec<_> = path.split(['/', '\\']).collect();
    for part in &items {
        let device = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if part.is_empty()
            || *part == "."
            || *part == ".."
            || part.ends_with([' ', '.'])
            || part.chars().any(|c| c < ' ' || "<>:\"|?*".contains(c))
            || ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&device.as_str())
            || ((device.starts_with("COM") || device.starts_with("LPT"))
                && device.len() == 4
                && device.as_bytes()[3].is_ascii_digit())
            || part.to_ascii_lowercase().starts_with(".fmmedia-")
        {
            return Err(denied());
        }
    }
    Ok(items)
}
pub fn denied() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, "Sandbox path denied")
}
fn error() -> io::Error {
    io::Error::last_os_error()
}
pub(super) fn validate(file: &File, directory: bool) -> io::Result<()> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(error());
    }
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
        || (!directory && info.nNumberOfLinks != 1)
    {
        return Err(denied());
    }
    Ok(())
}
impl Directory {
    pub fn trusted(path: &Path) -> io::Result<Arc<Self>> {
        Self::trustedWithAccess(path, true)
    }
    pub fn trustedReadOnly(path: &Path) -> io::Result<Arc<Self>> {
        Self::trustedWithAccess(path, false)
    }
    fn trustedWithAccess(path: &Path, writable: bool) -> io::Result<Arc<Self>> {
        use std::os::windows::ffi::OsStrExt;
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | if writable { GENERIC_WRITE } else { 0 },
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(error());
        }
        let file = unsafe { File::from_raw_handle(handle) };
        validate(&file, true)?;
        Ok(Arc::new(Self {
            File: file,
            Writable: writable,
        }))
    }
    pub(super) fn child(
        &self,
        name: &str,
        access: u32,
        disposition: u32,
        options: u32,
        share: u32,
    ) -> io::Result<File> {
        let mut name: Vec<u16> = name.encode_utf16().collect();
        let length = u16::try_from(name.len() * 2).map_err(|_| denied())?;
        let mut unicode = UnicodeString {
            Length: length,
            MaximumLength: length,
            Buffer: name.as_mut_ptr(),
        };
        let mut attributes = ObjectAttributes {
            Length: size_of::<ObjectAttributes>() as u32,
            RootDirectory: self.File.as_raw_handle(),
            ObjectName: &mut unicode,
            Attributes: 0x40 | 0x1000,
            SecurityDescriptor: std::ptr::null_mut(),
            SecurityQualityOfService: std::ptr::null_mut(),
        };
        let mut status = IoStatus {
            Status: 0,
            Information: 0,
        };
        let mut handle = std::ptr::null_mut();
        // Root-directory handles and OBJ_DONT_REPARSE prevent check/open races.
        let code = unsafe {
            NtCreateFile(
                &mut handle,
                access | SYNCHRONIZE,
                &mut attributes,
                &mut status,
                std::ptr::null(),
                FILE_ATTRIBUTE_NORMAL,
                share,
                disposition,
                options | 0x20 | 0x00200000,
                std::ptr::null(),
                0,
            )
        };
        if code < 0 {
            return Err(io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(code) } as i32,
            ));
        }
        Ok(unsafe { File::from_raw_handle(handle) })
    }
    pub fn openDirectory(self: &Arc<Self>, path: &str, create: bool) -> io::Result<Arc<Self>> {
        if create && !self.Writable {
            return Err(denied());
        }
        let mut current = self.clone();
        for part in segments(path)? {
            let file = current.child(
                part,
                GENERIC_READ | if current.Writable { GENERIC_WRITE } else { 0 },
                if create { 3 } else { 1 },
                1,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
            )?;
            validate(&file, true)?;
            current = Arc::new(Self {
                File: file,
                Writable: self.Writable,
            });
        }
        Ok(current)
    }
    pub(super) fn parent(self: &Arc<Self>, path: &str) -> io::Result<(Arc<Self>, String)> {
        let parts = segments(path)?;
        let name = parts.last().unwrap().to_string();
        let parent = if parts.len() == 1 {
            self.clone()
        } else {
            self.openDirectory(&parts[..parts.len() - 1].join("/"), false)?
        };
        Ok((parent, name))
    }
    pub fn open(self: &Arc<Self>, path: &str, flags: u32) -> io::Result<File> {
        if flags & !15 != 0 || flags & 3 == 0 || (flags & 12 != 0 && flags & 2 == 0) {
            return Err(denied());
        }
        let (parent, name) = self.parent(path)?;
        let access = (if flags & 1 != 0 { GENERIC_READ } else { 0 })
            | (if flags & 2 != 0 { GENERIC_WRITE } else { 0 });
        let file = parent.child(
            &name,
            access,
            if flags & 4 != 0 { 3 } else { 1 },
            0x40,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
        )?;
        validate(&file, false)?;
        if flags & 8 != 0 {
            file.set_len(0)?;
        }
        Ok(file)
    }
    pub fn remove(self: &Arc<Self>, path: &str) -> io::Result<()> {
        let (parent, name) = self.parent(path)?;
        let file = parent.child(
            &name,
            DELETE | FILE_READ_ATTRIBUTES,
            1,
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        )?;
        let metadata = file.metadata()?;
        validate(&file, metadata.is_dir())?;
        let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfo,
                (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        } == 0
        {
            return Err(error());
        }
        Ok(())
    }
    pub fn media(self: &Arc<Self>, name: &str) -> io::Result<File> {
        let file = self.child(
            name,
            GENERIC_READ | GENERIC_WRITE | DELETE,
            2,
            0x40 | 0x1000,
            FILE_SHARE_READ | FILE_SHARE_DELETE,
        )?;
        validate(&file, false)?;
        Ok(file)
    }
    pub fn internalDirectory(self: &Arc<Self>, name: &str) -> io::Result<Arc<Self>> {
        if !name.starts_with(".fmMedia-") || name.contains(['/', '\\']) {
            return Err(denied());
        }
        let file = self.child(
            name,
            GENERIC_READ | GENERIC_WRITE,
            3,
            1,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
        )?;
        validate(&file, true)?;
        Ok(Arc::new(Self {
            File: file,
            Writable: true,
        }))
    }
    pub fn filePath(file: &File) -> io::Result<std::path::PathBuf> {
        let mut path = vec![0u16; 32768];
        let length = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                path.as_mut_ptr(),
                path.len() as u32,
                0,
            )
        };
        if length == 0 || length as usize >= path.len() {
            return Err(error());
        }
        Ok(String::from_utf16_lossy(&path[..length as usize]).into())
    }
}
