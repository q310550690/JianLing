//! 构建水印：用于「这个包是哪次构建产出的」溯源。
//!
//! 三类载体互相独立，任意一类被抹掉也不影响其它类：
//! 1. 元数据（APK 清单 meta-data / assets 隐藏 json / exe 版本信息 / exe 尾部 overlay）
//! 2. 页面痕迹（隐藏 DOM + 零宽字符注释 + 极低透明度文字）
//! 3. 图片频域盲水印（8x8 DCT，仅 PNG，肉眼不可见，需要解码脚本才能读出）

use std::io::Write;
use std::path::Path;

/// exe 尾部附加数据的魔数（放在最后，便于从文件尾反向定位）
pub const OVERLAY_MAGIC: &[u8; 8] = b"ASWMBLD1";

/// 生成构建 ID：`jl-<项目短码>-<毫秒的 36 进制>-<4 位随机>`。
/// 只用作标识，不需要密码学强度。
pub fn new_build_id(project: &str, now_ms: u128) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in project.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let short = format!("{:08x}", (hash as u32) & 0xffff_ffff);
    let mut seed = now_ms as u64 ^ hash.rotate_left(17);
    seed ^= seed >> 33;
    seed = seed.wrapping_mul(0xff51_afd7_ed55_8ccd);
    seed ^= seed >> 29;
    format!(
        "jl-{}-{}-{:04x}",
        &short[..6],
        base36(now_ms as u64),
        (seed & 0xffff) as u16
    )
}

fn base36(mut value: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut out = Vec::new();
    while value > 0 {
        out.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

// ---------------------------------------------------------------- 零宽字符

const ZW_ALPHABET: [char; 4] = ['\u{200b}', '\u{200c}', '\u{200d}', '\u{2060}'];
/// 识别标记：`ASWM` 用 8 个零宽字符表示（每个字节 4 个 2bit 字符）
const ZW_MAGIC: &[u8] = b"ASWM";

/// 把文本编成零宽字符串：每字节 4 个字符，前面带 `ASWM` 魔数便于检索。
pub fn zero_width_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in ZW_MAGIC.iter().copied().chain(text.bytes()) {
        for shift in [6u8, 4, 2, 0] {
            out.push(ZW_ALPHABET[((byte >> shift) & 0b11) as usize]);
        }
    }
    out
}

/// 从任意文本里解出零宽载荷（仅测试用，校验逻辑已移到独立工具）。
#[cfg(test)]
pub fn zero_width_decode(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i + 4 <= chars.len() {
        // 从 i 开始尽量往后解一段连续的零宽串
        let mut bytes: Vec<u8> = Vec::new();
        let mut j = i;
        while j + 4 <= chars.len() {
            let mut value = 0u8;
            let mut ok = true;
            for k in 0..4 {
                match ZW_ALPHABET.iter().position(|c| *c == chars[j + k]) {
                    Some(bits) => value = (value << 2) | bits as u8,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                break;
            }
            bytes.push(value);
            j += 4;
        }
        if bytes.starts_with(ZW_MAGIC) {
            let payload: String = bytes[ZW_MAGIC.len()..]
                .iter()
                .take_while(|b| **b >= 0x20 && **b < 0x7f)
                .map(|b| *b as char)
                .collect();
            if !payload.is_empty() {
                return Some(payload);
            }
        }
        i = if j > i { j } else { i + 1 };
    }
    None
}

// ---------------------------------------------------------------- 构建元数据

/// 一次构建的完整标识：写入包内文件、exe overlay、页面 meta。
#[derive(Debug, Clone, serde::Serialize)]
pub struct BuildMeta {
    /// 短 ID，也是图片盲水印里唯一的内容
    pub build: String,
    pub app: String,
    /// Android 包名；Windows 用 exe 文件名
    pub package: String,
    pub version: String,
    #[serde(rename = "versionCode")]
    pub version_code: u32,
    pub platform: String,
    /// 项目 id，便于多项目反查
    pub project: String,
    /// 构建时间（毫秒时间戳）
    #[serde(rename = "createdAt")]
    pub created_at: u128,
    /// 构建机器名（只取主机名，不含用户名）
    pub host: String,
    /// 产出工具与版本
    pub tool: String,
    /// 产物文件名
    pub output: String,
    /// 转发规则条数
    #[serde(rename = "proxyCount")]
    pub proxy_count: usize,
}

impl BuildMeta {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        build: String,
        app: String,
        package: String,
        version: String,
        version_code: u32,
        platform: &str,
        project: &str,
        output: &str,
        proxy_count: usize,
    ) -> Self {
        Self {
            build,
            app,
            package,
            version,
            version_code,
            platform: platform.to_string(),
            project: project.to_string(),
            created_at: now_ms(),
            host: host_name(),
            tool: format!("jianling {}", env!("CARGO_PKG_VERSION")),
            output: output.to_string(),
            proxy_count,
        }
    }

    /// 单行 JSON（页面 meta / exe overlay 用）
    pub fn compact(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// 缩进 JSON（包内 build.json 用，方便人看）
    pub fn pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

pub fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default()
}

// ---------------------------------------------------------------- 页面痕迹

/// 生成注入到 index.html 的片段：meta 标记 + 零宽注释 + 隐藏 DOM + 极低透明度文字。
/// 远程页面（start_url）改不了，只有本地静态包能注入。
pub fn page_snippet(meta: &BuildMeta) -> String {
    let app = escape_html(&meta.app);
    let id = escape_html(&meta.build);
    let version = escape_html(&meta.version);
    let host = escape_html(&meta.host);
    let zw = zero_width_encode(&meta.compact());
    format!(
        concat!(
            "<meta name=\"jianling-build\" content=\"{id}\">\n",
            "<meta name=\"jianling-version\" content=\"{version}\">\n",
            "<meta name=\"jianling-build-info\" content='{info}'>\n",
            "<!-- {zw} -->\n",
            "<script>(function(){{try{{",
            "var m=document.createElement('div');",
            "m.id='__jianling_meta';m.setAttribute('data-build','{id}');",
            "m.setAttribute('data-version','{version}');m.setAttribute('data-host','{host}');",
            "m.style.cssText='position:fixed;left:-9999px;top:-9999px;width:0;height:0;overflow:hidden';",
            "m.textContent='{app} · {version} · {id}';",
            "var w=document.createElement('div');",
            "w.setAttribute('aria-hidden','true');",
            "w.style.cssText='position:fixed;inset:0;pointer-events:none;z-index:2147483000;'",
            "+'display:flex;align-items:center;justify-content:center;overflow:hidden;';",
            "var t=document.createElement('span');",
            "t.textContent='{app} · {version} · {id}';",
            "t.style.cssText='font:600 14px/1.2 system-ui,sans-serif;color:#000;opacity:0.02;'",
            "+'transform:rotate(-18deg);white-space:nowrap;user-select:none;';",
            "w.appendChild(t);",
            "(function boot(){{",
            "var root=document.body||document.documentElement;",
            "if(!root){{return setTimeout(boot,30);}}",
            "root.appendChild(m);root.appendChild(w);}})();",
            "}}catch(e){{}}}})();</script>\n"
        ),
        id = id,
        version = version,
        host = host,
        info = escape_attr(&meta.compact()),
        zw = zw,
        app = app
    )
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 单引号包裹的属性值：把 `'` 和 `&` 转义即可安全放进 content='...'。
fn escape_attr(text: &str) -> String {
    text.replace('&', "&amp;").replace('\'', "&#39;")
}

/// 从 HTML 里取出 `jianling-build-info` 的内容（仅测试用）。
#[cfg(test)]
pub fn extract_build_info(html: &str) -> Option<String> {
    let key = "jianling-build-info\" content='";
    let start = html.find(key)? + key.len();
    let rest = &html[start..];
    let end = rest.find('\'')?;
    Some(rest[..end].replace("&#39;", "'").replace("&amp;", "&"))
}

/// 把水印片段插进 index.html：有 `</head>` 就插在它前面，否则追加到末尾。
pub fn inject_into_html(html: &str, snippet: &str) -> String {
    if html.contains("jianling-build") {
        return html.to_string();
    }
    if let Some(pos) = html.to_lowercase().find("</head>") {
        let mut out = String::with_capacity(html.len() + snippet.len());
        out.push_str(&html[..pos]);
        out.push_str(snippet);
        out.push_str(&html[pos..]);
        out
    } else {
        format!("{html}\n{snippet}")
    }
}

/// 给目录里的 index.html 打页面水印（不存在则跳过）。
pub fn watermark_index_html(www: &Path, meta: &BuildMeta) -> Result<bool, String> {
    let index = www.join("index.html");
    if !index.is_file() {
        return Ok(false);
    }
    let html = std::fs::read_to_string(&index).map_err(|e| format!("读取 index.html 失败: {e}"))?;
    let next = inject_into_html(&html, &page_snippet(meta));
    if next == html {
        return Ok(false);
    }
    std::fs::write(&index, next).map_err(|e| format!("写入 index.html 失败: {e}"))?;
    Ok(true)
}

// ---------------------------------------------------------------- exe overlay

/// 在 exe 尾部追加一段 `[json][len:u32][magic]`，不影响 PE 结构。
pub fn append_overlay(exe: &Path, meta: &BuildMeta) -> Result<(), String> {
    let payload = meta.compact();
    let bytes = payload.as_bytes();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(exe)
        .map_err(|e| format!("打开 exe 失败: {e}"))?;
    file.write_all(bytes).map_err(|e| format!("写入失败: {e}"))?;
    file.write_all(&(bytes.len() as u32).to_le_bytes())
        .map_err(|e| format!("写入失败: {e}"))?;
    file.write_all(OVERLAY_MAGIC)
        .map_err(|e| format!("写入失败: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------- PNG 盲水印

const BLOCK: usize = 8;
/// 嵌入强度：同一频率对内两个系数的差值目标
const DELTA: f64 = 28.0;
/// 每个比特重复嵌入的块数，取多数票
const REPEAT: usize = 5;
/// 载荷上限（超过就截断，构建 ID 实际只有 20 来字符）
const MAX_PAYLOAD: usize = 64;

/// 把文本写进 PNG 的 DCT 中频系数。过小的图片直接跳过（返回原数据）。
pub fn embed_png_watermark(png: &[u8], text: &str) -> Result<Vec<u8>, String> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|e| format!("解析 PNG 失败: {e}"))?;
    let mut rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let blocks_x = (width as usize) / BLOCK;
    let blocks_y = (height as usize) / BLOCK;
    if blocks_x == 0 || blocks_y == 0 {
        return Ok(png.to_vec());
    }

    let bytes = text.as_bytes();
    let len = bytes.len().min(MAX_PAYLOAD);
    let mut bits: Vec<u8> = Vec::with_capacity((len + 1) * 8);
    // 先放一字节长度（MSB 优先），再放正文
    for shift in (0..8).rev() {
        bits.push(((len as u8) >> shift) & 1);
    }
    for byte in &bytes[..len] {
        for shift in (0..8).rev() {
            bits.push((byte >> shift) & 1);
        }
    }

    // 块按行优先编号，每个比特重复占用 REPEAT 个连续块
    let capacity = (blocks_x * blocks_y) / REPEAT;
    if bits.len() > capacity {
        return Ok(png.to_vec());
    }

    for (index, bit) in bits.iter().enumerate() {
        for repeat in 0..REPEAT {
            let block = index * REPEAT + repeat;
            let bx = (block % blocks_x) * BLOCK;
            let by = (block / blocks_x) * BLOCK;
            embed_block(&mut rgba, bx, by, *bit);
        }
    }

    let mut out = Vec::new();
    rgba.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| format!("编码 PNG 失败: {e}"))?;
    Ok(out)
}

fn embed_block(rgba: &mut image::RgbaImage, x: usize, y: usize, bit: u8) {
    let mut plane = [[0f64; BLOCK]; BLOCK];
    for (row, line) in plane.iter_mut().enumerate() {
        for (col, cell) in line.iter_mut().enumerate() {
            let pixel = rgba.get_pixel((x + col) as u32, (y + row) as u32);
            // 亮度近似（BT.601）
            *cell = 0.299 * pixel[0] as f64 + 0.587 * pixel[1] as f64 + 0.114 * pixel[2] as f64;
        }
    }
    let base = dct2(&plane);
    let (a, b) = (base[3][2], base[2][3]);

    // 亮部 / 暗部被 0..255 截断会让码值翻转，这里逐级降低嵌入强度，
    // 最后还给不出不溢出的结果就整块放弃（交给重复块多数投票兜底）。
    for scale in [1.0f64, 0.7, 0.5, 0.35, 0.2] {
        let gap = DELTA * scale;
        let hi = a.max(b) + gap / 2.0;
        let lo = a.min(b) - gap / 2.0;
        let (target_a, target_b) = if bit == 1 { (hi, lo) } else { (lo, hi) };
        let mut coef = base;
        coef[3][2] = target_a;
        coef[2][3] = target_b;
        let pixels = idct2(&coef);

        let mut next = [[[0u8; 3]; BLOCK]; BLOCK];
        let mut clipped = false;
        for row in 0..BLOCK {
            for col in 0..BLOCK {
                let pixel = rgba.get_pixel((x + col) as u32, (y + row) as u32);
                let delta = pixels[row][col] - plane[row][col];
                for channel in 0..3 {
                    let value = pixel[channel] as f64 + delta;
                    if !(0.0..=255.0).contains(&value) {
                        clipped = true;
                    }
                    next[row][col][channel] = value.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        if clipped {
            continue;
        }
        for row in 0..BLOCK {
            for col in 0..BLOCK {
                let px = rgba.get_pixel_mut((x + col) as u32, (y + row) as u32);
                for channel in 0..3 {
                    px[channel] = next[row][col][channel];
                }
            }
        }
        return;
    }
}

/// 从 PNG 里读回盲水印文本（仅测试用）。
#[cfg(test)]
pub fn extract_png_watermark(png: &[u8]) -> Option<String> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let blocks_x = (width as usize) / BLOCK;
    let blocks_y = (height as usize) / BLOCK;
    if blocks_x == 0 || blocks_y == 0 {
        return None;
    }
    let read_bit = |index: usize| -> u8 {
        let mut ones = 0;
        let mut total = 0;
        for repeat in 0..REPEAT {
            let block = index * REPEAT + repeat;
            let bx = (block % blocks_x) * BLOCK;
            let by = (block / blocks_x) * BLOCK;
            if bx + BLOCK > width as usize || by + BLOCK > height as usize {
                continue;
            }
            let mut plane = [[0f64; BLOCK]; BLOCK];
            for (row, line) in plane.iter_mut().enumerate() {
                for (cell_x, cell) in line.iter_mut().enumerate() {
                    let pixel = rgba.get_pixel((bx + cell_x) as u32, (by + row) as u32);
                    *cell = 0.299 * pixel[0] as f64
                        + 0.587 * pixel[1] as f64
                        + 0.114 * pixel[2] as f64;
                }
            }
            let coef = dct2(&plane);
            total += 1;
            if coef[3][2] > coef[2][3] {
                ones += 1;
            }
        }
        if total == 0 {
            return 0;
        }
        if ones * 2 >= total {
            1
        } else {
            0
        }
    };

    let mut length = 0usize;
    for index in 0..8 {
        length = (length << 1) | read_bit(index) as usize;
    }
    if length == 0 || length > MAX_PAYLOAD {
        return None;
    }
    let mut out = Vec::with_capacity(length);
    for byte_index in 0..length {
        let mut value = 0u8;
        for bit_index in 0..8 {
            let index = 8 + byte_index * 8 + bit_index;
            value = (value << 1) | read_bit(index);
        }
        out.push(value);
    }
    let text = String::from_utf8(out).ok()?;
    if text.chars().all(|c| c.is_ascii_graphic()) {
        Some(text)
    } else {
        None
    }
}

/// 对 png 文件原地打盲水印（失败时不改动原文件）。
pub fn watermark_png_file(path: &Path, text: &str) -> Result<bool, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取图片失败: {e}"))?;
    if !bytes.starts_with(b"\x89PNG") {
        return Ok(false);
    }
    let marked = embed_png_watermark(&bytes, text)?;
    if marked.len() == bytes.len() && marked == bytes {
        return Ok(false);
    }
    std::fs::write(path, marked).map_err(|e| format!("写入图片失败: {e}"))?;
    Ok(true)
}

// ---------------------------------------------------------------- DCT 工具

fn dct2(input: &[[f64; BLOCK]; BLOCK]) -> [[f64; BLOCK]; BLOCK] {
    let mut tmp = [[0f64; BLOCK]; BLOCK];
    for row in 0..BLOCK {
        let coeffs = dct1(&input[row]);
        tmp[row] = coeffs;
    }
    let mut out = [[0f64; BLOCK]; BLOCK];
    for col in 0..BLOCK {
        let mut column = [0f64; BLOCK];
        for row in 0..BLOCK {
            column[row] = tmp[row][col];
        }
        let coeffs = dct1(&column);
        for row in 0..BLOCK {
            out[row][col] = coeffs[row];
        }
    }
    out
}

fn idct2(input: &[[f64; BLOCK]; BLOCK]) -> [[f64; BLOCK]; BLOCK] {
    let mut tmp = [[0f64; BLOCK]; BLOCK];
    for col in 0..BLOCK {
        let mut column = [0f64; BLOCK];
        for row in 0..BLOCK {
            column[row] = input[row][col];
        }
        let values = idct1(&column);
        for row in 0..BLOCK {
            tmp[row][col] = values[row];
        }
    }
    let mut out = [[0f64; BLOCK]; BLOCK];
    for row in 0..BLOCK {
        out[row] = idct1(&tmp[row]);
    }
    out
}

fn dct1(input: &[f64; BLOCK]) -> [f64; BLOCK] {
    let mut out = [0f64; BLOCK];
    for (u, slot) in out.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (x, value) in input.iter().enumerate() {
            sum += value * ((2.0 * x as f64 + 1.0) * u as f64 * std::f64::consts::PI
                / (2.0 * BLOCK as f64))
                .cos();
        }
        *slot = alpha(u) * sum;
    }
    out
}

fn idct1(input: &[f64; BLOCK]) -> [f64; BLOCK] {
    let mut out = [0f64; BLOCK];
    for (x, slot) in out.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (u, value) in input.iter().enumerate() {
            sum += alpha(u)
                * value
                * ((2.0 * x as f64 + 1.0) * u as f64 * std::f64::consts::PI / (2.0 * BLOCK as f64))
                    .cos();
        }
        *slot = sum;
    }
    out
}

fn alpha(u: usize) -> f64 {
    if u == 0 {
        (1.0 / BLOCK as f64).sqrt()
    } else {
        (2.0 / BLOCK as f64).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_meta() -> BuildMeta {
        BuildMeta::new(
            "jl-abc123-m3k9-1f2e".to_string(),
            "测试应用".to_string(),
            "com.jianling.test".to_string(),
            "1.2.3".to_string(),
            7,
            "android",
            "proj-1",
            "测试应用-1.2.3.apk",
            3,
        )
    }

    #[test]
    fn zero_width_roundtrip() {
        let id = "jl-abc123-m3k9-1f2e";
        let encoded = zero_width_encode(id);
        assert!(encoded.chars().all(|c| ZW_ALPHABET.contains(&c)));
        assert_eq!(zero_width_decode(&format!("前缀{encoded}后缀")).as_deref(), Some(id));
    }

    #[test]
    fn png_blind_watermark_roundtrip() {
        // 320×320 才够容量放 ~20 字符的构建 ID，亮度控制在中段避免 0/255 截断
        let mut img = image::RgbaImage::new(320, 320);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let value = 70 + ((x * 7 + y * 13) % 100) as u8;
            *pixel = image::Rgba([value, value + 20, value + 40, 255]);
        }
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();

        let id = "jl-abc123-m3k9-1f2e";
        let marked = embed_png_watermark(&png, id).unwrap();
        assert_ne!(marked, png, "水印未写入（容量不足？）");
        assert_eq!(extract_png_watermark(&marked).as_deref(), Some(id));
        assert_ne!(extract_png_watermark(&png).as_deref(), Some(id));
    }

    #[test]
    fn html_injection_is_idempotent() {
        let html = "<html><head><title>x</title></head><body></body></html>";
        let snippet = page_snippet(&sample_meta());
        let once = inject_into_html(html, &snippet);
        assert!(once.contains("jianling-build"));
        let twice = inject_into_html(&once, &snippet);
        assert_eq!(once, twice);
        assert!(zero_width_decode(&once).is_some());
    }

    #[test]
    fn page_meta_contains_full_info() {
        let html = inject_into_html(
            "<html><head></head><body></body></html>",
            &page_snippet(&sample_meta()),
        );
        let info = extract_build_info(&html).expect("页面里应有 build-info");
        let value: serde_json::Value = serde_json::from_str(&info).unwrap();
        assert_eq!(value["build"], "jl-abc123-m3k9-1f2e");
        assert_eq!(value["app"], "测试应用");
        assert_eq!(value["version"], "1.2.3");
        assert_eq!(value["versionCode"], 7);
        assert_eq!(value["platform"], "android");
        assert_eq!(value["project"], "proj-1");
        assert_eq!(value["proxyCount"], 3);
        assert!(value["host"].is_string());
        assert!(value["tool"].as_str().unwrap().starts_with("jianling "));
        let zw = zero_width_decode(&html).unwrap();
        assert!(zw.contains("jl-abc123-m3k9-1f2e"));
    }

    #[test]
    fn dct_roundtrip_is_exact() {
        let mut plane = [[0f64; BLOCK]; BLOCK];
        for row in 0..BLOCK {
            for col in 0..BLOCK {
                plane[row][col] = ((row * 7 + col * 13) % 50) as f64;
            }
        }
        let back = idct2(&dct2(&plane));
        for row in 0..BLOCK {
            for col in 0..BLOCK {
                let diff = (back[row][col] - plane[row][col]).abs();
                assert!(diff < 1e-6, "diff={diff} at {row},{col}");
            }
        }
    }
}
