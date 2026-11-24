//! 证书密码等敏感字段的落盘保护。
//! Windows 上使用 DPAPI（CryptProtectData），密文与当前用户绑定；
//! 其他平台退化为 Base64（仅避免直接肉眼可见，不提供真正保护）。

pub mod optional {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Option<String>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // 一律明文出站：前端需要用明文填进输入框、也要把明文交给构建命令；
        // 落盘前的加密统一由 config::save_settings 处理，避免 IPC 往返被二次加密。
        match value.as_deref().filter(|v| !v.is_empty()) {
            Some(plain) => serializer.serialize_some(plain),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = Option::<String>::deserialize(deserializer)?;
        match raw.filter(|v| !v.is_empty()) {
            Some(value) => {
                // 落盘的密文才需要解密；界面回传的明文原样保留，
                // 否则「保存」会在参数反序列化阶段就报 DPAPI 解密失败。
                if super::looks_encrypted(&value) {
                    super::unprotect(&value)
                        .map(Some)
                        .map_err(serde::de::Error::custom)
                } else {
                    Ok(Some(value))
                }
            }
            None => Ok(None),
        }
    }
}

/// 判断字符串是否是本模块产出的 DPAPI 密文：base64 解码后应带 DPAPI blob 的固定头
/// （version=1 + Provider GUID {df9d8cd0-1501-11d1-8c7a-00c04fc297eb}）。
#[cfg(windows)]
pub fn looks_encrypted(value: &str) -> bool {
    use base64::Engine;
    const HEADER: [u8; 20] = [
        0x01, 0x00, 0x00, 0x00,
        0xd0, 0x8c, 0x9d, 0xdf, 0x01, 0x15, 0xd1, 0x11,
        0x8c, 0x7a, 0x00, 0xc0, 0x4f, 0xc2, 0x97, 0xeb,
    ];
    match base64::engine::general_purpose::STANDARD.decode(value) {
        Ok(bytes) => bytes.len() >= HEADER.len() && bytes[..HEADER.len()] == HEADER,
        Err(_) => false,
    }
}

/// 非 Windows 平台无法区分「base64(明文)」和「base64(密文)」，保持原有行为一律按密文处理。
#[cfg(not(windows))]
pub fn looks_encrypted(_value: &str) -> bool {
    true
}

#[cfg(windows)]
pub fn protect(plain: &str) -> Result<String, String> {
    use base64::Engine;
    use std::ptr;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPT_INTEGER_BLOB,
    };

    let bytes = plain.as_bytes();
    let mut input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    let ok = unsafe {
        CryptProtectData(
            &mut input,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("DPAPI 加密失败".to_string());
    }

    let encrypted =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData as *mut core::ffi::c_void);
    }

    Ok(base64::engine::general_purpose::STANDARD.encode(encrypted))
}

#[cfg(windows)]
pub fn unprotect(encoded: &str) -> Result<String, String> {
    use base64::Engine;
    use std::ptr;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPT_INTEGER_BLOB,
    };

    let mut data = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "密文格式无效".to_string())?;

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    let ok = unsafe {
        CryptUnprotectData(
            &mut input,
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("DPAPI 解密失败（可能不是当前用户加密的数据）".to_string());
    }

    let plain =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData as *mut core::ffi::c_void);
    }

    String::from_utf8(plain).map_err(|_| "解密结果不是有效文本".to_string())
}

#[cfg(not(windows))]
pub fn protect(plain: &str) -> Result<String, String> {
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(plain.as_bytes()))
}

#[cfg(not(windows))]
pub fn unprotect(encoded: &str) -> Result<String, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "密文格式无效".to_string())?;
    String::from_utf8(bytes).map_err(|_| "解密结果不是有效文本".to_string())
}
