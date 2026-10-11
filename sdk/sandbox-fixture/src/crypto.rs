#![allow(non_snake_case)]
use windows_sys::Win32::{Foundation::LocalFree, Security::Cryptography::*};
fn transform(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, ()> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(());
    }
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(result)
}
pub fn verify() -> Result<(), ()> {
    let sealed = transform(b"synthetic sandbox credential", true)?;
    if transform(&sealed, false)? != b"synthetic sandbox credential" {
        return Err(());
    }
    Ok(())
}
