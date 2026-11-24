package __APPLICATION_ID__;

import android.content.Context;
import android.content.res.AssetManager;
import android.os.Handler;
import android.os.Looper;
import android.util.Base64;
import android.util.Log;
import android.webkit.WebView;

import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.util.Locale;

/**
 * 启动屏：先展示一个静态 HTML（整屏图片，或 logo + 名称），
 * 服务就绪并到达最短展示时长后再切到真正的页面。
 *
 * <p>配置来自 assets/jianling-splash.json，图片同样打进 assets 并以 data URL 内联，
 * 因此不依赖本地服务，也不受 file:// 与跨域限制。
 */
final class Splash {
    private static final String TAG = "JianlingSplash";
    private static final String CONFIG_ASSET = "jianling-splash.json";

    private Splash() {
    }

    /** 展示启动屏并安排跳转；返回 false 表示不可用，调用方应直接加载目标页。 */
    static boolean loadInto(Context context, WebView webView, String targetUrl) {
        try {
            JSONObject config = readConfig(context.getAssets());
            if (config == null || !config.optBoolean("enabled", false)) {
                return false;
            }
            String html = buildHtml(context.getAssets(), config);
            webView.loadDataWithBaseURL(null, html, "text/html", "utf-8", null);

            int delay = Math.max(200, config.optInt("minShowMs", 600));
            final WebView target = webView;
            final String url = targetUrl;
            new Handler(Looper.getMainLooper()).postDelayed(new Runnable() {
                @Override
                public void run() {
                    try {
                        target.loadUrl(url);
                    } catch (Exception e) {
                        Log.w(TAG, "切换到主页面失败: " + e.getMessage());
                    }
                }
            }, delay);
            return true;
        } catch (Exception e) {
            Log.w(TAG, "启动屏准备失败，直接加载页面: " + e.getMessage());
            return false;
        }
    }

    private static JSONObject readConfig(AssetManager assets) {
        InputStream in = null;
        try {
            in = assets.open(CONFIG_ASSET);
            ByteArrayOutputStream buffer = new ByteArrayOutputStream(1024);
            byte[] chunk = new byte[4096];
            int read;
            while ((read = in.read(chunk)) > 0) {
                buffer.write(chunk, 0, read);
            }
            return new JSONObject(new String(buffer.toByteArray(), "UTF-8"));
        } catch (IOException e) {
            return null;
        } catch (Exception e) {
            Log.w(TAG, "读取启动屏配置失败: " + e.getMessage());
            return null;
        } finally {
            if (in != null) {
                try {
                    in.close();
                } catch (IOException ignored) {
                    // 忽略
                }
            }
        }
    }

    private static String buildHtml(AssetManager assets, JSONObject config) {
        String background = config.optString("background", "#FFFFFF").trim();
        if (background.length() == 0) {
            background = "#FFFFFF";
        }
        String foreground = colorIsDark(background) ? "#FFFFFF" : "#1F2430";
        String title = config.optString("title", "");

        String full = dataUrl(assets, config.optString("image", ""));
        StringBuilder content = new StringBuilder();
        if (full != null) {
            content.append("<img class=\"full\" src=\"").append(full).append("\" alt=\"\">");
        } else {
            String logo = dataUrl(assets, config.optString("logo", ""));
            content.append("<div class=\"center\">");
            if (logo != null) {
                content.append("<img class=\"logo\" src=\"").append(logo).append("\" alt=\"\">");
            }
            content.append("<div class=\"name\">").append(htmlEscape(title)).append("</div>");
            content.append("</div>");
        }

        // 默认模式：背景图优先于背景色
        String backgroundImage = full == null
                ? dataUrl(assets, config.optString("backgroundImage", ""))
                : null;
        String bodyCss = backgroundImage != null
                ? "background:#FFFFFF url('" + backgroundImage + "') center/cover no-repeat"
                : "background:" + background;

        return "<!doctype html><html><head><meta charset=\"utf-8\">"
                + "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1,maximum-scale=1,user-scalable=no\">"
                + "<style>"
                + "html,body{margin:0;padding:0;height:100%;overflow:hidden;" + bodyCss + "}"
                + "img.full{position:fixed;inset:0;width:100%;height:100%;object-fit:cover;display:block}"
                + ".center{position:absolute;left:50%;top:33.3333%;transform:translate(-50%,-50%);"
                + "display:flex;flex-direction:column;align-items:center;justify-content:center;gap:20px;max-width:72%}"
                + ".center img.logo{max-width:120px;max-height:120px;display:block}"
                + ".name{font:600 22px/1.45 -apple-system,'Segoe UI','Microsoft YaHei',sans-serif;"
                + "color:" + foreground + ";text-align:center;word-break:break-word}"
                + "</style></head><body>" + content + "</body></html>";
    }

    private static String dataUrl(AssetManager assets, String name) {
        if (name == null || name.trim().length() == 0) {
            return null;
        }
        InputStream in = null;
        try {
            in = assets.open(name.trim());
            ByteArrayOutputStream buffer = new ByteArrayOutputStream(64 * 1024);
            byte[] chunk = new byte[32 * 1024];
            int read;
            while ((read = in.read(chunk)) > 0) {
                buffer.write(chunk, 0, read);
            }
            return "data:" + mimeOf(name) + ";base64,"
                    + Base64.encodeToString(buffer.toByteArray(), Base64.NO_WRAP);
        } catch (Exception e) {
            Log.w(TAG, "读取启动屏图片失败(" + name + "): " + e.getMessage());
            return null;
        } finally {
            if (in != null) {
                try {
                    in.close();
                } catch (IOException ignored) {
                    // 忽略
                }
            }
        }
    }

    private static String mimeOf(String name) {
        int dot = name.lastIndexOf('.');
        String ext = dot >= 0 ? name.substring(dot + 1).toLowerCase(Locale.ROOT) : "";
        if ("jpg".equals(ext) || "jpeg".equals(ext)) {
            return "image/jpeg";
        }
        if ("webp".equals(ext)) {
            return "image/webp";
        }
        if ("gif".equals(ext)) {
            return "image/gif";
        }
        if ("svg".equals(ext)) {
            return "image/svg+xml";
        }
        if ("bmp".equals(ext)) {
            return "image/bmp";
        }
        return "image/png";
    }

    private static boolean colorIsDark(String color) {
        String hex = color.trim();
        if (hex.startsWith("#")) {
            hex = hex.substring(1);
        }
        if (hex.length() < 6) {
            return false;
        }
        try {
            int r = Integer.parseInt(hex.substring(0, 2), 16);
            int g = Integer.parseInt(hex.substring(2, 4), 16);
            int b = Integer.parseInt(hex.substring(4, 6), 16);
            return (r * 299 + g * 587 + b * 114) / 1000 < 140;
        } catch (NumberFormatException e) {
            return false;
        }
    }

    private static String htmlEscape(String text) {
        return text.replace("&", "&amp;")
                .replace("<", "&lt;")
                .replace(">", "&gt;")
                .replace("\"", "&quot;");
    }
}
