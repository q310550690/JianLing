//! 给宿主 exe 打上项目图标与版本信息（Windows PE 资源）。
//!
//! 宿主程序是全局共用的一个 exe，所以图标和版本信息只能在打包时按项目改写：
//! 用 UpdateResource 写入 RT_ICON / RT_GROUP_ICON / RT_VERSION 三类资源。

use std::ffi::c_void;
use std::path::Path;

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;
const LANG_ZH_CN: u16 = 0x0804;
const LANG_EN_US: u16 = 0x0409;
const CODEPAGE_UNICODE: u16 = 0x04B0;

unsafe extern "system" {
    fn BeginUpdateResourceW(file: *const u16, delete_existing: i32) -> *mut c_void;
    fn UpdateResourceW(
        handle: *mut c_void,
        ty: *const u16,
        name: *const u16,
        lang: u16,
        data: *const c_void,
        size: u32,
    ) -> i32;
    fn EndUpdateResourceW(handle: *mut c_void, discard: i32) -> i32;
}

/// 把图标与版本信息写进 exe。任何一步失败都不阻断打包，但会返回错误给调用方记录日志。
pub fn stamp_exe(
    exe: &Path,
    icon: Option<&[u8]>,
    product: &str,
    display_version: &str,
    build_id: &str,
) -> Result<(), String> {
    let wide = to_wide(exe.to_string_lossy().as_ref());
    let handle = unsafe { BeginUpdateResourceW(wide.as_ptr(), 0) };
    if handle.is_null() {
        return Err("无法打开 exe 资源（BeginUpdateResource 失败）".into());
    }

    let mut first_error: Option<String> = None;
    if let Some(bytes) = icon {
        if let Err(e) = write_icons(handle, bytes) {
            first_error.get_or_insert(e);
        }
    }
    let blocker = if first_error.is_none() {
        write_version(handle, product, display_version, build_id).err()
    } else {
        None
    };

    let ok = unsafe { EndUpdateResourceW(handle, 0) } != 0;
    if !ok {
        return Err("写回 exe 资源失败（EndUpdateResource 失败）".into());
    }
    match first_error.or(blocker) {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn write_icons(handle: *mut c_void, bytes: &[u8]) -> Result<(), String> {
    let entries = parse_ico(bytes)?;
    for (index, entry) in entries.iter().enumerate() {
        let start = entry.offset as usize;
        let end = start + entry.size as usize;
        if end > bytes.len() {
            return Err("图标文件数据越界".into());
        }
        let id = (index + 1) as u16;
        if !update(handle, RT_ICON, id, LANG_ZH_CN, &bytes[start..end]) {
            return Err(format!("写入图标位图 {id} 失败"));
        }
    }
    let group = build_group_dir(&entries);
    // 1 与 IDI_APPLICATION(32512) 各写一份，资源管理器与快捷方式都能取到
    for group_id in [1u16, 32512u16] {
        if !update(handle, RT_GROUP_ICON, group_id, LANG_ZH_CN, &group) {
            return Err(format!("写入图标组 {group_id} 失败"));
        }
    }
    Ok(())
}

fn write_version(
    handle: *mut c_void,
    product: &str,
    display_version: &str,
    build_id: &str,
) -> Result<(), String> {
    let blob = version_info(product, display_version, build_id);
    if !update(handle, RT_VERSION, 1, LANG_ZH_CN, &blob) {
        return Err("写入版本信息失败".into());
    }
    if !update(handle, RT_VERSION, 1, LANG_EN_US, &blob) {
        return Err("写入版本信息（英文）失败".into());
    }
    Ok(())
}

fn update(handle: *mut c_void, ty: u16, name: u16, lang: u16, data: &[u8]) -> bool {
    let type_ptr = ty as usize as *const u16;
    let name_ptr = name as usize as *const u16;
    unsafe {
        UpdateResourceW(
            handle,
            type_ptr,
            name_ptr,
            lang,
            data.as_ptr() as *const c_void,
            data.len() as u32,
        ) != 0
    }
}

#[derive(Debug, Clone)]
struct IconEntry {
    width: u8,
    height: u8,
    colors: u8,
    planes: u16,
    bits: u16,
    size: u32,
    offset: u32,
}

fn parse_ico(bytes: &[u8]) -> Result<Vec<IconEntry>, String> {
    if bytes.len() < 6 {
        return Err("图标文件过小".into());
    }
    let count = le_u16(bytes, 4) as usize;
    if count == 0 {
        return Err("图标文件里没有图像".into());
    }
    let need = 6 + count * 16;
    if bytes.len() < need {
        return Err("图标目录不完整".into());
    }
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let base = 6 + i * 16;
        entries.push(IconEntry {
            width: bytes[base],
            height: bytes[base + 1],
            colors: bytes[base + 2],
            planes: le_u16(bytes, base + 4),
            bits: le_u16(bytes, base + 6),
            size: le_u32(bytes, base + 8),
            offset: le_u32(bytes, base + 12),
        });
    }
    Ok(entries)
}

fn build_group_dir(entries: &[IconEntry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + entries.len() * 14);
    push16(&mut out, 0);
    push16(&mut out, 1);
    push16(&mut out, entries.len() as u16);
    for (index, entry) in entries.iter().enumerate() {
        out.push(entry.width);
        out.push(entry.height);
        out.push(entry.colors);
        out.push(0);
        push16(&mut out, entry.planes);
        push16(&mut out, entry.bits);
        push32(&mut out, entry.size);
        push16(&mut out, (index + 1) as u16);
    }
    out
}

/// 把 png / jpeg / ico 统一成 ICO 字节（多尺寸）；生成失败返回 None 让调用方继续打包。
pub fn ico_bytes(source: &Path) -> Result<Vec<u8>, String> {
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ext == "ico" {
        return std::fs::read(source).map_err(|e| format!("读取图标失败：{e}"));
    }
    let image = image::open(source).map_err(|e| format!("解析图片失败：{e}"))?;
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let mut frames: Vec<(u32, Vec<u8>)> = Vec::with_capacity(sizes.len());
    for size in sizes {
        let resized = image.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
        let mut png = Vec::new();
        resized
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| format!("编码图标失败：{e}"))?;
        frames.push((size, png));
    }

    let mut out = Vec::new();
    push16(&mut out, 0);
    push16(&mut out, 1);
    push16(&mut out, frames.len() as u16);
    let mut offset = 6 + 16 * frames.len() as u32;
    for (size, png) in &frames {
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        out.push(dim);
        out.push(dim);
        out.push(0);
        out.push(0);
        push16(&mut out, 1);
        push16(&mut out, 32);
        push32(&mut out, png.len() as u32);
        push32(&mut out, offset);
        offset += png.len() as u32;
    }
    for (_, png) in &frames {
        out.extend_from_slice(png);
    }
    Ok(out)
}

fn version_info(product: &str, display_version: &str, build_id: &str) -> Vec<u8> {
    let version = parse_version(display_version);
    let fixed = fixed_file_info(version, version);
    let table_key = format!("{LANG_ZH_CN:04X}{CODEPAGE_UNICODE:04X}");
    let strings = version_node(
        &table_key,
        0,
        1,
        &[],
        &[
            string_node("FileDescription", product),
            string_node("FileVersion", display_version),
            string_node("InternalName", product),
            string_node("OriginalFilename", &format!("{product}.exe")),
            string_node("ProductName", product),
            string_node("ProductVersion", display_version),
            string_node("Comments", &format!("jianling build {build_id}")),
            string_node("LegalCopyright", &format!("jianling build {build_id}")),
        ],
    );
    let string_table = version_node("StringFileInfo", 0, 1, &[], &[strings]);
    let mut translation = Vec::new();
    push16(&mut translation, LANG_ZH_CN);
    push16(&mut translation, CODEPAGE_UNICODE);
    let var = version_node(
        "VarFileInfo",
        0,
        1,
        &[],
        &[version_node("Translation", 4, 0, &translation, &[])],
    );
    version_node("VS_VERSION_INFO", 52, 0, &fixed, &[string_table, var])
}

fn fixed_file_info(version: [u16; 4], product: [u16; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(52);
    push32(&mut out, 0xFEEF_04BD);
    push32(&mut out, 0x0001_0000);
    push32(&mut out, ((version[0] as u32) << 16) | version[1] as u32);
    push32(&mut out, ((version[2] as u32) << 16) | version[3] as u32);
    push32(&mut out, ((product[0] as u32) << 16) | product[1] as u32);
    push32(&mut out, ((product[2] as u32) << 16) | product[3] as u32);
    push32(&mut out, 0x0000_003F);
    push32(&mut out, 0);
    push32(&mut out, 0x0004_0004);
    push32(&mut out, 1);
    push32(&mut out, 0);
    push32(&mut out, 0);
    push32(&mut out, 0);
    out
}

fn parse_version(text: &str) -> [u16; 4] {
    let mut parts = [0u16; 4];
    for (index, chunk) in text.split('.').take(4).enumerate() {
        let digits: String = chunk.chars().take_while(|c| c.is_ascii_digit()).collect();
        parts[index] = digits.parse().unwrap_or(0);
    }
    parts
}

fn version_node(key: &str, value_words: u16, value_type: u16, value: &[u8], children: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + value.len() + key.len() * 2);
    push16(&mut out, 0);
    push16(&mut out, value_words);
    push16(&mut out, value_type);
    out.extend_from_slice(&utf16z(key));
    align4(&mut out);
    out.extend_from_slice(value);
    align4(&mut out);
    for child in children {
        out.extend_from_slice(child);
    }
    let length = out.len() as u16;
    out[0..2].copy_from_slice(&length.to_le_bytes());
    out
}

fn string_node(key: &str, value: &str) -> Vec<u8> {
    let text = utf16z(value);
    let words = (value.encode_utf16().count() + 1) as u16;
    version_node(key, words, 1, &text, &[])
}

fn utf16z(text: &str) -> Vec<u8> {
    let mut out: Vec<u8> = text.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    out.extend_from_slice(&[0, 0]);
    out
}

fn to_wide(text: &str) -> Vec<u16> {
    let mut out: Vec<u16> = text.encode_utf16().collect();
    out.push(0);
    out
}

fn align4(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn push16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_blob_is_aligned_and_parsable() {
        let blob = version_info("移星智护", "1.2.0", "jl-test000-0-0000");
        assert_eq!(blob.len() % 4, 0, "版本资源需按 4 字节对齐");
        assert_eq!(le_u16(&blob, 0) as usize, blob.len());
        let key_len = 15 * 2 + 2;
        let key = &blob[6..6 + key_len - 2];
        let key_text = String::from_utf16(
            &key.chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(key_text, "VS_VERSION_INFO");
    }

    #[test]
    fn version_parts_are_split() {
        assert_eq!(parse_version("1.2.0"), [1, 2, 0, 0]);
        assert_eq!(parse_version("10.0.19041.789"), [10, 0, 19041, 789]);
        assert_eq!(parse_version(""), [0, 0, 0, 0]);
    }

    #[test]
    fn stamps_real_host_exe() {
        let host = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries/shell-host.exe");
        if !host.is_file() {
            return;
        }
        let target = std::env::temp_dir().join("jianling-winres-stamp.exe");
        std::fs::copy(&host, &target).unwrap();
        // 单张 PNG 组一个 1×1 的 ICO，足够验证资源写入链路
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            256,
            256,
            image::Rgba([30, 90, 200, 255]),
        ))
        .resize_exact(256, 256, image::imageops::FilterType::Nearest)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
        let icon = ico_bytes_from_png(&png);
        stamp_exe(&target, Some(&icon), "移星智护", "1.2.0", "jl-test000-0-0000").unwrap();
        assert_eq!(std::fs::metadata(&target).unwrap().len() > 0, true);
    }

    fn ico_bytes_from_png(png: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        push16(&mut out, 0);
        push16(&mut out, 1);
        push16(&mut out, 1);
        out.extend_from_slice(&[0, 0, 0, 0]);
        push16(&mut out, 1);
        push16(&mut out, 32);
        push32(&mut out, png.len() as u32);
        push32(&mut out, 22);
        out.extend_from_slice(png);
        out
    }
}
