use super::{ApiError, storage_error};
#[cfg(windows)]
const ENTROPY: &[u8] = b"FluentMusic/qqmusic/session/v1";

#[cfg(windows)]
fn dpapi(input: &[u8], encrypt: bool) -> Result<Vec<u8>, ApiError> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };
    use zeroize::Zeroize;
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(input.len()).map_err(|_| storage_error("凭证过大"))?,
        pbData: input.as_ptr().cast_mut(),
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // DPAPI current-user scope, UI forbidden. No machine-wide protection or insecure fallback.
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(storage_error(if encrypt {
            "Windows 登录凭证加密失败"
        } else {
            "Windows 无法解密登录凭证，请在原用户下运行或重新扫码"
        }));
    }
    // DPAPI owns this allocation. Copy it, wipe decrypted memory, then release using LocalFree.
    let result = unsafe {
        let bytes = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let result = bytes.to_vec();
        bytes.zeroize();
        LocalFree(output.pbData.cast());
        result
    };
    Ok(result)
}
pub(super) fn protect(input: &[u8]) -> Result<Vec<u8>, ApiError> {
    #[cfg(windows)]
    {
        dpapi(input, true)
    }
    #[cfg(not(windows))]
    {
        let _ = input;
        Err(storage_error("当前平台不支持安全登录持久化"))
    }
}
pub(super) fn unprotect(input: &[u8]) -> Result<Vec<u8>, ApiError> {
    #[cfg(windows)]
    {
        dpapi(input, false)
    }
    #[cfg(not(windows))]
    {
        let _ = input;
        Err(storage_error("当前平台不支持安全登录持久化"))
    }
}
