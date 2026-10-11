#![allow(non_snake_case)]
use std::path::PathBuf;
use windows_sys::Win32::{
    System::Com::*,
    UI::{Controls::Dialogs::*, Shell::*},
};
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
pub fn choose(pluginId: &str, directory: bool, write: bool, purpose: &str) -> Option<PathBuf> {
    let purpose: String = purpose
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    let title = wide("Fluent Music — 插件文件授权");
    if !crate::accessPrompt::requestApproval(crate::accessPrompt::AccessPrompt {
        PluginId: pluginId.into(),
        Directory: directory,
        Writable: write,
        Purpose: purpose,
    }) {
        return None;
    }
    if directory {
        if unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) } < 0 {
            return None;
        }
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _apartment = Apartment;
        let mut display = [0u16; 260];
        let info = BROWSEINFOW {
            pszDisplayName: display.as_mut_ptr(),
            lpszTitle: title.as_ptr(),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_EDITBOX | BIF_NEWDIALOGSTYLE,
            ..Default::default()
        };
        let item = unsafe { SHBrowseForFolderW(&info) };
        if item.is_null() {
            return None;
        }
        let ok = unsafe { SHGetPathFromIDListW(item, display.as_mut_ptr()) };
        unsafe {
            CoTaskMemFree(item.cast());
        }
        if ok == 0 {
            return None;
        }
        let length = display.iter().position(|c| *c == 0)?;
        Some(String::from_utf16_lossy(&display[..length]).into())
    } else {
        let mut file = vec![0u16; 32768];
        let mut options = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            lpstrFile: file.as_mut_ptr(),
            nMaxFile: file.len() as u32,
            lpstrTitle: title.as_ptr(),
            Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_EXPLORER,
            ..Default::default()
        };
        if unsafe { GetOpenFileNameW(&mut options) } == 0 {
            return None;
        }
        let length = file.iter().position(|c| *c == 0)?;
        Some(String::from_utf16_lossy(&file[..length]).into())
    }
}
