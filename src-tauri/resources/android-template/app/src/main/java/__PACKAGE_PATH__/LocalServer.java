package __APPLICATION_ID__;

import android.content.Context;
import android.content.res.AssetFileDescriptor;
import android.content.res.AssetManager;
import android.util.Log;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.ByteArrayOutputStream;
import java.io.Closeable;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.net.URLDecoder;
import java.nio.charset.StandardCharsets;
import java.security.SecureRandom;
import java.security.cert.X509Certificate;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.Date;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;

import javax.net.ssl.HttpsURLConnection;
import javax.net.ssl.SNIHostName;
import javax.net.ssl.SNIServerName;
import javax.net.ssl.SSLContext;
import javax.net.ssl.SSLParameters;
import javax.net.ssl.SSLSocket;
import javax.net.ssl.SSLSocketFactory;
import javax.net.ssl.TrustManager;
import javax.net.ssl.X509TrustManager;

/**
 * 本地静态包的内置服务：静态托管 + 接口转发（等价 vite proxy）。
 *
 * <p>让本地包页面跑在 http://127.0.0.1:端口 上，页面里的相对路径（/api、/assets）
 * 与页面天然同源：既不需要后端配置 CORS，也不必修改前端 base。
 *
 * <p>转发规则来自 assets/jianling-local-server.json，按前缀匹配（最长优先），
 * 可把 Origin 改写为上游自身 Origin；http(s) 与 ws(s) 都支持。
 */
public class LocalServer {
    private static final String TAG = "JianLingLocal";
    private static final String CONFIG_ASSET = "jianling-local-server.json";
    private static final int HEAD_LIMIT = 64 * 1024;
    private static final int BUF_SIZE = 16 * 1024;
    private static final int SOCKET_TIMEOUT = 120000;
    /**
     * WS 隧道空闲上限（毫秒）：只有「两个方向都完全静默」超过该时长才判定为死连接。
     * 单侧静音（长下载、只说不听）不算，见 {@link WsActivity}。
     */
    private static final int WS_IDLE_TIMEOUT = 300000;
    /** 每条转发规则保留的空闲上游连接数（避免每个请求都重新 TLS 握手） */
    private static final int MAX_IDLE_PER_RULE = 2;
    private static final byte[] CRLF = "\r\n".getBytes(StandardCharsets.ISO_8859_1);

    private final AssetManager assets;
    /** 非空时优先从该目录读页面资源（调试期热推的 dist），缺失的文件回退 APK 内 assets */
    private final File fileRoot;
    private final String rootPrefix;
    private final String indexName;
    private final String basePath;
    private final int preferredPort;
    /** 应用包名：端口未固定时用它派生稳定端口，保证 localStorage 不因端口变化而"丢失" */
    private final String appId;
    private final List<Rule> rules = new ArrayList<Rule>();
    /** 调试包下的流量日志；正式包恒为 null，不产生文件 */
    private final TrafficLog traffic;

    private ServerSocket serverSocket;
    private volatile boolean running;
    private static volatile SSLSocketFactory insecureFactory;

    public LocalServer(Context context, String rootPrefix, String indexName) throws Exception {
        this(context, null, rootPrefix, indexName);
    }

    /**
     * @param fileRoot 外部资源根目录（如 getExternalFilesDir(null)），非空时优先于 assets；
     *                 缺失的文件仍回退 APK 内 assets，方便只推部分文件。
     */
    public LocalServer(Context context, File fileRoot, String rootPrefix, String indexName)
            throws Exception {
        this.fileRoot = fileRoot;
        this.assets = context.getAssets();
        this.rootPrefix = rootPrefix;
        this.indexName = indexName;

        JSONObject cfg = readConfig();
        if (!cfg.optBoolean("enabled", true)) {
            throw new IllegalStateException("本地服务未启用");
        }
        this.preferredPort = cfg.optInt("port", 0);
        this.appId = context.getPackageName();
        this.traffic = TrafficLog.create(context, cfg.optBoolean("debugLog", false));
        this.basePath = normalizeBase(cfg.optString("basePath", "/"));
        JSONArray arr = cfg.optJSONArray("proxies");
        if (arr != null) {
            for (int i = 0; i < arr.length(); i++) {
                JSONObject item = arr.optJSONObject(i);
                if (item == null) {
                    continue;
                }
                Rule rule = parseRule(item);
                if (rule != null) {
                    rules.add(rule);
                }
            }
        }
        Collections.sort(rules, new Comparator<Rule>() {
            @Override
            public int compare(Rule a, Rule b) {
                return b.prefix.length() - a.prefix.length();
            }
        });
        for (Rule rule : rules) {
            Log.i(TAG, "转发规则: " + rule.prefix + " -> " + rule.host + ":" + rule.port + rule.basePath);
        }
    }

    /**
     * 未显式配置端口时，按包名派生一个稳定端口（每次启动都一样）。
     *
     * <p>localStorage / cookie / IndexedDB 都按 origin（含端口）隔离，
     * 端口每次随机就等价于每次换一个站点，登录态自然读不到。
     */
    private static int derivePort(String seed) {
        int hash = 17;
        if (seed != null) {
            for (int i = 0; i < seed.length(); i++) {
                hash = (hash * 31 + seed.charAt(i)) & 0x7fffffff;
            }
        }
        return 38080 + (hash % 200);
    }

    public int start() throws IOException {
        // 固定端口优先；被占用时顺延少量端口（仍可预测），不跳随机端口
        int base = preferredPort > 0 ? preferredPort : derivePort(appId);
        IOException lastError = null;
        for (int attempt = 0; attempt < 10; attempt++) {
            int port = base + attempt;
            try {
                ServerSocket socket = new ServerSocket();
                socket.setReuseAddress(true);
                socket.bind(new InetSocketAddress(InetAddress.getByName("127.0.0.1"), port));
                serverSocket = socket;
                lastError = null;
                if (port != base) {
                    Log.w(TAG, "固定端口 " + base + " 被占用，改用 " + port
                            + "：本次登录态（localStorage）可能与上次不同");
                }
                break;
            } catch (IOException e) {
                lastError = e;
                Log.w(TAG, "本地服务端口 " + port + " 绑定失败: " + e.getMessage());
            }
        }
        if (serverSocket == null) {
            throw lastError != null ? lastError : new IOException("本地服务启动失败");
        }
        running = true;
        Thread loop = new Thread(new Runnable() {
            @Override
            public void run() {
                acceptLoop();
            }
        }, "jianling-local-server");
        loop.setDaemon(true);
        loop.start();
        Log.i(TAG, "本地服务已启动: http://127.0.0.1:" + getPort());
        return getPort();
    }

    public int getPort() {
        ServerSocket socket = serverSocket;
        return socket == null ? 0 : socket.getLocalPort();
    }

    /** 部署目录前缀，形如 "/" 或 "/h5/"，页面地址要带上它。 */
    public String getBasePath() {
        return basePath;
    }

    /** 统一成「以 / 开头并以 / 结尾」的部署目录，空值即根目录 /。 */
    private static String normalizeBase(String raw) {
        String text = raw == null ? "" : raw.trim();
        while (text.startsWith("/")) {
            text = text.substring(1);
        }
        while (text.endsWith("/")) {
            text = text.substring(0, text.length() - 1);
        }
        return text.isEmpty() ? "/" : "/" + text + "/";
    }

    /** 去掉部署目录前缀：/h5/assets/a.js → /assets/a.js；不带前缀时原样使用。 */
    private String stripBase(String path) {
        if ("/".equals(basePath) || path == null) {
            return path;
        }
        if (path.startsWith(basePath)) {
            return "/" + path.substring(basePath.length());
        }
        String trimmed = basePath.substring(0, basePath.length() - 1);
        if (trimmed.equals(path)) {
            return "/";
        }
        return path;
    }

    public void stop() {
        running = false;
        closeQuietly(serverSocket);
        serverSocket = null;
        if (traffic != null) {
            traffic.close();
        }
        for (Rule rule : rules) {
            synchronized (rule.idle) {
                for (Upstream up : rule.idle) {
                    up.close();
                }
                rule.idle.clear();
            }
        }
    }

    private void acceptLoop() {
        while (running) {
            try {
                final Socket client = serverSocket.accept();
                Thread worker = new Thread(new Runnable() {
                    @Override
                    public void run() {
                        handle(client);
                    }
                }, "jianling-local-conn");
                worker.setDaemon(true);
                worker.start();
            } catch (IOException e) {
                if (running) {
                    Log.w(TAG, "接受连接失败: " + e.getMessage());
                }
            }
        }
    }

    private void handle(Socket socket) {
        try {
            socket.setSoTimeout(SOCKET_TIMEOUT);
            socket.setTcpNoDelay(true);
            InputStream in = new BufferedInputStream(socket.getInputStream(), BUF_SIZE);
            OutputStream out = socket.getOutputStream();

            byte[] head = readHead(in);
            if (head == null) {
                return;
            }
            Req req = Req.parse(head);
            if (req == null) {
                plain(out, 400, "Bad Request", "请求格式无法解析");
                return;
            }
            Rule rule = matchRule(req.pathOnly());
            if (traffic != null && "/__jianling-logs".equals(req.pathOnly())) {
                serveTrafficLog(out, req);
                return;
            }
            if (rule != null && req.isUpgrade()) {
                wsTunnel(socket, in, out, req, rule);
                return;
            }
            if (rule != null) {
                httpProxy(in, out, req, rule);
                return;
            }
            if (req.isUpgrade()) {
                plain(out, 501, "WebSocket 未转发", "该路径没有配置转发规则，请在前缀列表里补上");
                return;
            }
            serveStatic(in, out, req);
        } catch (Exception e) {
            Log.w(TAG, "处理请求失败: " + e.getMessage());
        } finally {
            closeQuietly(socket);
        }
    }

    // ------------------------------------------------------------ 静态托管

    /**
     * 调试包专用：在 http://127.0.0.1:端口/__jianling-logs 直接查看流量日志，
     * 加 ?file=文件名 查看单个文件内容。
     */
    private void serveTrafficLog(OutputStream out, Req req) throws IOException {
        String file = null;
        int queryAt = req.path.indexOf('?');
        if (queryAt >= 0) {
            for (String pair : req.path.substring(queryAt + 1).split("&")) {
                if (pair.startsWith("file=")) {
                    file = URLDecoder.decode(pair.substring(5), "UTF-8");
                }
            }
        }
        StringBuilder sb = new StringBuilder(4096);
        if (file == null || file.length() == 0) {
            sb.append("流量日志目录: ").append(traffic.dir.getAbsolutePath()).append("\r\n\r\n");
            File[] files = traffic.dir.listFiles();
            if (files != null) {
                for (File f : files) {
                    sb.append(f.getName()).append("  ").append(f.length()).append("B\r\n")
                            .append("  http://127.0.0.1:").append(getPort())
                            .append("/__jianling-logs?file=").append(f.getName()).append("\r\n");
                }
            }
        } else {
            File base = traffic.dir.getCanonicalFile();
            File target = new File(base, file).getCanonicalFile();
            if (!base.equals(target.getParentFile()) || !target.isFile()) {
                plain(out, 404, "Not Found", "文件不存在");
                return;
            }
            if (target.length() > 8L * 1024 * 1024) {
                sb.append("文件过大（").append(target.length()).append("B），请用 adb pull 取回\r\n");
            } else {
                byte[] data = readFile(target);
                if (file.endsWith(".log")) {
                    sb.append(new String(data, StandardCharsets.UTF_8));
                } else {
                    sb.append("采样数据 ").append(data.length).append("B，前 512 字节：\r\n")
                            .append(TrafficLog.hex(data, Math.min(512, data.length)));
                }
            }
        }
        byte[] body = sb.toString().getBytes(StandardCharsets.UTF_8);
        out.write(("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: "
                + body.length + "\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.ISO_8859_1));
        out.write(body);
        out.flush();
    }

    private static byte[] readFile(File file) throws IOException {
        InputStream in = new FileInputStream(file);
        try {
            ByteArrayOutputStream buffer = new ByteArrayOutputStream((int) Math.min(file.length(), 1 << 20));
            byte[] chunk = new byte[BUF_SIZE];
            int read;
            while ((read = in.read(chunk)) > 0) {
                buffer.write(chunk, 0, read);
            }
            return buffer.toByteArray();
        } finally {
            closeQuietly(in);
        }
    }

    private void serveStatic(InputStream in, OutputStream out, Req req) throws IOException {
        if (!"GET".equals(req.method) && !"HEAD".equals(req.method)) {
            plain(out, 405, "Method Not Allowed", "静态资源仅支持 GET / HEAD");
            return;
        }
        StringBuilder rel = new StringBuilder(rootPrefix);
        boolean empty = true;
        String decoded = URLDecoder.decode(stripBase(req.pathOnly()), "UTF-8");
        for (String segment : decoded.split("/")) {
            if (segment.length() == 0 || ".".equals(segment)) {
                continue;
            }
            if ("..".equals(segment)) {
                plain(out, 403, "Forbidden", "路径非法");
                return;
            }
            rel.append('/').append(segment);
            empty = false;
        }
        if (empty) {
            rel.append('/').append(indexName);
        }

        Asset asset = openAsset(rel.toString());
        if (asset == null) {
            String relText = rel.toString();
            int slash = relText.lastIndexOf('/');
            String last = slash >= 0 ? relText.substring(slash + 1) : relText;
            if (last.indexOf('.') < 0) {
                // 无扩展名的路径按前端路由兜底到首页
                asset = openAsset(rootPrefix + "/" + indexName);
            }
        }
        if (asset == null) {
            plain(out, 404, "Not Found", "文件不存在");
            return;
        }

        try {
            String mime = mimeOf(rel.toString());
            long total = asset.length;
            long[] range = total > 0 ? parseRange(req.header("range"), total) : null;
            int status = range == null ? 200 : 206;
            long start = range == null ? 0 : range[0];
            long length = range == null ? total : range[1] - range[0] + 1;

            StringBuilder head = new StringBuilder(256);
            head.append("HTTP/1.1 ").append(status).append(status == 206 ? " Partial Content" : " OK").append("\r\n");
            head.append("Content-Type: ").append(mime).append("\r\n");
            // 热推目录（files/www）随时会被「重新推送资源」整体替换，必须禁用缓存，
            // 否则冷启动仍可能命中 WebView 磁盘缓存里的旧 JS/CSS。
            head.append(fileRoot != null ? "Cache-Control: no-store\r\n" : "Cache-Control: no-cache\r\n");
            head.append("Connection: close\r\n");
            if (length >= 0) {
                head.append("Accept-Ranges: bytes\r\n");
                head.append("Content-Length: ").append(length).append("\r\n");
                if (status == 206) {
                    head.append("Content-Range: bytes ").append(start).append('-')
                            .append(start + length - 1).append('/').append(total).append("\r\n");
                }
            } else {
                head.append("Transfer-Encoding: chunked\r\n");
            }
            head.append("\r\n");
            out.write(head.toString().getBytes(StandardCharsets.UTF_8));
            out.flush();

            if ("HEAD".equals(req.method)) {
                return;
            }
            if (start > 0) {
                skipFully(asset.in, start);
            }
            if (length >= 0) {
                copyExact(asset.in, out, length);
            } else {
                writeChunked(asset.in, out);
            }
            out.flush();
        } finally {
            closeQuietly(asset.in);
        }
    }

    private static final class Asset {
        final InputStream in;
        final long length;

        Asset(InputStream in, long length) {
            this.in = in;
            this.length = length;
        }
    }

    private Asset openAsset(String rel) {
        // 调试期热推：先看外部目录，没有这个文件再回退 APK 内 assets
        if (fileRoot != null) {
            File file = resolveFile(rel);
            if (file != null) {
                try {
                    return new Asset(new FileInputStream(file), file.length());
                } catch (IOException ignored) {
                    // 读不到就回退 assets
                }
            }
        }
        long length = -1;
        try {
            AssetFileDescriptor fd = assets.openFd(rel);
            try {
                length = fd.getLength();
            } finally {
                closeQuietly(fd);
            }
        } catch (IOException ignored) {
            // 压缩存储的 asset 拿不到 fd 长度，退回流式响应
        }
        try {
            return new Asset(assets.open(rel), length);
        } catch (IOException e) {
            return null;
        }
    }

    /** 把请求路径映射到外部资源目录里的文件；越界或不存在返回 null。 */
    private File resolveFile(String rel) {
        try {
            File file = new File(fileRoot, rel);
            String base = fileRoot.getCanonicalPath();
            String full = file.getCanonicalPath();
            if (!full.startsWith(base + File.separator)) {
                return null;
            }
            return file.isFile() ? file : null;
        } catch (IOException e) {
            return null;
        }
    }

    // ------------------------------------------------------------ 转发

    private void httpProxy(InputStream in, OutputStream out, Req req, Rule rule) throws IOException {
        boolean hasBody = req.isChunked() || req.contentLength() > 0;
        // 复用与重试都只对幂等方法生效：无 body 的 POST 一旦池中连接失效，重发会重复提交
        boolean retryable = !hasBody && isIdempotent(req.method);
        long startedAt = System.currentTimeMillis();
        Upstream upstream = acquireUpstream(rule, retryable);
        boolean responded = false;
        try {
            byte[] rawHead = sendRequest(in, out, req, rule, upstream, hasBody);
            Resp resp = Resp.parse(rawHead);
            if (resp == null) {
                throw new IOException("上游响应无法解析");
            }
            responded = true;
            writeResponse(out, resp, upstream);
            logHttp(req, rule, resp.status, System.currentTimeMillis() - startedAt, resp.contentLength);
            if (canReuse(resp)) {
                releaseUpstream(rule, upstream);
            } else {
                upstream.close();
            }
        } catch (IOException e) {
            upstream.close();
            if (responded || !upstream.reused || !retryable) {
                if (!responded) {
                    plain(out, 502, "Bad Gateway", "上游连接失败");
                    logHttp(req, rule, 502, System.currentTimeMillis() - startedAt, -1);
                }
                throw e;
            }
            // 池里的连接已被服务端关闭：请求还没转发给客户端，可以安全重试一次
            Log.i(TAG, "复用连接已失效，改用新连接重试: " + e.getMessage());
            if (traffic != null) {
                traffic.line("HTTP 复用连接失效，重试: " + TrafficLog.safePath(req.path)
                        + " (" + e.getMessage() + ")");
            }
            Upstream fresh = new Upstream(openUpstream(rule));
            try {
                byte[] rawHead = sendRequest(in, out, req, rule, fresh, false);
                Resp resp = Resp.parse(rawHead);
                if (resp == null) {
                    throw new IOException("上游响应无法解析");
                }
                writeResponse(out, resp, fresh);
                logHttp(req, rule, resp.status, System.currentTimeMillis() - startedAt, resp.contentLength);
            } catch (IOException retryError) {
                fresh.close();
                throw retryError;
            }
            fresh.close();
        }
    }

    /** 把一条 HTTP 转发结果写进调试日志（仅调试包）。 */
    private void logHttp(Req req, Rule rule, int status, long costMs, long bodySize) {
        if (traffic == null) {
            return;
        }
        StringBuilder sb = new StringBuilder(160);
        sb.append("HTTP ").append(req.method).append(' ')
                .append(TrafficLog.safePath(req.path))
                .append(" -> ").append(rule.host).append(':').append(rule.port)
                .append(' ').append(status).append(' ').append(costMs).append("ms");
        if (bodySize >= 0) {
            sb.append(' ').append(bodySize).append("B");
        }
        String contentType = req.header("content-type");
        if (contentType != null) {
            sb.append(" req=").append(contentType);
        }
        traffic.line(sb.toString());
    }

    /** 发请求头 +（可能的）请求体，并读回响应头。 */
    private byte[] sendRequest(InputStream in, OutputStream clientOut, Req req, Rule rule,
                               Upstream upstream, boolean hasBody) throws IOException {
        upstream.out.write(buildHead(req, rule, false).getBytes(StandardCharsets.UTF_8));
        upstream.out.flush();

        if (req.expectsContinue()) {
            clientOut.write("HTTP/1.1 100 Continue\r\n\r\n".getBytes(StandardCharsets.ISO_8859_1));
            clientOut.flush();
        }
        if (hasBody) {
            if (req.isChunked()) {
                copyChunked(in, upstream.out);
            } else {
                long length = req.contentLength();
                if (length > 0) {
                    copyExact(in, upstream.out, length);
                }
            }
            upstream.out.flush();
        }

        byte[] rawHead = readHead(upstream.in);
        if (rawHead == null) {
            throw new IOException("上游没有响应");
        }
        return rawHead;
    }

    /** 把上游响应回写给 WebView（响应侧统一 Connection: close，本地回环代价很低）。 */
    private void writeResponse(OutputStream out, Resp resp, Upstream upstream) throws IOException {
        StringBuilder head = new StringBuilder(512);
        head.append("HTTP/1.1 ").append(resp.status).append(" X\r\n");
        for (String[] header : resp.headers) {
            String lower = header[0].toLowerCase(Locale.ROOT);
            if ("connection".equals(lower) || "keep-alive".equals(lower) || "transfer-encoding".equals(lower)) {
                continue;
            }
            head.append(header[0]).append(": ").append(header[1]).append("\r\n");
        }
        if (resp.chunked) {
            head.append("Transfer-Encoding: chunked\r\n");
        }
        head.append("Connection: close\r\n\r\n");
        out.write(head.toString().getBytes(StandardCharsets.UTF_8));
        out.flush();

        if (!resp.hasBody()) {
            return;
        }
        if (resp.chunked) {
            copyChunked(upstream.in, out);
        } else if (resp.contentLength >= 0) {
            copyExact(upstream.in, out, resp.contentLength);
        } else {
            copyUntilEof(upstream.in, out);
        }
    }

    /** 响应读完且服务端没有要求关闭时，连接可以放回池里复用。 */
    private static boolean canReuse(Resp resp) {
        if (resp == null || resp.status < 200 || resp.status == 101) {
            return false;
        }
        if (!resp.http11) {
            return false;
        }
        String connection = resp.header("connection");
        if (connection != null && connection.toLowerCase(Locale.ROOT).contains("close")) {
            return false;
        }
        if (resp.chunked || resp.contentLength >= 0) {
            return true;
        }
        return !resp.hasBody();
    }

    /** 幂等方法才允许复用池中连接并在失效后重试一次。 */
    private static boolean isIdempotent(String method) {
        return "GET".equalsIgnoreCase(method)
                || "HEAD".equalsIgnoreCase(method)
                || "OPTIONS".equalsIgnoreCase(method);
    }

    /** WS 双向共享的最后收包时间：任一方向有数据都算连接活着。 */
    private static final class WsActivity {
        private volatile long lastReadAt = System.currentTimeMillis();

        void touch() {
            lastReadAt = System.currentTimeMillis();
        }

        long idleMs() {
            return System.currentTimeMillis() - lastReadAt;
        }
    }

    /** 上游连接：socket + 带缓冲的收发流，归还后可继续复用（缓冲一并保留）。 */
    private static final class Upstream {
        final Socket socket;
        final InputStream in;
        final OutputStream out;
        /** 是否从空闲池取出（用于判断失败后能否重试） */
        boolean reused;

        Upstream(Socket socket) throws IOException {
            this.socket = socket;
            this.in = new BufferedInputStream(socket.getInputStream(), BUF_SIZE);
            this.out = new BufferedOutputStream(socket.getOutputStream(), BUF_SIZE);
        }

        void close() {
            closeQuietly(socket);
        }
    }

    private Upstream acquireUpstream(Rule rule, boolean allowReuse) throws IOException {
        if (allowReuse) {
            synchronized (rule.idle) {
                while (!rule.idle.isEmpty()) {
                    Upstream candidate = rule.idle.remove(rule.idle.size() - 1);
                    if (isUsable(candidate)) {
                        candidate.reused = true;
                        return candidate;
                    }
                    candidate.close();
                }
            }
        }
        return new Upstream(openUpstream(rule));
    }

    private void releaseUpstream(Rule rule, Upstream upstream) {
        if (upstream == null) {
            return;
        }
        synchronized (rule.idle) {
            if (rule.idle.size() < MAX_IDLE_PER_RULE) {
                upstream.reused = false;
                rule.idle.add(upstream);
                return;
            }
        }
        upstream.close();
    }

    /** 池中连接是否还能用：已关闭 / 有残留字节都直接丢弃。 */
    private static boolean isUsable(Upstream upstream) {
        try {
            Socket socket = upstream.socket;
            if (socket.isClosed() || !socket.isConnected()
                    || socket.isInputShutdown() || socket.isOutputShutdown()) {
                return false;
            }
            if (upstream.in.available() > 0) {
                return false;
            }
            int oldTimeout = socket.getSoTimeout();
            try {
                socket.setSoTimeout(1);
                // 读到任何字节或 EOF 都说明连接状态不干净，直接丢弃
                upstream.in.read();
                return false;
            } catch (SocketTimeoutException nothingPending) {
                return true;
            } finally {
                socket.setSoTimeout(oldTimeout);
            }
        } catch (IOException e) {
            return false;
        }
    }

    private void wsTunnel(Socket socket, InputStream in, OutputStream out, Req req, Rule rule) throws IOException {
        Socket upstream = null;
        try {
            upstream = openUpstream(rule);
            OutputStream upOut = upstream.getOutputStream();
            final InputStream upIn = new BufferedInputStream(upstream.getInputStream(), BUF_SIZE);

            upOut.write(buildHead(req, rule, true).getBytes(StandardCharsets.UTF_8));
            upOut.flush();

            byte[] rawHead = readHead(upIn);
            if (rawHead == null) {
                plain(out, 502, "Bad Gateway", "上游握手无响应");
                return;
            }
            // 握手响应原样回写，保留 Sec-WebSocket-Accept / Connection: Upgrade
            out.write(rawHead);
            out.flush();
            Resp resp = Resp.parse(rawHead);
            if (resp == null || resp.status != 101) {
                if (traffic != null) {
                    traffic.line("WS 握手失败 " + TrafficLog.safePath(req.path)
                            + " -> " + rule.host + ":" + rule.port
                            + " status=" + (resp == null ? -1 : resp.status));
                }
                return;
            }
            if (traffic != null) {
                traffic.line("WS 握手成功 " + TrafficLog.safePath(req.path)
                        + " -> " + rule.host + ":" + rule.port);
                for (String[] header : req.headers) {
                    String lower = header[0].toLowerCase(Locale.ROOT);
                    if ("sec-websocket-protocol".equals(lower) || "origin".equals(lower)
                            || "sec-websocket-extensions".equals(lower)) {
                        traffic.line("  " + header[0] + ": "
                                + TrafficLog.safeHeader(header[0], header[1]));
                    }
                }
            }

            // WS 是长连接，单侧静音很常见（长下载 / 只说不听），因此超时只作为
            // 「双向完全静默」的死连接兜底；任一方向有字节都算连接还活着。
            final WsActivity activity = new WsActivity();
            try {
                socket.setSoTimeout(WS_IDLE_TIMEOUT);
                socket.setKeepAlive(true);
                upstream.setSoTimeout(WS_IDLE_TIMEOUT);
                upstream.setKeepAlive(true);
            } catch (Exception ignored) {
                // 部分设备不允许修改 keepalive，忽略
            }

            // 上行用独立线程搬运；TLS 流的读写可以分别在不同线程进行
            final OutputStream upOutRef = upOut;
            final Socket upstreamRef = upstream;
            final Socket clientRef = socket;
            Thread pump = new Thread(new Runnable() {
                @Override
                public void run() {
                    try {
                        pumpWsFrames(in, upOutRef, true, activity);
                    } catch (SocketTimeoutException e) {
                        Log.w(TAG, "ws 上行空闲超时，回收隧道: " + e.getMessage());
                        // 判定死连接：关掉两侧，让下行 read 立即退出
                        closeQuietly(upstreamRef);
                        closeQuietly(clientRef);
                    } catch (Exception e) {
                        // 连接关闭属于正常结束
                        Log.i(TAG, "ws 上行结束: " + e.getMessage());
                    } finally {
                        try {
                            upOutRef.flush();
                        } catch (IOException ignored) {
                        }
                        // 浏览器断开时把 FIN 传给上游，让服务端及时收尾
                        try {
                            upstreamRef.shutdownOutput();
                        } catch (IOException ignored) {
                        }
                    }
                }
            }, "jianling-ws-up");
            pump.setDaemon(true);
            pump.start();

            pumpWsFrames(upIn, out, false, activity);
        } catch (SocketTimeoutException e) {
            Log.w(TAG, "ws 隧道空闲超时: " + e.getMessage());
        } catch (IOException e) {
            Log.w(TAG, "ws 隧道异常: " + e.getMessage());
            throw e;
        } finally {
            closeQuietly(upstream);
            closeQuietly(socket);
        }
    }

    private String buildHead(Req req, Rule rule, boolean keepUpgrade) {
        boolean defaultPort = (rule.tls && rule.port == 443) || (!rule.tls && rule.port == 80);
        String hostHeader = defaultPort ? rule.host : rule.host + ":" + rule.port;

        StringBuilder head = new StringBuilder(512);
        head.append(req.method).append(' ').append(rule.basePath).append(req.path).append(" HTTP/1.1\r\n");
        head.append("Host: ").append(hostHeader).append("\r\n");
        for (String[] header : req.headers) {
            String lower = header[0].toLowerCase(Locale.ROOT);
            if ("host".equals(lower) || "proxy-connection".equals(lower)
                    || "keep-alive".equals(lower) || "expect".equals(lower)) {
                continue;
            }
            // 调试包去掉压缩协商：帧内容保持明文，日志/pcm 才能直接看出音频与 JSON
            if (keepUpgrade && traffic != null && "sec-websocket-extensions".equals(lower)) {
                continue;
            }
            if ("connection".equals(lower)) {
                if (keepUpgrade) {
                    head.append(header[0]).append(": ").append(header[1]).append("\r\n");
                }
                continue;
            }
            if ("origin".equals(lower)) {
                if (rule.rewriteOrigin) {
                    head.append("Origin: ").append(rule.origin).append("\r\n");
                } else {
                    head.append(header[0]).append(": ").append(header[1]).append("\r\n");
                }
                continue;
            }
            head.append(header[0]).append(": ").append(header[1]).append("\r\n");
        }
        for (String[] extra : rule.extraHeaders) {
            String lower = extra[0].toLowerCase(Locale.ROOT);
            if ("host".equals(lower) || "content-length".equals(lower)
                    || "transfer-encoding".equals(lower) || "connection".equals(lower)) {
                continue;
            }
            head.append(extra[0]).append(": ").append(extra[1]).append("\r\n");
        }
        if (!keepUpgrade) {
            head.append("Connection: keep-alive\r\n");
        }
        head.append("\r\n");
        return head.toString();
    }

    private Rule matchRule(String path) {
        for (Rule rule : rules) {
            if (path.startsWith(rule.prefix)) {
                return rule;
            }
        }
        return null;
    }

    private Socket openUpstream(Rule rule) throws IOException {
        // resolveIp 非空时按固定 IP 建连（域名只在 hosts 里能解析时用），TLS 的 SNI 仍用域名
        String dial = rule.resolveIp.length() > 0 ? rule.resolveIp : rule.host;
        if (!rule.tls) {
            Socket socket = new Socket();
            socket.connect(new InetSocketAddress(dial, rule.port), 15000);
            socket.setSoTimeout(SOCKET_TIMEOUT);
            socket.setTcpNoDelay(true);
            return socket;
        }
        SSLSocketFactory factory;
        if (rule.insecure) {
            try {
                factory = getInsecureFactory();
            } catch (Exception e) {
                throw new IOException("初始化 TLS 失败: " + e.getMessage());
            }
        } else {
            factory = (SSLSocketFactory) SSLSocketFactory.getDefault();
        }
        SSLSocket socket = (SSLSocket) factory.createSocket();
        socket.connect(new InetSocketAddress(dial, rule.port), 15000);
        socket.setSoTimeout(SOCKET_TIMEOUT);
        socket.setTcpNoDelay(true);
        SSLParameters params = socket.getSSLParameters();
        boolean fixedIp = rule.resolveIp.length() > 0;
        if (fixedIp) {
            // 连的是 IP，但握手要按域名发 SNI，否则拿不到对应站点的证书
            try {
                List<SNIServerName> names = new ArrayList<SNIServerName>();
                names.add(new SNIHostName(rule.host));
                params.setServerNames(names);
            } catch (IllegalArgumentException ignored) {
                // 纯 IP 形式的 host 不支持 SNI，忽略
            }
        }
        if (!rule.insecure && !fixedIp) {
            params.setEndpointIdentificationAlgorithm("HTTPS");
        }
        socket.setSSLParameters(params);
        socket.startHandshake();
        if (!rule.insecure && fixedIp) {
            // 固定 IP 时套接字的对端是 IP，系统的主机名校验必然失败，改按域名手工校验
            boolean ok = HttpsURLConnection.getDefaultHostnameVerifier()
                    .verify(rule.host, socket.getSession());
            if (!ok) {
                closeQuietly(socket);
                throw new IOException("证书与域名 " + rule.host + " 不匹配");
            }
        }
        return socket;
    }

    private static SSLSocketFactory getInsecureFactory() throws Exception {
        SSLSocketFactory factory = insecureFactory;
        if (factory != null) {
            return factory;
        }
        synchronized (LocalServer.class) {
            if (insecureFactory == null) {
                TrustManager[] trustAll = new TrustManager[]{new X509TrustManager() {
                    @Override
                    public void checkClientTrusted(X509Certificate[] chain, String authType) {
                    }

                    @Override
                    public void checkServerTrusted(X509Certificate[] chain, String authType) {
                    }

                    @Override
                    public X509Certificate[] getAcceptedIssuers() {
                        return new X509Certificate[0];
                    }
                }};
                SSLContext context = SSLContext.getInstance("TLS");
                context.init(null, trustAll, new SecureRandom());
                insecureFactory = context.getSocketFactory();
            }
            return insecureFactory;
        }
    }

    // ------------------------------------------------------------ 协议工具

    private static final class Rule {
        String prefix;
        boolean rewriteOrigin;
        boolean insecure;
        String host;
        int port;
        boolean tls;
        String basePath;
        String origin;
        /** 可选：固定解析 IP，空串表示按域名建连 */
        String resolveIp = "";
        /** 追加到上游请求的 header（来自 rules 里的 headers JSON，解析失败则为空） */
        final List<String[]> extraHeaders = new ArrayList<String[]>();
        /** 空闲上游连接池（访问时 synchronized (idle)） */
        final List<Upstream> idle = new ArrayList<Upstream>();
    }

    private static Rule parseRule(JSONObject item) {
        String prefix = item.optString("prefix", "").trim();
        String target = item.optString("target", "").trim();
        if (prefix.length() == 0 || target.length() == 0) {
            return null;
        }
        int schemeEnd = target.indexOf("://");
        if (schemeEnd <= 0) {
            return null;
        }
        String scheme = target.substring(0, schemeEnd).toLowerCase(Locale.ROOT);
        String rest = target.substring(schemeEnd + 3);
        int slash = rest.indexOf('/');
        String authority = slash >= 0 ? rest.substring(0, slash) : rest;
        String path = slash >= 0 ? rest.substring(slash) : "";
        if (authority.length() == 0) {
            return null;
        }
        boolean tls = "https".equals(scheme) || "wss".equals(scheme);
        int defaultPort = tls ? 443 : 80;
        String host = authority;
        int port = defaultPort;
        int colon = authority.lastIndexOf(':');
        if (colon > 0 && colon < authority.length() - 1) {
            String portText = authority.substring(colon + 1);
            boolean digits = true;
            for (int i = 0; i < portText.length(); i++) {
                if (!Character.isDigit(portText.charAt(i))) {
                    digits = false;
                    break;
                }
            }
            if (digits) {
                host = authority.substring(0, colon);
                port = Integer.parseInt(portText);
            }
        }
        Rule rule = new Rule();
        rule.prefix = prefix.endsWith("/") ? prefix.substring(0, prefix.length() - 1) : prefix;
        rule.host = host;
        rule.port = port;
        rule.tls = tls;
        rule.basePath = path.endsWith("/") ? path.substring(0, path.length() - 1) : path;
        rule.origin = (tls ? "https://" : "http://") + host + (port == defaultPort ? "" : ":" + port);
        rule.rewriteOrigin = item.optBoolean("rewriteOrigin", true);
        rule.insecure = item.optBoolean("insecure", false);
        rule.resolveIp = item.optString("resolveIp", "").trim();
        parseExtraHeaders(item.optString("headers", "").trim(), rule.extraHeaders);
        return rule;
    }

    /** 解析 headers JSON（对象，值为字符串），逐个塞进 extraHeaders；非法内容只打日志，不影响转发。 */
    private static void parseExtraHeaders(String text, List<String[]> out) {
        if (text.length() == 0) {
            return;
        }
        try {
            JSONObject obj = new JSONObject(text);
            Iterator<String> keys = obj.keys();
            while (keys.hasNext()) {
                String name = keys.next().trim();
                if (name.length() == 0 || obj.isNull(name)) {
                    continue;
                }
                out.add(new String[]{name, String.valueOf(obj.get(name))});
            }
        } catch (Exception e) {
            Log.w(TAG, "转发规则 headers JSON 解析失败，已忽略: " + e.getMessage());
        }
    }

    private static final class Req {
        String method = "";
        String path = "/";
        final List<String[]> headers = new ArrayList<String[]>();

        static Req parse(byte[] raw) {
            if (raw == null || raw.length == 0) {
                return null;
            }
            String text = new String(raw, StandardCharsets.UTF_8);
            String[] lines = text.split("\r?\n");
            if (lines.length == 0) {
                return null;
            }
            String[] first = lines[0].split(" ");
            if (first.length < 2) {
                return null;
            }
            Req req = new Req();
            req.method = first[0];
            req.path = first[1];
            for (int i = 1; i < lines.length; i++) {
                String line = lines[i];
                if (line.length() == 0) {
                    continue;
                }
                int idx = line.indexOf(':');
                if (idx <= 0) {
                    continue;
                }
                req.headers.add(new String[]{
                        line.substring(0, idx).trim(),
                        line.substring(idx + 1).trim()
                });
            }
            return req;
        }

        String header(String name) {
            for (String[] header : headers) {
                if (header[0].equalsIgnoreCase(name)) {
                    return header[1];
                }
            }
            return null;
        }

        String pathOnly() {
            int idx = path.indexOf('?');
            return idx >= 0 ? path.substring(0, idx) : path;
        }

        boolean isUpgrade() {
            String value = header("upgrade");
            return value != null && value.toLowerCase(Locale.ROOT).contains("websocket");
        }

        boolean isChunked() {
            String value = header("transfer-encoding");
            return value != null && value.toLowerCase(Locale.ROOT).contains("chunked");
        }

        boolean expectsContinue() {
            String value = header("expect");
            return value != null && value.toLowerCase(Locale.ROOT).contains("100-continue");
        }

        long contentLength() {
            String value = header("content-length");
            if (value == null) {
                return -1;
            }
            try {
                return Long.parseLong(value.trim());
            } catch (NumberFormatException e) {
                return -1;
            }
        }
    }

    private static final class Resp {
        int status;
        boolean chunked;
        /** 状态行是否为 HTTP/1.1（连接复用的前提） */
        boolean http11;
        long contentLength = -1;
        final List<String[]> headers = new ArrayList<String[]>();

        static Resp parse(byte[] raw) {
            if (raw == null || raw.length == 0) {
                return null;
            }
            String text = new String(raw, StandardCharsets.ISO_8859_1);
            String[] lines = text.split("\r?\n");
            if (lines.length == 0) {
                return null;
            }
            String[] first = lines[0].split(" ");
            if (first.length < 2) {
                return null;
            }
            Resp resp = new Resp();
            resp.http11 = "HTTP/1.1".equalsIgnoreCase(first[0].trim());
            try {
                resp.status = Integer.parseInt(first[1].trim());
            } catch (NumberFormatException e) {
                return null;
            }
            for (int i = 1; i < lines.length; i++) {
                String line = lines[i];
                if (line.length() == 0) {
                    continue;
                }
                int idx = line.indexOf(':');
                if (idx <= 0) {
                    continue;
                }
                String name = line.substring(0, idx).trim();
                String value = line.substring(idx + 1).trim();
                resp.headers.add(new String[]{name, value});
                String lower = name.toLowerCase(Locale.ROOT);
                if ("transfer-encoding".equals(lower) && value.toLowerCase(Locale.ROOT).contains("chunked")) {
                    resp.chunked = true;
                } else if ("content-length".equals(lower)) {
                    try {
                        resp.contentLength = Long.parseLong(value);
                    } catch (NumberFormatException ignored) {
                    }
                }
            }
            return resp;
        }

        boolean hasBody() {
            return !(status < 200 || status == 204 || status == 304);
        }

        String header(String name) {
            for (String[] header : headers) {
                if (header[0].equalsIgnoreCase(name)) {
                    return header[1];
                }
            }
            return null;
        }
    }

    private static byte[] readHead(InputStream in) throws IOException {
        ByteArrayOutputStream buffer = new ByteArrayOutputStream(2048);
        int state = 0;
        int b;
        while ((b = in.read()) != -1) {
            buffer.write(b);
            switch (state) {
                case 0:
                    state = (b == '\r') ? 1 : 0;
                    break;
                case 1:
                    state = (b == '\n') ? 2 : ((b == '\r') ? 1 : 0);
                    break;
                case 2:
                    state = (b == '\r') ? 3 : 0;
                    break;
                case 3:
                    state = (b == '\n') ? 4 : ((b == '\r') ? 1 : 0);
                    break;
                default:
                    break;
            }
            if (state == 4) {
                break;
            }
            if (buffer.size() > HEAD_LIMIT) {
                break;
            }
        }
        if (buffer.size() == 0) {
            return null;
        }
        return buffer.toByteArray();
    }

    private static String readLine(InputStream in) throws IOException {
        ByteArrayOutputStream buffer = new ByteArrayOutputStream(64);
        int b;
        while ((b = in.read()) != -1) {
            buffer.write(b);
            if (b == '\n') {
                break;
            }
        }
        if (buffer.size() == 0) {
            return null;
        }
        return new String(buffer.toByteArray(), StandardCharsets.ISO_8859_1);
    }

    private static void copyExact(InputStream in, OutputStream out, long length) throws IOException {
        byte[] buffer = new byte[BUF_SIZE];
        long left = length;
        while (left > 0) {
            int want = (int) Math.min(buffer.length, left);
            int read = in.read(buffer, 0, want);
            if (read < 0) {
                break;
            }
            out.write(buffer, 0, read);
            out.flush();
            left -= read;
        }
    }

    private static void copyUntilEof(InputStream in, OutputStream out) throws IOException {
        byte[] buffer = new byte[BUF_SIZE];
        int read;
        while ((read = in.read(buffer)) > 0) {
            out.write(buffer, 0, read);
            out.flush();
        }
    }

    /**
     * 原样转发 WebSocket 帧，同时把帧元数据（以及客户端上行音频）写进调试日志。
     *
     * <p>只解析帧头读长度，payload 逐字节透传（不解掩码、不解压缩），
     * 因此对 permessage-deflate 等扩展仍然透明；只有调试包才真正写文件。
     */
    private void pumpWsFrames(InputStream in, OutputStream out, boolean uplink,
                              WsActivity activity) throws IOException {
        byte[] buffer = new byte[BUF_SIZE];
        // 分片消息：首片带 opcode，续片为 0，这里记住整条消息的类型
        int messageOpcode = -1;
        // 文本消息常被分片，累积到整条消息结束再落日志
        ByteArrayOutputStream pendingText = null;
        boolean textAbandoned = false;
        while (true) {
            int b0;
            try {
                b0 = in.read();
            } catch (SocketTimeoutException e) {
                // 停在帧边界：另一方向仍有流量说明只是本侧静音，继续等新帧；
                // 双向都静默超过上限才判定死连接（抛给上层回收隧道）。
                if (activity.idleMs() >= WS_IDLE_TIMEOUT) {
                    throw e;
                }
                continue;
            }
            if (b0 < 0) {
                return;
            }
            activity.touch();
            int b1 = in.read();
            if (b1 < 0) {
                throw new IOException("ws 帧头不完整");
            }
            activity.touch();
            boolean fin = (b0 & 0x80) != 0;
            int opcode = b0 & 0x0F;
            if (opcode != 0) {
                messageOpcode = opcode;
                pendingText = null;
                textAbandoned = false;
            }
            boolean audioFrame = messageOpcode == 2;
            boolean masked = (b1 & 0x80) != 0;
            long length = b1 & 0x7FL;

            ByteArrayOutputStream frameHead = new ByteArrayOutputStream(14);
            frameHead.write(b0);
            frameHead.write(b1);
            if (length == 126L) {
                int hi = in.read();
                int lo = in.read();
                if (hi < 0 || lo < 0) {
                    throw new IOException("ws 帧头不完整");
                }
                frameHead.write(hi);
                frameHead.write(lo);
                length = ((long) hi << 8) | (long) lo;
            } else if (length == 127L) {
                length = 0;
                for (int i = 0; i < 8; i++) {
                    int v = in.read();
                    if (v < 0) {
                        throw new IOException("ws 帧头不完整");
                    }
                    frameHead.write(v);
                    length = (length << 8) | (long) v;
                }
            }
            if (masked) {
                for (int i = 0; i < 4; i++) {
                    int v = in.read();
                    if (v < 0) {
                        throw new IOException("ws 帧头不完整");
                    }
                    frameHead.write(v);
                }
            }
            out.write(frameHead.toByteArray());
            out.flush();

            // 超过 64MB 的帧只透传、不记录，避免把内存/日志撑爆
            TrafficLog log = traffic;
            if (log != null) {
                log.line("WS " + (uplink ? "C>S" : "S>C")
                        + " op=" + opcode + " fin=" + (fin ? 1 : 0)
                        + " len=" + length + (masked ? " masked" : ""));
            }
            boolean recordPayload = log != null && length <= 64L * 1024 * 1024;
            if (recordPayload && messageOpcode == 1 && !textAbandoned) {
                if (pendingText == null) {
                    pendingText = new ByteArrayOutputStream();
                }
                if (pendingText.size() + length > 131072L) {
                    // 单条文本消息超过 128KB 就放弃记录内容，避免日志膨胀
                    pendingText = null;
                    textAbandoned = true;
                }
            }
            byte[] firstBytes = recordPayload && audioFrame && opcode != 0
                    ? new byte[(int) Math.min(16L, length)] : null;
            int firstFilled = 0;
            long remaining = length;
            while (remaining > 0) {
                int want = (int) Math.min((long) buffer.length, remaining);
                int got = in.read(buffer, 0, want);
                if (got < 0) {
                    throw new IOException("ws 帧负载不完整");
                }
                activity.touch();
                if (firstBytes != null && firstFilled < firstBytes.length) {
                    int copy = Math.min(got, firstBytes.length - firstFilled);
                    System.arraycopy(buffer, 0, firstBytes, firstFilled, copy);
                    firstFilled += copy;
                }
                if (pendingText != null) {
                    pendingText.write(buffer, 0, got);
                }
                out.write(buffer, 0, got);
                // 音频原始字节按方向留档，便于离线分析录音与播放两侧
                if (recordPayload && audioFrame) {
                    log.audio(uplink, buffer, 0, got);
                }
                remaining -= got;
            }
            out.flush();
            if (log != null) {
                if (firstBytes != null && firstFilled > 0) {
                    log.line("  head=" + TrafficLog.hex(firstBytes, firstFilled));
                }
                if (fin) {
                    if (pendingText != null) {
                        log.line("  text=" + TrafficLog.escape(pendingText.toString("UTF-8")));
                        pendingText = null;
                    } else if (textAbandoned) {
                        log.line("  text=<skipped 超出记录上限>");
                    }
                }
            }
        }
    }

    private static void copyChunked(InputStream in, OutputStream out) throws IOException {
        while (true) {
            String line = readLine(in);
            if (line == null) {
                return;
            }
            out.write(line.getBytes(StandardCharsets.ISO_8859_1));
            String sizeText = line.trim();
            int semi = sizeText.indexOf(';');
            if (semi >= 0) {
                sizeText = sizeText.substring(0, semi);
            }
            long size;
            try {
                size = Long.parseLong(sizeText.trim(), 16);
            } catch (NumberFormatException e) {
                size = 0;
            }
            if (size == 0) {
                while (true) {
                    String trailer = readLine(in);
                    if (trailer == null) {
                        return;
                    }
                    out.write(trailer.getBytes(StandardCharsets.ISO_8859_1));
                    if ("\r\n".equals(trailer) || "\n".equals(trailer)) {
                        break;
                    }
                }
                out.flush();
                return;
            }
            copyExact(in, out, size + 2);
        }
    }

    private static void writeChunked(InputStream in, OutputStream out) throws IOException {
        byte[] buffer = new byte[BUF_SIZE];
        int read;
        while ((read = in.read(buffer)) > 0) {
            out.write(Integer.toHexString(read).getBytes(StandardCharsets.ISO_8859_1));
            out.write(CRLF);
            out.write(buffer, 0, read);
            out.write(CRLF);
            out.flush();
        }
        out.write("0\r\n\r\n".getBytes(StandardCharsets.ISO_8859_1));
        out.flush();
    }

    private static void skipFully(InputStream in, long count) throws IOException {
        long left = count;
        while (left > 0) {
            long skipped = in.skip(left);
            if (skipped <= 0) {
                if (in.read() < 0) {
                    return;
                }
                skipped = 1;
            }
            left -= skipped;
        }
    }

    private static long[] parseRange(String value, long total) {
        if (value == null || total <= 0) {
            return null;
        }
        String text = value.trim();
        if (!text.startsWith("bytes=")) {
            return null;
        }
        text = text.substring(6);
        int comma = text.indexOf(',');
        if (comma >= 0) {
            text = text.substring(0, comma);
        }
        text = text.trim();
        int dash = text.indexOf('-');
        if (dash < 0) {
            return null;
        }
        String startText = text.substring(0, dash).trim();
        String endText = text.substring(dash + 1).trim();
        long start;
        long end;
        try {
            if (startText.length() == 0) {
                long suffix = Long.parseLong(endText);
                if (suffix <= 0) {
                    return null;
                }
                start = Math.max(0, total - suffix);
                end = total - 1;
            } else {
                start = Long.parseLong(startText);
                end = endText.length() == 0 ? total - 1 : Long.parseLong(endText);
                if (end > total - 1) {
                    end = total - 1;
                }
                if (start > end || start >= total) {
                    return null;
                }
            }
        } catch (NumberFormatException e) {
            return null;
        }
        return new long[]{start, end};
    }

    private static void plain(OutputStream out, int status, String title, String detail) {
        String body = "<!doctype html><meta charset=\"utf-8\">"
                + "<body style=\"font:14px/1.7 system-ui;padding:24px\"><h3>" + title + "</h3><p>"
                + detail + "</p></body>";
        byte[] bytes = body.getBytes(StandardCharsets.UTF_8);
        String head = "HTTP/1.1 " + status + " " + title + "\r\n"
                + "Content-Type: text/html; charset=utf-8\r\n"
                + "Content-Length: " + bytes.length + "\r\n"
                + "Connection: close\r\n\r\n";
        try {
            out.write(head.getBytes(StandardCharsets.UTF_8));
            out.write(bytes);
            out.flush();
        } catch (IOException ignored) {
            // 连接已断开
        }
    }

    private static String mimeOf(String path) {
        int dot = path.lastIndexOf('.');
        String ext = dot >= 0 ? path.substring(dot + 1).toLowerCase(Locale.ROOT) : "";
        if ("html".equals(ext) || "htm".equals(ext)) {
            return "text/html; charset=utf-8";
        }
        if ("js".equals(ext) || "mjs".equals(ext)) {
            return "text/javascript; charset=utf-8";
        }
        if ("css".equals(ext)) {
            return "text/css; charset=utf-8";
        }
        if ("json".equals(ext) || "map".equals(ext)) {
            return "application/json; charset=utf-8";
        }
        if ("txt".equals(ext)) {
            return "text/plain; charset=utf-8";
        }
        if ("xml".equals(ext)) {
            return "application/xml; charset=utf-8";
        }
        if ("svg".equals(ext)) {
            return "image/svg+xml";
        }
        if ("png".equals(ext)) {
            return "image/png";
        }
        if ("jpg".equals(ext) || "jpeg".equals(ext)) {
            return "image/jpeg";
        }
        if ("gif".equals(ext)) {
            return "image/gif";
        }
        if ("webp".equals(ext)) {
            return "image/webp";
        }
        if ("avif".equals(ext)) {
            return "image/avif";
        }
        if ("ico".equals(ext)) {
            return "image/x-icon";
        }
        if ("bmp".equals(ext)) {
            return "image/bmp";
        }
        if ("woff".equals(ext)) {
            return "font/woff";
        }
        if ("woff2".equals(ext)) {
            return "font/woff2";
        }
        if ("ttf".equals(ext)) {
            return "font/ttf";
        }
        if ("otf".equals(ext)) {
            return "font/otf";
        }
        if ("eot".equals(ext)) {
            return "application/vnd.ms-fontobject";
        }
        if ("mp3".equals(ext)) {
            return "audio/mpeg";
        }
        if ("wav".equals(ext)) {
            return "audio/wav";
        }
        if ("ogg".equals(ext)) {
            return "audio/ogg";
        }
        if ("m4a".equals(ext)) {
            return "audio/mp4";
        }
        if ("mp4".equals(ext)) {
            return "video/mp4";
        }
        if ("webm".equals(ext)) {
            return "video/webm";
        }
        if ("wasm".equals(ext)) {
            return "application/wasm";
        }
        if ("pdf".equals(ext)) {
            return "application/pdf";
        }
        if ("zip".equals(ext)) {
            return "application/zip";
        }
        return "application/octet-stream";
    }

    private JSONObject readConfig() throws Exception {
        InputStream in = assets.open(CONFIG_ASSET);
        try {
            ByteArrayOutputStream buffer = new ByteArrayOutputStream(1024);
            byte[] chunk = new byte[4096];
            int read;
            while ((read = in.read(chunk)) > 0) {
                buffer.write(chunk, 0, read);
            }
            return new JSONObject(new String(buffer.toByteArray(), StandardCharsets.UTF_8));
        } finally {
            closeQuietly(in);
        }
    }

    private static void closeQuietly(Closeable target) {
        if (target == null) {
            return;
        }
        try {
            target.close();
        } catch (IOException ignored) {
            // 忽略
        }
    }

    // ------------------------------------------------------------ 调试流量日志

    /**
     * 只在构建时勾选了「调试流量日志」的调试包里，把转发流量落到应用外部私有目录，
     * 方便 adb pull 后离线排查：
     *
     * <pre>
     * /sdcard/Android/data/&lt;包名&gt;/files/jianling-logs/traffic-yyyyMMdd-HHmmss-SSS.log
     * /sdcard/Android/data/&lt;包名&gt;/files/jianling-logs/traffic-yyyyMMdd-HHmmss-SSS.uplink.pcm
     * </pre>
     *
     * 正式包恒不创建任何文件；写日志失败也绝不影响转发。
     */
    private static final class TrafficLog {
        /** 音频留档上限（每个方向），超过后停止写入 */
        private static final long PCM_LIMIT = 200L * 1024 * 1024;
        private final File dir;
        private final File logFile;
        private final File uplinkFile;
        private final File downlinkFile;
        private final SimpleDateFormat timeFormat = new SimpleDateFormat("HH:mm:ss.SSS", Locale.US);
        private Writer writer;
        private FileOutputStream uplinkOut;
        private FileOutputStream downlinkOut;
        private long uplinkBytes;
        private long downlinkBytes;

        private TrafficLog(File dir, File logFile, File uplinkFile, File downlinkFile) {
            this.dir = dir;
            this.logFile = logFile;
            this.uplinkFile = uplinkFile;
            this.downlinkFile = downlinkFile;
        }

        static TrafficLog create(Context context, boolean debugLog) {
            // 只认构建时写入的 debugLog 开关（正式包恒为 false），不按 debuggable 隐式开启
            if (!debugLog) {
                return null;
            }
            try {
                File base = context.getExternalFilesDir(null);
                if (base == null) {
                    base = context.getFilesDir();
                }
                File dir = new File(base, "jianling-logs");
                if (!dir.isDirectory() && !dir.mkdirs()) {
                    Log.w(TAG, "流量日志目录创建失败: " + dir.getAbsolutePath());
                    return null;
                }
                String stamp = new SimpleDateFormat("yyyyMMdd-HHmmss-SSS", Locale.US).format(new Date());
                TrafficLog log = new TrafficLog(dir, new File(dir, "traffic-" + stamp + ".log"),
                        new File(dir, "traffic-" + stamp + ".uplink.pcm"),
                        new File(dir, "traffic-" + stamp + ".downlink.pcm"));
                log.writer = new OutputStreamWriter(new FileOutputStream(log.logFile, true), "UTF-8");
                log.line("会话开始 " + log.logFile.getAbsolutePath());
                Log.i(TAG, "流量日志: " + log.logFile.getAbsolutePath());
                return log;
            } catch (Exception e) {
                Log.w(TAG, "流量日志初始化失败: " + e.getMessage());
                return null;
            }
        }

        synchronized void line(String text) {
            if (writer == null) {
                return;
            }
            try {
                writer.write(timeFormat.format(new Date()));
                writer.write(' ');
                writer.write(text);
                writer.write("\r\n");
                writer.flush();
            } catch (IOException e) {
                closeQuietly(writer);
                writer = null;
            }
        }

        /** 按方向把音频原始字节追加到 .uplink.pcm / .downlink.pcm */
        synchronized void audio(boolean uplink, byte[] data, int offset, int length) {
            if (length <= 0) {
                return;
            }
            try {
                if (uplink) {
                    if (uplinkBytes >= PCM_LIMIT) {
                        return;
                    }
                    if (uplinkOut == null) {
                        uplinkOut = new FileOutputStream(uplinkFile, true);
                    }
                    uplinkOut.write(data, offset, length);
                    uplinkBytes += length;
                } else {
                    if (downlinkBytes >= PCM_LIMIT) {
                        return;
                    }
                    if (downlinkOut == null) {
                        downlinkOut = new FileOutputStream(downlinkFile, true);
                    }
                    downlinkOut.write(data, offset, length);
                    downlinkBytes += length;
                }
            } catch (IOException e) {
                if (uplink) {
                    uplinkBytes = PCM_LIMIT;
                    closeQuietly(uplinkOut);
                    uplinkOut = null;
                } else {
                    downlinkBytes = PCM_LIMIT;
                    closeQuietly(downlinkOut);
                    downlinkOut = null;
                }
            }
        }

        synchronized void close() {
            if (writer != null) {
                try {
                    writer.flush();
                } catch (IOException ignored) {
                    // 忽略
                }
                closeQuietly(writer);
                writer = null;
            }
            closeQuietly(uplinkOut);
            uplinkOut = null;
            closeQuietly(downlinkOut);
            downlinkOut = null;
        }

        /** 头部字段脱敏：凭证类只保留前缀/长度 */
        static String safeHeader(String name, String value) {
            if (value == null) {
                return "";
            }
            String lower = name.toLowerCase(Locale.ROOT);
            if (lower.contains("protocol")) {
                // 如 auth-token.<jwt>：只留协议名，token 部分丢掉
                int dot = value.indexOf('.');
                return dot > 0 ? value.substring(0, dot) + ".<hidden>" : "<hidden len=" + value.length() + ">";
            }
            if (lower.contains("authorization") || lower.contains("cookie")
                    || lower.contains("token") || lower.contains("key")
                    || lower.contains("secret")) {
                return "<hidden len=" + value.length() + ">";
            }
            return value;
        }

        /** URL 里的 token/ticket 等参数脱敏 */
        static String safePath(String path) {
            if (path == null) {
                return "/";
            }
            int queryAt = path.indexOf('?');
            if (queryAt < 0) {
                return path;
            }
            StringBuilder sb = new StringBuilder(path.substring(0, queryAt + 1));
            String[] parts = path.substring(queryAt + 1).split("&");
            for (int i = 0; i < parts.length; i++) {
                if (i > 0) {
                    sb.append('&');
                }
                String part = parts[i];
                int eq = part.indexOf('=');
                String key = (eq >= 0 ? part.substring(0, eq) : part).toLowerCase(Locale.ROOT);
                if (key.contains("token") || key.contains("ticket")
                        || key.contains("key") || key.contains("secret")) {
                    sb.append(part, 0, eq >= 0 ? eq : part.length()).append("=***");
                } else {
                    sb.append(part);
                }
            }
            return sb.toString();
        }

        static String hex(byte[] data, int length) {
            StringBuilder sb = new StringBuilder(length * 3);
            for (int i = 0; i < length && i < data.length; i++) {
                if (sb.length() > 0) {
                    sb.append(' ');
                }
                int v = data[i] & 0xFF;
                if (v < 16) {
                    sb.append('0');
                }
                sb.append(Integer.toHexString(v));
            }
            return sb.toString();
        }

        static String escape(String text) {
            if (text == null) {
                return "";
            }
            return text.replace("\r", "\\r").replace("\n", "\\n");
        }
    }
}
