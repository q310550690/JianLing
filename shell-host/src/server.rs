//! 本地静态包的内置服务：静态托管 + 接口转发（等价 vite proxy）。
//!
//! 让本地包页面跑在 `http://127.0.0.1:<端口>` 上，页面里的相对路径（`/api`、`/assets`）
//! 与页面天然同源，不需要后端配置 CORS，也不必修改前端的 `base`。
//!
//! 转发规则按前缀匹配（最长优先），并可选把 `Origin` 改写为上游自身的 Origin
//! —— 部分后端（如本项目的 nginx）会校验 Origin，不改就返回 403。

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde::Deserialize;

const MAX_HEAD: usize = 64 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(120);
/// WebSocket 隧道的轮询间隔（双向搬运靠短读超时实现）。
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// 单条转发规则。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Rule {
    pub prefix: String,
    pub target: String,
    pub rewrite_origin: bool,
    pub insecure: bool,
    /// 可选：把上游域名固定解析到该 IP（证书/SNI 仍用域名）
    pub resolve_ip: String,
    /// 可选：追加到上游请求的 header，JSON 对象文本
    pub headers: String,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            target: String::new(),
            rewrite_origin: true,
            insecure: false,
            resolve_ip: String::new(),
            headers: String::new(),
        }
    }
}

/// `shell.json` 里的 localServer 段。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub enabled: bool,
    /// 0 表示按站点目录派生固定端口（端口变化会让 localStorage / cookie 变成另一个 origin）
    pub port: u16,
    /// 部署目录，形如 "/" 或 "/h5/"：访问路径去掉这个前缀后再映射到文件
    pub base_path: String,
    pub proxies: Vec<Rule>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 0,
            base_path: "/".to_string(),
            proxies: Vec::new(),
        }
    }
}

/// 统一成「以 / 开头并以 / 结尾」的部署目录，空值即根目录 /。
pub fn normalize_base(raw: &str) -> String {
    let text = raw.trim().trim_matches('/');
    if text.is_empty() {
        "/".to_string()
    } else {
        format!("/{text}/")
    }
}

struct Upstream {
    tls: bool,
    host: String,
    port: u16,
    base_path: String,
    origin: String,
    rewrite_origin: bool,
    insecure: bool,
    /// 连接时使用的地址（空则用 host）
    resolve_ip: String,
    /// 追加到上游请求的 header（来自规则的 headers JSON，非法时为空）
    extra_headers: Vec<(String, String)>,
}

struct Ctx {
    root: PathBuf,
    index: String,
    base: String,
    rules: Vec<(String, Upstream)>,
}

/// 未显式配置端口时按站点根目录派生稳定端口：
/// localStorage / cookie / IndexedDB 都按 origin（含端口）隔离，端口随机等于换站点。
fn derive_port(seed: &str) -> u16 {
    let mut hash: u32 = 17;
    for byte in seed.as_bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(*byte as u32) & 0x7fff_ffff;
    }
    38080 + (hash % 200) as u16
}

/// 启动本地服务并返回实际端口（配置 0 时按根目录派生固定端口）。
pub fn start(cfg: &Config, root: PathBuf, index: String, logger: fn(&str)) -> Result<u16, String> {
    let mut rules: Vec<(String, Upstream)> = Vec::new();
    for rule in &cfg.proxies {
        let prefix = rule.prefix.trim();
        let target = rule.target.trim();
        if prefix.is_empty() || target.is_empty() {
            continue;
        }
        match parse_upstream(target, rule) {
            Ok(upstream) => {
                logger(&format!("本地服务转发：{prefix} → {target}"));
                rules.push((prefix.trim_end_matches('/').to_string(), upstream));
            }
            Err(e) => logger(&format!("转发规则无效（{target}）：{e}")),
        }
    }
    // 最长前缀优先
    rules.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    // 固定端口优先；被占用时顺延少量端口（仍可预测），不跳随机端口
    let base = if cfg.port > 0 {
        cfg.port
    } else {
        derive_port(&root.to_string_lossy())
    };
    let mut listener = None;
    for attempt in 0..10u16 {
        let candidate = base + attempt;
        match TcpListener::bind(("127.0.0.1", candidate)) {
            Ok(bound) => {
                if candidate != base {
                    logger(&format!(
                        "固定端口 {base} 被占用，改用 {candidate}：本次登录态（localStorage）可能与上次不同"
                    ));
                }
                listener = Some(bound);
                break;
            }
            Err(e) => logger(&format!("绑定 127.0.0.1:{candidate} 失败：{e}")),
        }
    }
    let listener = listener.ok_or_else(|| format!("本地服务端口 {base} 起连续 10 个端口都不可用"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("读取本地端口失败：{e}"))?
        .port();
    let base = normalize_base(&cfg.base_path);
    logger(&format!(
        "本地服务已启动：http://127.0.0.1:{port}{base}（根目录 {}）",
        root.display()
    ));

    let ctx = Arc::new(Ctx {
        root,
        index,
        base,
        rules,
    });
    std::thread::spawn(move || {
        for incoming in listener.incoming() {
            match incoming {
                Ok(stream) => {
                    let ctx = ctx.clone();
                    std::thread::spawn(move || handle(stream, &ctx, logger));
                }
                Err(e) => logger(&format!("本地服务接受连接失败：{e}")),
            }
        }
    });
    Ok(port)
}

// ---------------------------------------------------------------- 连接与缓冲

/// 带预读缓冲的连接：读头时可能一次读到 body，剩余字节必须留在缓冲区。
struct Conn<S> {
    inner: S,
    buf: Vec<u8>,
    pos: usize,
}

impl<S> Conn<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buf: Vec::new(),
            pos: 0,
        }
    }

    fn pending(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// 取出底层连接与尚未消费的字节（WebSocket 隧道移交时使用）。
    fn into_parts(self) -> (S, Vec<u8>) {
        let Conn {
            inner,
            mut buf,
            pos,
        } = self;
        if pos > 0 && pos <= buf.len() {
            buf.drain(..pos);
        }
        (inner, buf)
    }
}

impl<S: Read> Conn<S> {
    fn fill(&mut self) -> io::Result<usize> {
        if self.pos > 0 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        let mut chunk = [0u8; 16384];
        let n = self.inner.read(&mut chunk)?;
        self.buf.extend_from_slice(&chunk[..n]);
        Ok(n)
    }

    /// 读到 `\r\n\r\n` 为止，返回头部字节（不含结尾空行）。
    fn read_head(&mut self) -> io::Result<Vec<u8>> {
        loop {
            if let Some(idx) = find_head_end(&self.buf) {
                let head = self.buf[..idx].to_vec();
                self.pos = idx;
                return Ok(head);
            }
            if self.buf.len() > MAX_HEAD {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "请求头过大"));
            }
            if self.fill()? == 0 {
                if self.buf.is_empty() {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "空连接"));
                }
                let head = self.buf.clone();
                self.pos = self.buf.len();
                return Ok(head);
            }
        }
    }

    fn read_line(&mut self) -> io::Result<Vec<u8>> {
        loop {
            if let Some(idx) = self.buf[self.pos..].iter().position(|b| *b == b'\n') {
                let end = self.pos + idx + 1;
                let line = self.buf[self.pos..end].to_vec();
                self.pos = end;
                return Ok(line);
            }
            if self.buf.len() > MAX_HEAD {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "分块头过大"));
            }
            if self.fill()? == 0 {
                let line = self.buf[self.pos..].to_vec();
                self.pos = self.buf.len();
                return Ok(line);
            }
        }
    }

    fn copy_exact<W: Write>(&mut self, len: u64, out: &mut W) -> io::Result<u64> {
        let mut left = len;
        while left > 0 {
            if self.pending() == 0 && self.fill()? == 0 {
                break;
            }
            let n = self.pending().min(left as usize);
            let start = self.pos;
            out.write_all(&self.buf[start..start + n])?;
            out.flush()?;
            self.pos += n;
            left -= n as u64;
        }
        Ok(len - left)
    }

    /// 原样转发分块编码的 body，直到 0 长度块。
    fn copy_chunked<W: Write>(&mut self, out: &mut W) -> io::Result<()> {
        loop {
            let line = self.read_line()?;
            if line.is_empty() {
                return Ok(());
            }
            out.write_all(&line)?;
            let text = String::from_utf8_lossy(&line);
            let size_text = text.trim().split(';').next().unwrap_or("").trim();
            let size = u64::from_str_radix(size_text, 16).unwrap_or(0);
            if size == 0 {
                loop {
                    let trailer = self.read_line()?;
                    let empty = trailer == b"\r\n" || trailer == b"\n" || trailer.is_empty();
                    out.write_all(&trailer)?;
                    if empty {
                        break;
                    }
                }
                out.flush()?;
                return Ok(());
            }
            // 数据 + 结尾 CRLF
            self.copy_exact(size + 2, out)?;
        }
    }

    fn copy_until_eof<W: Write>(&mut self, out: &mut W) -> io::Result<u64> {
        let mut total = 0u64;
        loop {
            if self.pending() == 0 && self.fill()? == 0 {
                break;
            }
            let n = self.pending();
            let start = self.pos;
            out.write_all(&self.buf[start..start + n])?;
            out.flush()?;
            self.pos += n;
            total += n as u64;
        }
        Ok(total)
    }
}

impl<S: Read + Write> Conn<S> {
    fn send(&mut self, data: &[u8]) -> io::Result<()> {
        self.inner.write_all(data)?;
        self.inner.flush()
    }
}

impl<S: Write> Write for Conn<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|idx| idx + 4)
        .or_else(|| {
            buf.windows(2)
                .position(|w| w == b"\n\n")
                .map(|idx| idx + 2)
        })
}

// ---------------------------------------------------------------- 请求解析

struct Req {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
}

impl Req {
    fn parse(head: &[u8]) -> Option<Self> {
        let text = String::from_utf8_lossy(head);
        let mut lines = text.split("\r\n").flat_map(|l| l.split('\n'));
        let first = lines.next()?;
        let mut parts = first.split_whitespace();
        let method = parts.next()?.to_string();
        let path = parts.next()?.to_string();
        let mut headers = Vec::new();
        for line in lines {
            if line.is_empty() {
                continue;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.push((name.trim().to_string(), value.trim().to_string()));
            }
        }
        Some(Self {
            method,
            path,
            headers,
        })
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn path_only(&self) -> &str {
        self.path.split('?').next().unwrap_or("/")
    }
}

struct Resp {
    status: u16,
    headers: Vec<(String, String)>,
}

impl Resp {
    fn parse(head: &[u8]) -> Option<Self> {
        let text = String::from_utf8_lossy(head);
        let mut lines = text.split("\r\n").flat_map(|l| l.split('\n'));
        let first = lines.next()?;
        let mut parts = first.split_whitespace();
        let _version = parts.next()?;
        let status = parts.next()?.parse().ok()?;
        let mut headers = Vec::new();
        for line in lines {
            if line.is_empty() {
                continue;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.push((name.trim().to_string(), value.trim().to_string()));
            }
        }
        Some(Self { status, headers })
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn has_body(&self) -> bool {
        !(self.status < 200 || self.status == 204 || self.status == 304)
    }

    fn is_chunked(&self) -> bool {
        self.header("transfer-encoding")
            .map(|v| v.to_ascii_lowercase().contains("chunked"))
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------- 入口处理

fn handle(stream: TcpStream, ctx: &Ctx, logger: fn(&str)) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_nodelay(true);

    let mut conn = Conn::new(stream);
    let head = match conn.read_head() {
        Ok(head) => head,
        Err(_) => return,
    };
    let Some(req) = Req::parse(&head) else {
        let _ = plain(&mut conn, 400, "Bad Request", "请求格式无法解析");
        return;
    };

    let is_upgrade = req
        .header("upgrade")
        .map(|v| v.to_ascii_lowercase().contains("websocket"))
        .unwrap_or(false);

    if let Some((_prefix, upstream)) = ctx
        .rules
        .iter()
        .find(|(prefix, _)| req.path_only().starts_with(prefix.as_str()))
    {
        if is_upgrade {
            if let Err(e) = ws_tunnel(&req, conn, upstream) {
                logger(&format!("WebSocket 隧道结束 {}：{e}", req.path));
            }
            return;
        }
        if let Err(e) = proxy(&req, &mut conn, upstream) {
            logger(&format!("转发失败 {}：{e}", req.path));
            let _ = plain(&mut conn, 502, "Bad Gateway", &format!("上游请求失败：{e}"));
        }
        return;
    }

    if is_upgrade {
        logger(&format!(
            "WebSocket 请求没有匹配的转发规则，无法建立隧道：{}",
            req.path
        ));
        let _ = plain(
            &mut conn,
            501,
            "WebSocket 未转发",
            "该路径没有配置转发规则，请在前缀列表里补上",
        );
        return;
    }

    if let Err(e) = serve_static(&req, &mut conn, ctx) {
        let _ = e;
    }
}

fn plain<S: Read + Write>(
    conn: &mut Conn<S>,
    status: u16,
    title: &str,
    detail: &str,
) -> io::Result<()> {
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><body style=\"font:14px/1.7 system-ui;padding:24px\"><h3>{title}</h3><p>{detail}</p></body>"
    );
    let head = format!(
        "HTTP/1.1 {status} {title}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.as_bytes().len()
    );
    conn.send(head.as_bytes())?;
    conn.send(body.as_bytes())
}

// ---------------------------------------------------------------- 静态托管

fn serve_static(req: &Req, conn: &mut Conn<TcpStream>, ctx: &Ctx) -> io::Result<()> {
    serve_static_inner(req, conn, &ctx.root, &ctx.index, &ctx.base)
}
fn serve_static_inner(
    req: &Req,
    conn: &mut Conn<TcpStream>,
    root: &PathBuf,
    index: &str,
    base: &str,
) -> io::Result<()> {
    if req.method != "GET" && req.method != "HEAD" {
        return plain(conn, 405, "Method Not Allowed", "静态资源仅支持 GET / HEAD");
    }
    let decoded = strip_base(&percent_decode(req.path_only()), base);
    let mut relative: Vec<String> = Vec::new();
    for segment in decoded.split('/') {
        match segment {
            "" | "." => continue,
            ".." => return plain(conn, 403, "Forbidden", "路径非法"),
            other => relative.push(other.to_string()),
        }
    }
    let mut full = root.clone();
    for segment in &relative {
        full.push(segment);
    }
    if relative.is_empty() || full.is_dir() {
        full.push(index);
    }
    let mut file = match std::fs::File::open(&full) {
        Ok(file) => file,
        Err(_) => {
            // 无扩展名的路径按前端路由兜底到首页（hash 路由一般用不到）
            let has_ext = full
                .extension()
                .map(|e| !e.is_empty())
                .unwrap_or(false);
            if !has_ext {
                let fallback = root.join(index);
                match std::fs::File::open(&fallback) {
                    Ok(file) => {
                        full = fallback;
                        file
                    }
                    Err(_) => return plain(conn, 404, "Not Found", "文件不存在"),
                }
            } else {
                return plain(conn, 404, "Not Found", "文件不存在");
            }
        }
    };

    let total = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mime = mime_of(&full);
    let range = req.header("range").and_then(|value| parse_range(value, total));

    let (status, start, len) = match range {
        Some((start, end)) => (206u16, start, end - start + 1),
        None => (200u16, 0, total),
    };

    let mut head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {mime}\r\nAccept-Ranges: bytes\r\nContent-Length: {len}\r\nCache-Control: no-cache\r\nConnection: close\r\n",
        if status == 206 { "Partial Content" } else { "OK" }
    );
    if status == 206 {
        head.push_str(&format!(
            "Content-Range: bytes {}-{}/{total}\r\n",
            start,
            start + len - 1
        ));
    }
    head.push_str("\r\n");
    conn.send(head.as_bytes())?;

    if req.method == "HEAD" {
        return Ok(());
    }
    if start > 0 {
        use std::io::Seek;
        file.seek(std::io::SeekFrom::Start(start))?;
    }
    let mut remaining = len;
    let mut chunk = vec![0u8; 32 * 1024];
    while remaining > 0 {
        let want = remaining.min(chunk.len() as u64) as usize;
        let read = file.read(&mut chunk[..want])?;
        if read == 0 {
            break;
        }
        conn.send(&chunk[..read])?;
        remaining -= read as u64;
    }
    Ok(())
}

fn parse_range(value: &str, total: u64) -> Option<(u64, u64)> {
    let spec = value.trim().strip_prefix("bytes=")?;
    let spec = spec.split(',').next()?.trim();
    let (start_text, end_text) = spec.split_once('-')?;
    if total == 0 {
        return None;
    }
    let (start, end) = if start_text.is_empty() {
        // bytes=-N：最后 N 字节
        let suffix: u64 = end_text.trim().parse().ok()?;
        if suffix == 0 {
            return None;
        }
        (total.saturating_sub(suffix), total - 1)
    } else {
        let start: u64 = start_text.trim().parse().ok()?;
        let end = if end_text.trim().is_empty() {
            total - 1
        } else {
            end_text.trim().parse::<u64>().ok()?.min(total - 1)
        };
        (start, end)
    };
    if start > end || start >= total {
        return None;
    }
    Some((start, end))
}

/// 去掉部署目录前缀：/h5/assets/a.js → /assets/a.js；路径不带前缀时原样使用。
fn strip_base(path: &str, base: &str) -> String {
    if base == "/" {
        return path.to_string();
    }
    if let Some(rest) = path.strip_prefix(base) {
        return format!("/{rest}");
    }
    if path == base.trim_end_matches('/') {
        return "/".to_string();
    }
    path.to_string()
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &input[index + 1..index + 3];
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn mime_of(path: &std::path::Path) -> &'static str {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "eot" => "application/vnd.ms-fontobject",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "wasm" => "application/wasm",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "apk" => "application/vnd.android.package-archive",
        _ => "application/octet-stream",
    }
}

// ---------------------------------------------------------------- 转发

/// 组装发给上游的请求头。`keep_upgrade` 为 true 时保留升级握手所需的头。
fn build_head(req: &Req, upstream: &Upstream, keep_upgrade: bool) -> String {
    let host_header = if (upstream.tls && upstream.port == 443)
        || (!upstream.tls && upstream.port == 80)
    {
        upstream.host.clone()
    } else {
        format!("{}:{}", upstream.host, upstream.port)
    };
    let mut head = String::new();
    head.push_str(&format!(
        "{} {}{} HTTP/1.1\r\n",
        req.method, upstream.base_path, req.path
    ));
    head.push_str(&format!("Host: {host_header}\r\n"));
    for (name, value) in &req.headers {
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "host" | "proxy-connection" | "keep-alive" | "expect" => continue,
            "connection" => {
                if keep_upgrade {
                    head.push_str(&format!("{name}: {value}\r\n"));
                }
                continue;
            }
            "origin" => {
                if upstream.rewrite_origin {
                    head.push_str(&format!("Origin: {}\r\n", upstream.origin));
                } else {
                    head.push_str(&format!("{name}: {value}\r\n"));
                }
                continue;
            }
            _ => {}
        }
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    for (name, value) in &upstream.extra_headers {
        match name.to_ascii_lowercase().as_str() {
            "host" | "content-length" | "transfer-encoding" | "connection" => continue,
            _ => {}
        }
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if !keep_upgrade {
        head.push_str("Connection: close\r\n");
    }
    head.push_str("\r\n");
    head
}

fn proxy(req: &Req, conn: &mut Conn<TcpStream>, upstream: &Upstream) -> Result<(), String> {
    let mut up = Conn::new(connect(upstream)?);
    let head = build_head(req, upstream, false);
    up.send(head.as_bytes())
        .map_err(|e| format!("写入上游失败：{e}"))?;

    // 客户端要 100-continue 时由本地服务自己应答，否则它会等不到继续响应而不发 body
    let expect_continue = req
        .header("expect")
        .map(|v| v.to_ascii_lowercase().contains("100-continue"))
        .unwrap_or(false);
    if expect_continue {
        conn.send(b"HTTP/1.1 100 Continue\r\n\r\n")
            .map_err(|e| format!("回写 100 Continue 失败：{e}"))?;
    }

    // 请求 body：按框架原样透传，支持 JSON、multipart、分块上传
    let chunked_request = req
        .header("transfer-encoding")
        .map(|v| v.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);
    if chunked_request {
        conn.copy_chunked(&mut up)
            .map_err(|e| format!("转发请求体失败：{e}"))?;
    } else if let Some(len) = req
        .header("content-length")
        .and_then(|v| v.trim().parse::<u64>().ok())
    {
        conn.copy_exact(len, &mut up)
            .map_err(|e| format!("转发请求体失败：{e}"))?;
    }
    up.flush().ok();

    // 响应头
    let resp_head = up.read_head().map_err(|e| format!("读取上游响应失败：{e}"))?;
    let resp = Resp::parse(&resp_head).ok_or("上游响应无法解析")?;
    let mut out = format!("HTTP/1.1 {} X\r\n", resp.status);
    for (name, value) in &resp.headers {
        if name.eq_ignore_ascii_case("connection")
            || name.eq_ignore_ascii_case("keep-alive")
            || name.eq_ignore_ascii_case("transfer-encoding")
        {
            continue;
        }
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    if resp.is_chunked() {
        out.push_str("Transfer-Encoding: chunked\r\n");
    }
    out.push_str("Connection: close\r\n\r\n");
    conn.send(out.as_bytes())
        .map_err(|e| format!("回写响应头失败：{e}"))?;

    if !resp.has_body() {
        return Ok(());
    }
    if resp.is_chunked() {
        up.copy_chunked(conn)
            .map_err(|e| format!("转发响应体失败：{e}"))?;
    } else if let Some(len) = resp
        .header("content-length")
        .and_then(|v| v.trim().parse::<u64>().ok())
    {
        up.copy_exact(len, conn)
            .map_err(|e| format!("转发响应体失败：{e}"))?;
    } else {
        up.copy_until_eof(conn)
            .map_err(|e| format!("转发响应体失败：{e}"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------- WebSocket 隧道

fn retryable(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

/// WebSocket 转发：改写信手头后建立隧道，之后按原始字节双向搬运。
///
/// 上游是 TLS 时无法把连接拆给两个线程，所以用「短读超时 + 单线程轮询」实现双向搬运。
fn ws_tunnel(req: &Req, conn: Conn<TcpStream>, upstream: &Upstream) -> Result<(), String> {
    let (mut client, client_pending) = conn.into_parts();
    let mut up = Conn::new(connect(upstream)?);

    let head = build_head(req, upstream, true);
    up.send(head.as_bytes())
        .map_err(|e| format!("写入握手请求失败：{e}"))?;

    let resp_head = up.read_head().map_err(|e| format!("读取握手响应失败：{e}"))?;
    let resp = Resp::parse(&resp_head).ok_or("上游握手响应无法解析")?;

    // 握手响应原样回写，保留 Sec-WebSocket-Accept / Connection: Upgrade
    client
        .write_all(&resp_head)
        .map_err(|e| format!("回写握手响应失败：{e}"))?;
    client.flush().ok();
    if resp.status != 101 {
        return Ok(());
    }

    let (mut up_io, up_pending) = up.into_parts();
    if !up_pending.is_empty() {
        client
            .write_all(&up_pending)
            .map_err(|e| format!("回写首包失败：{e}"))?;
    }
    if !client_pending.is_empty() {
        up_io
            .write_all(&client_pending)
            .map_err(|e| format!("转发首包失败：{e}"))?;
        up_io.flush().ok();
    }

    client
        .set_read_timeout(Some(POLL_INTERVAL))
        .map_err(|e| e.to_string())?;
    client
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|e| e.to_string())?;
    up_io
        .set_read_timeout(Some(POLL_INTERVAL))
        .map_err(|e| e.to_string())?;
    up_io
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|e| e.to_string())?;

    let mut client_buf = vec![0u8; 16 * 1024];
    let mut up_buf = vec![0u8; 16 * 1024];
    loop {
        match client.read(&mut client_buf) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                up_io
                    .write_all(&client_buf[..n])
                    .map_err(|e| format!("隧道上行写入失败：{e}"))?;
                up_io.flush().ok();
            }
            Err(e) if retryable(&e) => {}
            Err(e) => return Err(format!("隧道上行读取失败：{e}")),
        }
        match up_io.read(&mut up_buf) {
            Ok(0) => return Ok(()),
            Ok(n) => {
                client
                    .write_all(&up_buf[..n])
                    .map_err(|e| format!("隧道下行写入失败：{e}"))?;
                client.flush().ok();
            }
            Err(e) if retryable(&e) => {}
            Err(e) => return Err(format!("隧道下行读取失败：{e}")),
        }
    }
}

enum Io {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Io {
    fn socket(&self) -> &TcpStream {
        match self {
            Io::Plain(stream) => stream,
            Io::Tls(stream) => &stream.sock,
        }
    }

    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.socket().set_read_timeout(timeout)
    }

    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.socket().set_write_timeout(timeout)
    }
}

impl Read for Io {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Io::Plain(stream) => stream.read(buf),
            Io::Tls(stream) => stream.read(buf),
        }
    }
}

impl Write for Io {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Io::Plain(stream) => stream.write(buf),
            Io::Tls(stream) => stream.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Io::Plain(stream) => stream.flush(),
            Io::Tls(stream) => stream.flush(),
        }
    }
}

fn connect(upstream: &Upstream) -> Result<Io, String> {
    // resolveIp 非空时按固定 IP 建连，TLS 的 SNI / 证书校验仍用域名
    let dial = if upstream.resolve_ip.is_empty() {
        upstream.host.as_str()
    } else {
        upstream.resolve_ip.as_str()
    };
    let stream = TcpStream::connect((dial, upstream.port))
        .map_err(|e| format!("连接 {dial}:{} 失败：{e}", upstream.port))?;
    let _ = stream.set_nodelay(true);
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|e| e.to_string())?;
    if !upstream.tls {
        return Ok(Io::Plain(stream));
    }
    let config = tls_config(upstream.insecure)?;
    let name = rustls::pki_types::ServerName::try_from(upstream.host.clone())
        .map_err(|e| format!("域名无法用于 TLS：{e}"))?;
    let session =
        rustls::ClientConnection::new(config, name).map_err(|e| format!("TLS 初始化失败：{e}"))?;
    Ok(Io::Tls(Box::new(rustls::StreamOwned::new(session, stream))))
}

fn tls_config(insecure: bool) -> Result<Arc<rustls::ClientConfig>, String> {
    static VERIFIED: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    static UNVERIFIED: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();

    let cache = if insecure { &UNVERIFIED } else { &VERIFIED };
    if let Some(config) = cache.get() {
        return Ok(config.clone());
    }
    install_provider();

    let config = if insecure {
        let provider = rustls::crypto::ring::default_provider();
        let verifier = Arc::new(NoVerify(provider));
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth()
    } else {
        let mut roots = rustls::RootCertStore::empty();
        let native = rustls_native_certs::load_native_certs();
        for cert in native.certs {
            let _ = roots.add(cert);
        }
        if roots.is_empty() {
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
        if roots.is_empty() {
            return Err("没有可用的根证书".to_string());
        }
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    };
    let shared = Arc::new(config);
    let _ = cache.set(shared.clone());
    Ok(shared)
}

fn install_provider() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// `insecure` 开关用：跳过证书链与签名校验（仅用于内网自签服务）。
#[derive(Debug)]
struct NoVerify(rustls::crypto::CryptoProvider);

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn parse_upstream(target: &str, rule: &Rule) -> Result<Upstream, String> {
    let (scheme, rest) = target.split_once("://").ok_or("缺少 http:// 或 https:// 前缀")?;
    let scheme = scheme.to_ascii_lowercase();
    let (authority, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, ""),
    };
    if authority.is_empty() {
        return Err("缺少域名".to_string());
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => (
            h.to_string(),
            p.parse::<u16>().map_err(|_| "端口不合法")?,
        ),
        _ => (
            authority.to_string(),
            if scheme == "https" || scheme == "wss" {
                443
            } else {
                80
            },
        ),
    };
    let tls = scheme == "https" || scheme == "wss";
    let default_port = if tls { 443 } else { 80 };
    let origin = if port == default_port {
        format!("{}://{}", if tls { "https" } else { "http" }, host)
    } else {
        format!("{}://{}:{}", if tls { "https" } else { "http" }, host, port)
    };
    Ok(Upstream {
        tls,
        host,
        port,
        base_path: path.trim_end_matches('/').to_string(),
        origin,
        rewrite_origin: rule.rewrite_origin,
        insecure: rule.insecure,
        resolve_ip: rule.resolve_ip.trim().to_string(),
        extra_headers: parse_extra_headers(&rule.headers),
    })
}

/// 解析规则里的 headers JSON（对象，值为字符串）；非法内容返回空表并打日志，转发本身不受影响。
fn parse_extra_headers(text: &str) -> Vec<(String, String)> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Object(map)) => map
            .into_iter()
            .filter_map(|(name, value)| match value {
                serde_json::Value::String(s) => Some((name, s)),
                serde_json::Value::Null => None,
                other => Some((name, other.to_string())),
            })
            .collect(),
        Ok(_) => {
            eprintln!("转发规则 headers 不是 JSON 对象，已忽略：{text}");
            Vec::new()
        }
        Err(err) => {
            eprintln!("转发规则 headers JSON 解析失败，已忽略：{err}");
            Vec::new()
        }
    }
}
