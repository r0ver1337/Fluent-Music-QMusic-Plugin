#![allow(non_snake_case)]
use std::{io, os::windows::ffi::OsStrExt, path::Path};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, Isolation::*, *},
    Storage::FileSystem::*,
};
pub struct Sid(pub PSID);
impl Drop for Sid {
    fn drop(&mut self) {
        unsafe {
            FreeSid(self.0);
        }
    }
}
fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    text.encode_wide().chain(Some(0)).collect()
}
pub fn grantRead(path: &Path, sid: PSID, directory: bool) -> io::Result<()> {
    let path = wide(path.as_os_str());
    let mut descriptor = std::ptr::null_mut();
    let mut oldAcl = std::ptr::null_mut();
    let code = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut oldAcl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if code != 0 {
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
        grfAccessMode: SET_ACCESS,
        grfInheritance: if directory {
            SUB_CONTAINERS_AND_OBJECTS_INHERIT
        } else {
            NO_INHERITANCE
        },
        Trustee: TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: sid.cast(),
        },
    };
    let mut acl = std::ptr::null_mut();
    let code = unsafe { SetEntriesInAclW(1, &entry, oldAcl, &mut acl) };
    let result = if code != 0 {
        code
    } else {
        unsafe {
            SetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                acl,
                std::ptr::null(),
            )
        }
    };
    unsafe {
        LocalFree(descriptor);
        if !acl.is_null() {
            LocalFree(acl.cast());
        }
    }
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    Ok(())
}
pub fn profile(plugin: &Path, id: &str) -> io::Result<Sid> {
    let input = format!("{}\0{id}", plugin.to_string_lossy().to_lowercase());
    let hash = ring::digest::digest(&ring::digest::SHA256, input.as_bytes());
    let hash = hash.as_ref()[..24]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let name = format!("FluentMusic.{hash}");
    let name = wide(std::ffi::OsStr::new(&name));
    let mut sid = std::ptr::null_mut();
    let code = unsafe {
        CreateAppContainerProfile(
            name.as_ptr(),
            name.as_ptr(),
            name.as_ptr(),
            std::ptr::null(),
            0,
            &mut sid,
        )
    };
    if code < 0 {
        if code as u32 != 0x800700B7 {
            return Err(io::Error::other(format!(
                "AppContainer profile error {code:08x}"
            )));
        }
        if unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) } < 0 {
            return Err(io::Error::other("AppContainer SID unavailable"));
        }
    }
    Ok(Sid(sid))
}
