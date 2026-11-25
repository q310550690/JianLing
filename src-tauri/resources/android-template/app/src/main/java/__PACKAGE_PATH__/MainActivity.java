package __APPLICATION_ID__;

import android.Manifest;
import android.annotation.SuppressLint;
import android.app.Activity;
import android.app.DownloadManager;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.content.ContentValues;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.content.res.ColorStateList;
import android.graphics.Bitmap;
import android.graphics.Color;
import android.media.MediaScannerConnection;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Environment;
import android.os.Handler;
import android.os.Looper;
import android.provider.MediaStore;
import android.util.Log;
import android.util.Base64;
import android.view.KeyEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.view.WindowManager;
import android.webkit.ConsoleMessage;
import android.webkit.CookieManager;
import android.webkit.DownloadListener;
import android.webkit.GeolocationPermissions;
import android.webkit.JavascriptInterface;
import android.webkit.PermissionRequest;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceError;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.ProgressBar;
import android.widget.TextView;

import androidx.core.graphics.Insets;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;
import androidx.core.view.WindowInsetsControllerCompat;
import android.widget.Toast;

import androidx.activity.result.ActivityResultLauncher;
import androidx.activity.result.contract.ActivityResultContracts;
import androidx.annotation.NonNull;
import androidx.annotation.Nullable;
import androidx.appcompat.app.AppCompatActivity;
import androidx.core.content.ContextCompat;
import androidx.core.content.FileProvider;
import androidx.swiperefreshlayout.widget.SwipeRefreshLayout;
import androidx.webkit.WebViewAssetLoader;
import androidx.webkit.WebViewCompat;

import org.json.JSONObject;

import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.FileOutputStream;
import java.io.OutputStream;
import java.net.URLConnection;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Date;
import java.util.List;
import java.util.Locale;

/**
 * 移星智护 — 极简 System WebView 套壳 APP（VIA 浏览器思路）
 *
 * 核心原则（VIA 为什么不出问题）：
 *   1) 小而精，不干预内核：System WebView 自己的 getUserMedia / WebRTC / AEC / AudioManager.MODE_IN_COMMUNICATION
 *      切换能力本身就是 Chrome 同一套内核，只要权限桥 + WebSettings + Manifest 正确，默认就能工作。
 *      任何"手动改 MODE / 注入 JS 覆盖 constraints / 绑 CommunicationDevice / vendor params 覆盖"
 *      都会引入竞态 → 反而让录音打不开或 AEC 失效。
 *   2) Manifest 补齐最小权限 + uses-feature：特别是 MODIFY_AUDIO_SETTINGS（Chromium 自己切 MODE 需要）
 *      与 microphone/camera uses-feature（PackageManager 不声明的话部分 ROM 直接拒绝 Web 侧权限 grant）。
 *   3) 权限桥只有一层：Android 运行时权限 与 WebChromeClient.onPermissionRequest 解耦。
 *      onPermissionRequest 里 — 如果 Android 运行时权限已授权 → 直接 request.grant()（VIA 的做法）。
 *      只在首次安装 / 运行时权限没被授予时才 save request → requestRuntimePermissions → 回调里再 grant。
 *   4) 不引入 Handler.postDelayed / WakeLock / WifiLock / AudioFocus 等任何时序相关对象。
 *   5) WebView 生命周期 / 销毁链 / FileChooser / 返回键 / 全屏等只做规范操作，不加魔改。
 */
@SuppressLint("SetJavaScriptEnabled")
public class MainActivity extends AppCompatActivity {
    private static final String TAG = "MainActivity";

    /**
     * 壳行为开关（打包时由 JianLing 注入）
     */
    private static final boolean FULLSCREEN = __FULLSCREEN__;
    /**
     * 边到边：状态栏透明，内容从屏幕顶端开始（工具栏 / 网页自行铺到状态栏区域）。
     * 关闭时沿用旧行为：状态栏用 STATUS_BAR_COLOR 绘制，内容让出状态栏高度。
     * 非全屏时状态栏一定显示（手机上的常态），只有全屏启动才连状态栏一起隐藏。
     */
    private static final boolean EDGE_TO_EDGE = __EDGE_TO_EDGE__;
    /** 只隐藏导航栏（保留状态栏），底部上滑临时呼出；全屏启动已包含隐藏导航栏 */
    private static final boolean HIDE_NAV_BAR = __HIDE_NAV_BAR__;
    /** 非边到边时绘制状态栏底色的颜色值 */
    private static final String STATUS_BAR_COLOR = "__STATUS_BAR_COLOR__";
    /** 状态栏高度注入页面时使用的 CSS 变量名（可配置，默认 --status-bar-height，与 uni-app 同名） */
    private static final String STATUS_BAR_CSS_VAR = "__STATUS_BAR_CSS_VAR__";

    private static final boolean IGNORE_SSL_ERRORS = __IGNORE_SSL_ERRORS__;
    private static final String UA_SUFFIX = "__UA_SUFFIX__";

    /**
     * 浏览器行为开关（打包时由 JianLing 注入）
     */
    private static final boolean USE_ASSET_LOADER = __USE_ASSET_LOADER__;
    private static final int TEXT_ZOOM = __TEXT_ZOOM__;
    private static final boolean MEDIA_GESTURE_REQUIRED = __MEDIA_GESTURE__;
    private static final boolean ALLOW_ZOOM = __ALLOW_ZOOM__;
    private static final boolean DISABLE_LONG_PRESS = __DISABLE_LONG_PRESS__;
    private static final boolean HIDE_SCROLLBARS = __HIDE_SCROLLBARS__;
    private static final boolean ALLOW_THIRD_PARTY_COOKIES = __THIRD_PARTY_COOKIES__;
    private static final boolean VIDEO_FULLSCREEN = __VIDEO_FULLSCREEN__;
    private static final boolean DEBUG_ENABLED = __DEBUG_ENABLED__;
    private static final boolean OPEN_EXTERNAL_IN_BROWSER = __OPEN_EXTERNAL_IN_BROWSER__;
    private static final String CACHE_MODE = "__CACHE_MODE__";
    /** append（默认 UA + 应用标记）| override（整串替换） */
    private static final String UA_MODE = "__UA_MODE__";
    private static final String USER_AGENT = "__USER_AGENT__";
    /** 静态资源目录名：APK 内是 assets/www，调试期热推落在 files/www */
    private static final String RESOURCE_DIR = "www";
    /** 附加到启动地址的查询参数（可选），如 key=value */
    private static final String START_QUERY = "__START_QUERY__";
    private static final boolean PULL_TO_REFRESH = __PULL_TO_REFRESH__;
    private static final boolean EXIT_ON_BACK = __EXIT_ON_BACK__;
    /** 禁止返回键退出应用：网页退不了时返回键被吃掉，既不退出也不提示 */
    private static final boolean DISABLE_BACK_EXIT = __DISABLE_BACK_EXIT__;
    private static final boolean PROGRESS_BAR = __PROGRESS_BAR__;
    private static final String PROGRESS_COLOR = "__PROGRESS_COLOR__";
    private static final int PROGRESS_HEIGHT_DP = __PROGRESS_HEIGHT__;
    private static final boolean NAV_BAR = __NAV_BAR__;

    /**
     * 显示类开关（打包时由 JianLing 注入）
     */
    private static final boolean LIGHT_STATUS_BAR = __LIGHT_STATUS_BAR__;
    private static final boolean KEEP_SCREEN_ON = __KEEP_SCREEN_ON__;
    private static final boolean SECURE_FLAG = __SECURE_FLAG__;

    /**
     * 每项权限的用途说明配置（JSON，打包时由 JianLing 注入）。
     * 形如：{"camera":{"enabled":true,"title":"相机权限","message":"..."}}
     */
    private static final String PERMISSION_PROMPTS_JSON = "__PERMISSION_PROMPTS_JSON__";
    private static final String PREFS_NAME = "jianling_permissions";

    /** 拉起系统授权前，顶部用途说明的最短展示时长 */
    private static final long PERMISSION_NOTICE_MS = 700L;

    /**
     * 打包时注入的 H5 首页地址
     */
    private static final String DEFAULT_URL = "__START_URL__";

    // =========================================================================
    // WebView 核心对象
    // =========================================================================
    private WebView mWebView;

    /**
     * 顶部进度条 / 加载失败页 / 下拉刷新容器（布局里固定存在，按开关启用）
     */
    private ProgressBar mPageProgress;
    private View mErrorView;
    private SwipeRefreshLayout mSwipeRefresh;

    /** 申请系统权限时顶部的用途说明区（合规：申请前告知目的） */
    private View mPermissionInfo;
    private TextView mPermissionInfoTitle;
    private TextView mPermissionInfoMessage;
    /** 同一次申请多个权限时，第 2 项起的说明条目容器 */
    private LinearLayout mPermissionInfoBlocks;

    /** 边到边模式下状态栏+刘海安全区高度（px），由 WindowInsets 回调写入 */
    private int mStatusBarInset = 0;

    /**
     * 最近一次主文档加载失败的地址，供错误页「重新加载」使用
     */
    private String mLastFailedUrl;

    /**
     * 当前主文档地址。原生桥运行在 WebView 的后台线程，那里调 getUrl() 拿不到值，
     * 所以由 UI 线程（onPageStarted）缓存一份，供来源校验使用。
     */
    private volatile String mCurrentPageUrl;

    /**
     * 返回键双击退出的时间戳
     */
    private long mLastBackPressedAt = 0L;

    /**
     * 本地静态包模式下的资源加载器（https://appassets.androidplatform.net/assets/）。
     * 远程模式下保持 null。
     */
    private WebViewAssetLoader mAssetLoader;

    /**
     * 本地静态包的内置服务（静态托管 + 接口转发，等价 vite proxy）。
     * 启动成功时页面加载 http://127.0.0.1:端口/index.html，相对路径与转发规则同源。
     */
    private LocalServer mLocalServer;

    /**
     * 本地服务给出的页面地址；为空时回退 DEFAULT_URL。
     */
    private String mLocalServerUrl;

    /**
     * 视频全屏播放时的自定义 View 与回调。
     */
    private View mCustomView;
    private WebChromeClient.CustomViewCallback mCustomViewCallback;

    /**
     * 内核 pending 的 Web 权限请求（仅用于首次安装、运行时权限还没授予时的兜底异步流程）。
     * 正常使用（用户已接受运行时权限）时此字段保持 null。
     */
    private PermissionRequest mWebPermissionRequest;

    /**
     * FileChooser 拍照 / 选文件的回调。onShowFileChooser 保存后，Activity Result 回调里回传 Uri[]。
     */
    private ValueCallback<Uri[]> mFilePathCallback;

    /**
     * 最近一次 createImageFile() 生成的拍照临时文件（用于 onActivityResult 给 WebView 拼 Uri）。
     */
    private File mCameraImageFile;

    // =========================================================================
    // Activity Result API（VIA 同样思路：注册在 super.onCreate 之后）
    // =========================================================================
    /**
     * 运行时权限请求回调：VIA 思路 — 完成后只检查是否还挂着一个 pending Web permission request，
     * 如果挂着就按权限状态 grant/deny。其它情况下不做任何事。
     */
    private ActivityResultLauncher<String[]> mPermissionRequestLauncher;

    /**
     * 文件选择 / 相机拍照 combined intent 的结果回调：用拍照临时文件的 Uri 拼回给内核。
     */
    private ActivityResultLauncher<Intent> mFileChooserLauncher;
    /** Android 13+ 下载通知用的权限请求（POST_NOTIFICATIONS） */
    private ActivityResultLauncher<String> mNotificationPermissionLauncher;

    /**
     * 主线程 Handler：仅用于"FileChooser 未选择时 500ms 超时兜底"、以及 onDestroy 清空队列。
     * VIA 同样保留一个轻量 Handler，不作为 AEC 重锁使用。
     */
    private final Handler mMainHandler = new Handler(Looper.getMainLooper());

    @Override
    protected void onCreate(@Nullable Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        try {
            // 1) Activity Result API：必须在 super.onCreate 返回后、STARTED 状态之前注册
            registerActivityResultLaunchers();
            // 2) 无标题 + 全屏沉浸式 + 硬件加速（VIA manifest + window 配置）
            requestWindowFeature(Window.FEATURE_NO_TITLE);
            setupFullScreen();
            applyDisplayFlags();
            // 3) 布局 + 初始化 WebView（WebSettings、Download、WebChromeClient、WebViewClient、loadUrl）
            setContentView(R.layout.activity_main);
            initWebView();
            // 3.1) 边到边：把状态栏高度交给工具栏 / 说明区各自处理
            applyEdgeInsets();
            // 4) 不在启动时申请任何运行时权限：只有页面真正用到某项能力（getUserMedia / 定位）时
            //    才在对应回调里展示用途说明并拉起系统授权，避免"一打开就弹权限"。
            // 5) 启动时清理 7 天以上未访问的临时拍照文件（files/Pictures/IMG_*.jpg）
            //    （VIA 没有这个，但属于磁盘卫生，对用户体感=不越用越膨胀，保留不影响任何核心流程）
            _cleanupOldCameraTempFiles();
        } catch (Exception e) {
            Log.e(TAG, "onCreate error: " + e.getMessage(), e);
        }
    }

    // ========================================================================
    // Activity Result Launchers 注册（权限 + 文件选择 两个）
    // ========================================================================
    private void registerActivityResultLaunchers() {
        // ——— 运行时权限回调 ———
        mPermissionRequestLauncher = registerForActivityResult(
                new ActivityResultContracts.RequestMultiplePermissions(),
                result -> {
                    // 页面触发的单权限请求（定位）：记录拒绝状态
                    if (mPendingSinglePermission != null) {
                        String permission = mPendingSinglePermission;
                        mPendingSinglePermission = null;
                        boolean granted = result != null
                                && Boolean.TRUE.equals(result.get(permission));
                        String key = _promptKeyForPermission(permission);
                        if (!granted && key != null) {
                            getSharedPreferences(PREFS_NAME, MODE_PRIVATE)
                                    .edit().putString("runtime:" + key, "denied").apply();
                        }
                    }
                    // 只处理 pending 的 Web permission request（首次安装场景）
                    if (mWebPermissionRequest != null) {
                        _handlePendingWebPermissionRequest();
                    }
                    // 系统框返回后一律收起说明：用户可能停留很久，期间内核已超时 cancel 掉原请求，
                    // 此时不能再依赖 pending 字段决定是否收起，否则说明会一直挂在顶部
                    _hidePermissionInfo();
                    _resolvePendingGeolocation();
                }
        );
        // ——— 文件选择 / 相机回调 ———
        mFileChooserLauncher = registerForActivityResult(
                new ActivityResultContracts.StartActivityForResult(),
                result -> {
                    if (mFilePathCallback == null) {
                        return;
                    }
                    Uri[] results = null;
                    try {
                        if (result.getResultCode() == Activity.RESULT_OK) {
                            Intent data = result.getData();
                            // 有返回 data（图库选图 / 系统文件选择器）→ 直接取
                            if (data != null && data.getDataString() != null) {
                                results = new Uri[]{ Uri.parse(data.getDataString()) };
                            } else if (data != null && data.getClipData() != null) {
                                // 多选：clipData 有多个 Uri
                                int n = data.getClipData().getItemCount();
                                results = new Uri[n];
                                for (int i = 0; i < n; i++) {
                                    results[i] = data.getClipData().getItemAt(i).getUri();
                                }
                            } else if (mCameraImageFile != null && mCameraImageFile.exists()) {
                                // 没有 data（纯相机拍照的典型返回）→ 用 createImageFile 创建好的文件拼 Uri
                                results = new Uri[]{
                                        FileProvider.getUriForFile(
                                                this,
                                                getPackageName() + ".fileprovider",
                                                mCameraImageFile
                                        )
                                };
                            }
                        }
                    } catch (Exception e) {
                        Log.w(TAG, "FileChooser result error: " + e.getMessage());
                    }
                    // 无论成功失败，必须把值回传内核，否则 H5 会永远卡住
                    mFilePathCallback.onReceiveValue(results);
                    mFilePathCallback = null;
                }
        );
        // ——— 下载通知权限（Android 13+）———
        mNotificationPermissionLauncher = registerForActivityResult(
                new ActivityResultContracts.RequestPermission(),
                granted -> Log.i(TAG, "POST_NOTIFICATIONS granted=" + granted)
        );
    }

    // ========================================================================
    // 全屏沉浸式（VIA 默认也是沉浸式全屏，不显示系统栏）
    // ========================================================================
    private void setupFullScreen() {
        try {
            Window window = getWindow();
            int flags = View.SYSTEM_UI_FLAG_LAYOUT_STABLE;
            // 边到边：内容延伸到系统栏区域，由 WindowCompat 关闭系统的自动避让
            final boolean edge = EDGE_TO_EDGE && !FULLSCREEN;

            if (FULLSCREEN) {
                // 沉浸式全屏：隐藏状态栏 + 导航栏，触摸时短暂浮现
                window.setFlags(
                        WindowManager.LayoutParams.FLAG_FULLSCREEN,
                        WindowManager.LayoutParams.FLAG_FULLSCREEN
                );
                flags |= View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                        | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                        | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                        | View.SYSTEM_UI_FLAG_FULLSCREEN
                        | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY;
            } else {
                // 非全屏：状态栏保持显示（手机上的常态），不再提供单独隐藏状态栏的开关
                if (edge) {
                    // 边到边：状态栏透明，内容从屏幕顶端开始（工具栏灰底铺进状态栏）。
                    // 不再用 legacy 的 SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN，改用官方 WindowCompat；
                    // 底部导航栏 / 键盘由 applyEdgeInsets() 里的 inset 补回去。
                    WindowCompat.setDecorFitsSystemWindows(window, false);
                    window.setStatusBarColor(Color.TRANSPARENT);
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                        // 关掉系统给透明状态栏加的对比度蒙层，否则会看到一条灰纱
                        window.setStatusBarContrastEnforced(false);
                    }
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                        // 允许内容铺进刘海 / 挖孔区（工具栏背景能一直铺到屏幕顶端），
                        // 实际安全区由 inset 的 displayCutout 兜住
                        WindowManager.LayoutParams attrs = window.getAttributes();
                        attrs.layoutInDisplayCutoutMode =
                                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
                        window.setAttributes(attrs);
                    }
                } else {
                    WindowCompat.setDecorFitsSystemWindows(window, true);
                    try {
                        window.setStatusBarColor(Color.parseColor(STATUS_BAR_COLOR));
                    } catch (Exception ignored) {
                        /* 颜色值非法时沿用主题默认 */
                    }
                }
            }

            if (edge) {
                // 边到边模式下不要再用 setSystemUiVisibility 覆盖（会清掉 WindowCompat 设的 LAYOUT_*），
                // 状态栏图标深浅走 WindowInsetsController
                WindowInsetsControllerCompat controller =
                        WindowCompat.getInsetsController(window, window.getDecorView());
                if (controller != null) {
                    controller.setAppearanceLightStatusBars(LIGHT_STATUS_BAR);
                }
            } else {
                if (LIGHT_STATUS_BAR && !FULLSCREEN) {
                    // 浅色背景上用深色状态栏图标（API 23+）
                    flags |= View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR;
                }
                window.getDecorView().setSystemUiVisibility(flags);
            }

            // 切屏 / 键盘弹出时重新对齐（防止底部 nav 把 webview 顶上去）
            window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE
                    | WindowManager.LayoutParams.SOFT_INPUT_STATE_HIDDEN);

            // 只隐藏导航栏、保留状态栏：全屏启动会把两个都隐藏，这是单独的开关
            if (HIDE_NAV_BAR && !FULLSCREEN) {
                WindowInsetsControllerCompat navController =
                        WindowCompat.getInsetsController(window, window.getDecorView());
                if (navController != null) {
                    navController.hide(WindowInsetsCompat.Type.navigationBars());
                    navController.setSystemBarsBehavior(
                            WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                }
            }
        } catch (Exception e) {
            Log.w(TAG, "setupFullScreen skipped: " + e.getMessage());
        }
    }

    /**
     * 边到边：内容从屏幕顶端开始，工具栏把自身背景铺到状态栏区域，
     * 用 paddingTop 补回状态栏（含刘海）高度，保证按钮不被状态栏图标压住。
     * 网页本身不加任何顶部内边距 —— 只把状态栏高度以 CSS 变量
     * {@link #STATUS_BAR_CSS_VAR} 注入页面，由页面自己决定怎么用。
     * 底部把导航栏 / 输入法高度补回根布局，等价于原来的 adjustResize 行为。
     */
    private void applyEdgeInsets() {
        if (!EDGE_TO_EDGE || FULLSCREEN) {
            return;
        }
        try {
            View root = findViewById(R.id.rootView);
            if (root == null) {
                return;
            }
            ViewCompat.setOnApplyWindowInsetsListener(root, (view, windowInsets) -> {
                Insets statusBars = windowInsets.getInsets(WindowInsetsCompat.Type.statusBars());
                Insets cutout = windowInsets.getInsets(WindowInsetsCompat.Type.displayCutout());
                Insets navBars = windowInsets.getInsets(WindowInsetsCompat.Type.navigationBars());
                Insets ime = windowInsets.getInsets(WindowInsetsCompat.Type.ime());

                int safeTop = Math.max(statusBars.top, cutout.top);
                int bottom = Math.max(navBars.bottom, ime.bottom);

                if (mStatusBarInset != safeTop) {
                    mStatusBarInset = safeTop;
                    _resizeNavBarForInset(safeTop);
                    if (mPermissionInfo != null
                            && mPermissionInfo.getVisibility() == View.VISIBLE) {
                        _placePermissionInfoBelowStatusBar();
                    }
                    // 状态栏高度变化（旋转 / 分屏）时同步给页面
                    injectStatusBarHeight();
                }
                if (view.getPaddingBottom() != bottom) {
                    view.setPadding(0, 0, 0, bottom);
                }
                return windowInsets;
            });
            root.requestApplyInsets();
        } catch (Exception e) {
            Log.w(TAG, "applyEdgeInsets skipped: " + e.getMessage());
        }
    }

    /**
     * 把状态栏高度注入页面 CSS 变量 {@link #STATUS_BAR_CSS_VAR}（默认 {@code --status-bar-height}，与 uni-app 同名）。
     * 只在「边到边且非全屏」时注入：全屏时状态栏被隐藏、非边到边时页面本就从状态栏下方开始，
     * 这两种情况都不该给页面凭空多出一段留白。
     */
    private void injectStatusBarHeight() {
        if (mWebView == null || FULLSCREEN || !EDGE_TO_EDGE || STATUS_BAR_CSS_VAR.isEmpty()) {
            return;
        }
        try {
            int px = Math.max(0, mStatusBarInset);
            float density = getResources().getDisplayMetrics().density;
            int cssPx = density > 0 ? Math.round(px / density) : 0;
            String js = "(function(){var r=document.documentElement;"
                    + "if(!r){return;}r.style.setProperty('" + STATUS_BAR_CSS_VAR
                    + "','" + cssPx + "px');})();";
            mWebView.evaluateJavascript(js, null);
        } catch (Exception e) {
            Log.w(TAG, "injectStatusBarHeight skipped: " + e.getMessage());
        }
    }

    /** 工具栏高度 = 44dp + 状态栏高度，多出来的部分作为顶部 padding。 */
    private void _resizeNavBarForInset(int topInset) {
        View navBar = findViewById(R.id.navBar);
        if (navBar == null || !NAV_BAR) {
            return;
        }
        int base = Math.round(44 * getResources().getDisplayMetrics().density);
        ViewGroup.LayoutParams params = navBar.getLayoutParams();
        int wantHeight = base + topInset;
        if (params.height != wantHeight) {
            params.height = wantHeight;
            navBar.setLayoutParams(params);
        }
        if (navBar.getPaddingTop() != topInset) {
            navBar.setPadding(navBar.getPaddingLeft(), topInset,
                    navBar.getPaddingRight(), navBar.getPaddingBottom());
        }
    }

    /**
     * 屏幕常亮 / 防截屏（打包时可选的显示类开关）
     */
    private void applyDisplayFlags() {
        try {
            Window window = getWindow();
            if (KEEP_SCREEN_ON) {
                window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            }
            if (SECURE_FLAG) {
                window.addFlags(WindowManager.LayoutParams.FLAG_SECURE);
            }
        } catch (Exception e) {
            Log.w(TAG, "applyDisplayFlags skipped: " + e.getMessage());
        }
    }

    // ========================================================================
    // 运行时权限：只在页面真正调用相关能力时，先显示用途说明再拉起系统授权
    // ========================================================================
    private String mPendingSinglePermission;

    /** 页面请求定位时暂存的回调（等运行时权限结果回来再答复） */
    private String mGeoOrigin;
    private GeolocationPermissions.Callback mGeoCallback;

    private static JSONObject sPromptConfigs;

    /** 用当前定位权限的授予状态答复 WebView 挂起的定位请求 */
    private void _resolvePendingGeolocation() {
        if (mGeoCallback == null) {
            return;
        }
        GeolocationPermissions.Callback callback = mGeoCallback;
        String origin = mGeoOrigin;
        mGeoCallback = null;
        mGeoOrigin = null;
        boolean granted = ContextCompat.checkSelfPermission(this, Manifest.permission.ACCESS_FINE_LOCATION)
                == PackageManager.PERMISSION_GRANTED;
        try {
            callback.invoke(origin, granted, false);
        } catch (Exception e) {
            Log.w(TAG, "geolocation callback failed: " + e.getMessage());
        }
    }

    /** 运行时权限 → 说明配置键。 */
    private static String _promptKeyForPermission(String permission) {
        if (Manifest.permission.CAMERA.equals(permission)) return "camera";
        if (Manifest.permission.RECORD_AUDIO.equals(permission)) return "microphone";
        if (Manifest.permission.READ_EXTERNAL_STORAGE.equals(permission)
                || Manifest.permission.WRITE_EXTERNAL_STORAGE.equals(permission)
                || Manifest.permission.READ_MEDIA_IMAGES.equals(permission)
                || Manifest.permission.READ_MEDIA_VIDEO.equals(permission)
                || Manifest.permission.READ_MEDIA_AUDIO.equals(permission)) return "storage";
        if (Manifest.permission.ACCESS_FINE_LOCATION.equals(permission)
                || Manifest.permission.ACCESS_COARSE_LOCATION.equals(permission)) return "location";
        if (Manifest.permission.POST_NOTIFICATIONS.equals(permission)) return "notification";
        if (Manifest.permission.BLUETOOTH_SCAN.equals(permission)
                || Manifest.permission.BLUETOOTH_CONNECT.equals(permission)) return "bluetooth";
        return null;
    }

    /** 取某一项权限的弹窗配置（解析一次后缓存）。 */
    private static JSONObject _promptConfig(String key) {
        if (sPromptConfigs == null) {
            try {
                sPromptConfigs = new JSONObject(PERMISSION_PROMPTS_JSON);
            } catch (Exception e) {
                sPromptConfigs = new JSONObject();
            }
        }
        return sPromptConfigs.optJSONObject(key);
    }

    /**
     * 先展示顶部用途说明，再执行 permissions 相关动作（拉起系统授权）。
     * 系统授权窗口会直接盖住页面，同步调用会导致说明来不及绘制（ROM 自动授权时整段说明完全不可见），
     * 因此留出最短展示时长，保证用户与审核能看到「申请目的」。
     *
     * keys：本次要申请的权限，页面可能一次要两个（例如音视频同采），卡片里会把它们都列出来。
     */
    private void _showNoticeThen(List<String> keys, final Runnable action) {
        List<String> shown = new ArrayList<>();
        if (keys != null) {
            for (String key : keys) {
                JSONObject config = key == null ? null : _promptConfig(key);
                if (config != null && config.optBoolean("enabled", true)) {
                    shown.add(key);
                }
            }
        }
        if (!shown.isEmpty()) {
            _showPermissionInfo(shown);
            if (mWebView != null) {
                mWebView.postDelayed(() -> {
                    if (isFinishing() || isDestroyed()) {
                        return;
                    }
                    action.run();
                }, PERMISSION_NOTICE_MS);
                return;
            }
        }
        action.run();
    }

    /** 权限的中文名（配置里没写标题时兜底）。 */
    private static String _permissionLabel(String key) {
        switch (key) {
            case "camera": return "相机权限";
            case "microphone": return "麦克风权限";
            case "storage": return "存储权限";
            case "location": return "地理位置权限";
            case "notification": return "通知权限";
            case "bluetooth": return "蓝牙权限";
            case "nfc": return "NFC 权限";
            case "vibrate": return "振动权限";
            case "foreground": return "前台服务权限";
            case "call": return "拨打电话权限";
            case "install": return "应用安装权限";
            default: return key + " 权限";
        }
    }

    /**
     * 顶部用途说明：每条原样显示配置的「说明标题」+「说明文案」。
     * 一次申请多个权限时仍然只有一张卡片，条目之间用分隔线排开。
     */
    private void _showPermissionInfo(List<String> keys) {
        if (mPermissionInfo == null || keys == null || keys.isEmpty()) {
            return;
        }
        try {
            if (mPermissionInfoBlocks != null) {
                mPermissionInfoBlocks.removeAllViews();
            }
            boolean first = true;
            for (String key : keys) {
                String title = _permissionTitle(key);
                String message = _permissionMessage(key);
                if (first) {
                    if (mPermissionInfoTitle != null) {
                        mPermissionInfoTitle.setText(title);
                    }
                    if (mPermissionInfoMessage != null) {
                        mPermissionInfoMessage.setText(message);
                    }
                    first = false;
                    continue;
                }
                if (mPermissionInfoBlocks == null) {
                    continue;
                }
                View block = getLayoutInflater().inflate(R.layout.perm_info_block,
                        mPermissionInfoBlocks, false);
                TextView blockTitle = block.findViewById(R.id.permInfoBlockTitle);
                TextView blockMessage = block.findViewById(R.id.permInfoBlockMessage);
                if (blockTitle != null) {
                    blockTitle.setText(title);
                }
                if (blockMessage != null) {
                    blockMessage.setText(message);
                }
                mPermissionInfoBlocks.addView(block);
            }
            _placePermissionInfoBelowStatusBar();
            mPermissionInfo.setVisibility(View.VISIBLE);
        } catch (Exception e) {
            Log.w(TAG, "show permission info failed: " + e.getMessage());
        }
    }

    /** 说明条目标题：原样使用配置里的「说明标题」，不再自行拼接应用名。 */
    private String _permissionTitle(String key) {
        JSONObject config = _promptConfig(key);
        String label = config == null ? "" : config.optString("title", "").trim();
        if (label.isEmpty() || "权限申请".equals(label)) {
            label = _permissionLabel(key);
        }
        return label;
    }

    /** 说明条目正文：优先用配置文案，没配就用通用兜底。 */
    private String _permissionMessage(String key) {
        JSONObject config = _promptConfig(key);
        String message = config == null ? "" : config.optString("message", "").trim();
        if (message.isEmpty()) {
            message = "用于向应用提供该权限对应的必要功能，仅在相关功能使用时访问。";
        }
        return message;
    }

    /** 说明卡片固定在状态栏下沿：与操作工具栏是否显示无关，总是贴内容区最顶部。 */
    private void _placePermissionInfoBelowStatusBar() {
        if (mPermissionInfo == null) {
            return;
        }
        ViewGroup.LayoutParams params = mPermissionInfo.getLayoutParams();
        if (!(params instanceof ViewGroup.MarginLayoutParams)) {
            return;
        }
        // 边到边时内容从屏幕顶端开始，需要下移状态栏高度；非边到边时内容本身已在状态栏下方
        int top = Math.max(0, mStatusBarInset);
        ViewGroup.MarginLayoutParams margin = (ViewGroup.MarginLayoutParams) params;
        if (margin.topMargin != top) {
            margin.topMargin = top;
            mPermissionInfo.setLayoutParams(margin);
        }
    }

    private void _hidePermissionInfo() {
        if (mPermissionInfo != null) {
            mPermissionInfo.setVisibility(View.GONE);
        }
    }

    // ========================================================================
    // WebView 初始化 + WebSettings / WebChromeClient / WebViewClient（VIA 同款配置）
    // ========================================================================
    /** 启动地址附加配置的查询参数（已自动处理 ? / &） */
    private String withStartQuery(String url) {
        if (url == null || START_QUERY == null || START_QUERY.isEmpty()) {
            return url;
        }
        return url + (url.contains("?") ? "&" : "?") + START_QUERY;
    }

    private void initWebView() {
        try {
            mWebView = findViewById(R.id.webView);
            if (mWebView == null) {
                throw new IllegalStateException("layout activity_main missing R.id.webView");
            }

            // 进度条 / 错误页 / 下拉刷新 / 权限用途说明区
            mPageProgress = findViewById(R.id.pageProgress);
            mErrorView = findViewById(R.id.errorView);
            mSwipeRefresh = findViewById(R.id.swipeRefresh);
            mPermissionInfo = findViewById(R.id.permissionInfo);
            mPermissionInfoTitle = findViewById(R.id.permissionInfoTitle);
            mPermissionInfoMessage = findViewById(R.id.permissionInfoMessage);
            mPermissionInfoBlocks = findViewById(R.id.permissionInfoBlocks);
            if (mSwipeRefresh != null) {
                mSwipeRefresh.setEnabled(PULL_TO_REFRESH);
                mSwipeRefresh.setOnRefreshListener(() -> {
                    if (mWebView != null) {
                        hideErrorView();
                        mWebView.reload();
                    } else {
                        mSwipeRefresh.setRefreshing(false);
                    }
                });
            }
            View retryButton = findViewById(R.id.errorRetry);
            if (retryButton != null) {
                retryButton.setOnClickListener(v -> {
                    if (mWebView == null) {
                        return;
                    }
                    hideErrorView();
                    String url = mLastFailedUrl != null
                            ? mLastFailedUrl
                            : withStartQuery(mLocalServerUrl != null ? mLocalServerUrl : DEFAULT_URL);
                    mWebView.loadUrl(url);
                });
            }

            // 顶部浏览器操作栏：前进 / 后退 / 刷新
            View navBar = findViewById(R.id.navBar);
            if (navBar != null) {
                navBar.setVisibility(NAV_BAR ? View.VISIBLE : View.GONE);
                if (NAV_BAR) {
                    View navBack = findViewById(R.id.navBack);
                    if (navBack != null) {
                        navBack.setOnClickListener(v -> {
                            if (mWebView != null && mWebView.canGoBack()) {
                                mWebView.goBack();
                            }
                        });
                    }
                    View navForward = findViewById(R.id.navForward);
                    if (navForward != null) {
                        navForward.setOnClickListener(v -> {
                            if (mWebView != null && mWebView.canGoForward()) {
                                mWebView.goForward();
                            }
                        });
                    }
                    View navRefresh = findViewById(R.id.navRefresh);
                    if (navRefresh != null) {
                        navRefresh.setOnClickListener(v -> {
                            if (mWebView != null) {
                                hideErrorView();
                                mWebView.reload();
                            }
                        });
                    }
                }
            }

            // 进度条：显示开关 / 颜色 / 高度
            if (mPageProgress != null) {
                if (!PROGRESS_BAR) {
                    mPageProgress.setVisibility(View.GONE);
                } else {
                    try {
                        int height = (int) (PROGRESS_HEIGHT_DP * getResources().getDisplayMetrics().density);
                        ViewGroup.LayoutParams lp = mPageProgress.getLayoutParams();
                        lp.height = Math.max(1, height);
                        mPageProgress.setLayoutParams(lp);
                        mPageProgress.setProgressTintList(
                                ColorStateList.valueOf(Color.parseColor(PROGRESS_COLOR)));
                    } catch (Exception e) {
                        Log.w(TAG, "progress bar style skipped: " + e.getMessage());
                    }
                }
            }
            WebSettings settings = mWebView.getSettings();

            // 基础 WebView 能力
            settings.setJavaScriptEnabled(true);
            settings.setDomStorageEnabled(true);
            settings.setDatabaseEnabled(true);
            settings.setLoadsImagesAutomatically(true);
            settings.setBlockNetworkImage(false);
            settings.setBlockNetworkLoads(false);
            settings.setGeolocationEnabled(true);

            // 缓存策略（HTTP cache + IDB / LocalStorage，W3C AppCache 在 compileSdk 33+ 已移除公开 API）
            settings.setCacheMode(cacheModeValue());

            // 缩放（JianLing 可关闭）
            settings.setSupportZoom(ALLOW_ZOOM);
            settings.setBuiltInZoomControls(ALLOW_ZOOM);
            settings.setDisplayZoomControls(false);

            // 屏幕适配
            settings.setUseWideViewPort(true);
            settings.setLoadWithOverviewMode(true);

            // 字体缩放（100 = 跟随系统；JianLing 可注入 50–200 解决"最小字太小"）
            settings.setTextZoom(TEXT_ZOOM);

            // 多窗口 / 小窗模式
            settings.setSupportMultipleWindows(false);
            settings.setJavaScriptCanOpenWindowsAutomatically(true);

            // 文件访问（H5 <input type=file> 需要）
            settings.setAllowFileAccess(true);
            settings.setAllowContentAccess(true);

            // 混合内容（HTTPS 页面里嵌套 HTTP 资源 — 例如 H5 里的 HTTP 信令服务）
            settings.setMixedContentMode(__MIXED_CONTENT_MODE__);

            // 媒体自动播放：false（= MEDIA_GESTURE_REQUIRED 关闭）= 无需用户手势即可有声自动播放
            settings.setMediaPlaybackRequiresUserGesture(MEDIA_GESTURE_REQUIRED);

            // 长按（默认屏蔽长按选取文字 / 长按图片菜单）
            if (DISABLE_LONG_PRESS) {
                mWebView.setLongClickable(false);
                mWebView.setHapticFeedbackEnabled(false);
                mWebView.setOnLongClickListener(v -> true);
            }

            // 滚动条
            mWebView.setHorizontalScrollBarEnabled(!HIDE_SCROLLBARS);
            mWebView.setVerticalScrollBarEnabled(!HIDE_SCROLLBARS);

            // Cookie（含第三方 Cookie，跨域 / iframe 内登录态需要）
            CookieManager cookieManager = CookieManager.getInstance();
            cookieManager.setAcceptCookie(true);
            cookieManager.setAcceptThirdPartyCookies(mWebView, ALLOW_THIRD_PARTY_COOKIES);

            // WebView 调试（Chrome DevTools → chrome://inspect 可连）
            WebView.setWebContentsDebuggingEnabled(DEBUG_ENABLED);

            // 本地静态包：WebViewAssetLoader 走 https://appassets.androidplatform.net/assets/，
            // 规避 file:// 下 localStorage / XHR / 跨域的各种限制
            if (USE_ASSET_LOADER) {
                // 调试期「重新推送资源」把 dist 推到 files/www；有 index.html 就优先读外部目录
                File pushedRoot = getExternalFilesDir(null);
                boolean usePushed = pushedRoot != null
                        && new File(new File(pushedRoot, RESOURCE_DIR), "index.html").isFile();
                if (usePushed) {
                    Log.i(TAG, "使用外部推送资源: "
                            + new File(pushedRoot, RESOURCE_DIR).getAbsolutePath());
                }
                // 优先启用内置本地服务：页面与 /api、/ws 同源，免 CORS、免改前端 base。
                // 启动失败（例如没写配置）时退回 WebViewAssetLoader。
                try {
                    mLocalServer = usePushed
                            ? new LocalServer(this, pushedRoot, RESOURCE_DIR, "index.html")
                            : new LocalServer(this, RESOURCE_DIR, "index.html");
                    int port = mLocalServer.start();
                    mLocalServerUrl = "http://127.0.0.1:" + port + mLocalServer.getBasePath() + "index.html";
                } catch (Exception e) {
                    Log.w(TAG, "本地服务启动失败，回退 WebViewAssetLoader: " + e.getMessage());
                    mLocalServer = null;
                    mLocalServerUrl = null;
                    WebViewAssetLoader.PathHandler assetsHandler =
                            new WebViewAssetLoader.AssetsPathHandler(this);
                    mAssetLoader = new WebViewAssetLoader.Builder()
                            .addPathHandler("/assets/",
                                    new PushedAssetsHandler(pushedRoot, assetsHandler))
                            .build();
                }
            }

            // 硬件加速（manifest 里也声明了）
            mWebView.setLayerType(View.LAYER_TYPE_HARDWARE, null);

            // User-Agent：override 用配置的整串；append 在系统默认 UA 后追加应用标记
            String baseUa = settings.getUserAgentString();
            String customUa = USER_AGENT == null ? "" : USER_AGENT.trim();
            if ("override".equals(UA_MODE) && !customUa.isEmpty()) {
                settings.setUserAgentString(customUa);
            } else {
                String composed = baseUa + UA_SUFFIX;
                if (!customUa.isEmpty()) {
                    composed = composed + " " + customUa;
                }
                settings.setUserAgentString(composed);
            }

            // H5 → 壳的原生桥：window.JianLing.*（方法内部按当前页面来源校验）
            mWebView.addJavascriptInterface(new JianLingBridge(), "JianLing");

            // —— 下载监听 ——
            // 普通 http(s) 文件交给系统 DownloadManager（自带进度通知 + 落到系统「下载」目录）；
            // assets / blob / data 这类系统下载器取不到的，改由页面 fetch 出内容再回传壳保存。
            mWebView.setDownloadListener(new DownloadListener() {
                @Override
                public void onDownloadStart(String url, String userAgent,
                                            String contentDisposition, String mimetype,
                                            long contentLength) {
                    if (url == null || url.isEmpty()) {
                        return;
                    }
                    try {
                        String fileName = _guessDownloadName(url, contentDisposition, mimetype);
                        _ensureNotificationPermission();
                        if (_enqueueSystemDownload(url, fileName, userAgent, mimetype)) {
                            return;
                        }
                        _requestPageSaveDownload(url, fileName, mimetype, contentLength);
                    } catch (Exception ex) {
                        Log.e(TAG, "download failed: " + ex.getMessage());
                    }
                }
            });

            // —— WebViewClient：页面加载 / 错误 / SSL / 外部链接 处理 ——
            mWebView.setWebViewClient(new WebViewClient() {

                /**
                 * 本地静态包：把 appassets.androidplatform.net/assets/** 映射到 APK 内的 assets/www。
                 */
                @Override
                public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest request) {
                    if (mAssetLoader != null && request != null && request.getUrl() != null) {
                        WebResourceResponse response = mAssetLoader.shouldInterceptRequest(request.getUrl());
                        if (response != null) {
                            return response;
                        }
                    }
                    return super.shouldInterceptRequest(view, request);
                }

                @Override
                public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                    if (request != null && request.getUrl() != null) {
                        String scheme = request.getUrl().getScheme();
                        // http(s) 默认在当前 WebView 内加载；tel / sms / mailto / intent 等丢给系统
                        if ("http".equals(scheme) || "https".equals(scheme)) {
                            // 开启「外链用系统浏览器」时，非本站域名交给系统浏览器
                            if (OPEN_EXTERNAL_IN_BROWSER && !isTrustedHost(request.getUrl())) {
                                try {
                                    startActivity(new Intent(Intent.ACTION_VIEW, request.getUrl()));
                                } catch (Exception ex) {
                                    Log.w(TAG, "external link failed: " + ex.getMessage());
                                }
                                return true;
                            }
                            return false;
                        }
                        try {
                            Intent i = new Intent(Intent.ACTION_VIEW, request.getUrl());
                            startActivity(i);
                        } catch (Exception ex) {
                            Log.w(TAG, "shouldOverrideUrlLoading external scheme failed: " + ex.getMessage());
                        }
                        return true;
                    }
                    return super.shouldOverrideUrlLoading(view, request);
                }

                @Override
                public void onPageStarted(WebView view, String url, Bitmap favicon) {
                    super.onPageStarted(view, url, favicon);
                    Log.d(TAG, "onPageStarted: " + url);
                    mCurrentPageUrl = url;
                    if (PROGRESS_BAR && mPageProgress != null) {
                        mPageProgress.setVisibility(View.VISIBLE);
                        mPageProgress.setProgress(0);
                    }
                }

                @Override
                public void onPageFinished(WebView view, String url) {
                    super.onPageFinished(view, url);
                    Log.d(TAG, "onPageFinished: " + url);
                    // 页面 DOM 就绪后注入状态栏高度（STATUS_BAR_CSS_VAR），由页面自己决定怎么用
                    injectStatusBarHeight();
                    if (mPageProgress != null) {
                        mPageProgress.setVisibility(View.GONE);
                    }
                    if (mSwipeRefresh != null && mSwipeRefresh.isRefreshing()) {
                        mSwipeRefresh.setRefreshing(false);
                    }
                    // VIA 不做 JS 注入。保持干净、让 H5 自己决定 getUserMedia / RTCPeerConnection 约束。
                }

                @Override
                public void onReceivedError(WebView view, WebResourceRequest request, WebResourceError error) {
                    super.onReceivedError(view, request, error);
                    if (error != null) {
                        Log.e(TAG, "onReceivedError: url=" + (request == null ? "null" : request.getUrl())
                                + " desc=" + error.getDescription());
                    }
                    // 只有主文档失败才显示错误页，子资源失败不打扰用户
                    boolean mainFrame = request == null || request.isForMainFrame();
                    if (mainFrame) {
                        mLastFailedUrl = request != null && request.getUrl() != null
                                ? request.getUrl().toString()
                                : null;
                        showErrorView();
                    }
                }

                @Override
                public void onReceivedHttpError(WebView view, WebResourceRequest request, WebResourceResponse errorResponse) {
                    super.onReceivedHttpError(view, request, errorResponse);
                    if (errorResponse != null) {
                        Log.e(TAG, "onReceivedHttpError: " + errorResponse.getStatusCode()
                                + " url=" + (request == null ? "null" : request.getUrl()));
                    }
                }

                @Override
                public void onReceivedSslError(WebView view, android.webkit.SslErrorHandler handler,
                                               android.net.http.SslError error) {
                    // 默认拒绝自签/错域名证书；内网调试可在打包时打开忽略开关
                    if (handler != null) {
                        if (IGNORE_SSL_ERRORS) {
                            handler.proceed();
                        } else {
                            handler.cancel();
                        }
                    }
                }
            });

            // —— WebChromeClient：权限桥 + FileChooser + Console 日志（VIA 最核心的 bridge 层）——
            mWebView.setWebChromeClient(new WebChromeClient() {

                /**
                 * 顶部进度条：100 时收起
                 */
                @Override
                public void onProgressChanged(WebView view, int newProgress) {
                    super.onProgressChanged(view, newProgress);
                    if (newProgress >= 100 && mSwipeRefresh != null && mSwipeRefresh.isRefreshing()) {
                        mSwipeRefresh.setRefreshing(false);
                    }
                    if (!PROGRESS_BAR || mPageProgress == null) {
                        return;
                    }
                    if (newProgress >= 100) {
                        mPageProgress.setVisibility(View.GONE);
                    } else {
                        mPageProgress.setVisibility(View.VISIBLE);
                        mPageProgress.setProgress(newProgress);
                    }
                }

                /**
                 * VIA 核心思路：Web 侧发起 RESOURCE_AUDIO_CAPTURE / RESOURCE_VIDEO_CAPTURE 请求时，
                 * 只要 Manifest 声明 + 运行时权限已授予，就直接 request.grant(...)。
                 * 不弹 APP 内二次确认框、不切 MODE、不注入 JS — 全交给内核默认行为，最稳。
                 */
                @Override
                public void onPermissionRequest(PermissionRequest request) {
                    if (request == null || request.getResources() == null) {
                        return;
                    }
                    try {
                        String[] resources = request.getResources();
                        boolean needRuntimePermissionNow = false;

                        // 1) 如果请求 AUDIO：看 RECORD_AUDIO 是否已 granted
                        if (_containsResource(resources, PermissionRequest.RESOURCE_AUDIO_CAPTURE)) {
                            if (ContextCompat.checkSelfPermission(MainActivity.this, Manifest.permission.RECORD_AUDIO)
                                    != PackageManager.PERMISSION_GRANTED) {
                                needRuntimePermissionNow = true;
                            }
                        }
                        // 2) 如果请求 VIDEO：看 CAMERA 是否已 granted
                        if (_containsResource(resources, PermissionRequest.RESOURCE_VIDEO_CAPTURE)) {
                            if (ContextCompat.checkSelfPermission(MainActivity.this, Manifest.permission.CAMERA)
                                    != PackageManager.PERMISSION_GRANTED) {
                                needRuntimePermissionNow = true;
                            }
                        }

                        if (needRuntimePermissionNow) {
                            // 首次安装场景（onCreate 请求还没跑完 / 或用户选了"仅在使用时允许"但权限实际给了之后又被撤销）
                            // 保存 request → 请求运行时权限 → 在 launcher callback 里再 grant
                            mWebPermissionRequest = request;
                            List<String> need = new ArrayList<>();
                            if (_containsResource(resources, PermissionRequest.RESOURCE_AUDIO_CAPTURE)) {
                                need.add(Manifest.permission.RECORD_AUDIO);
                            }
                            if (_containsResource(resources, PermissionRequest.RESOURCE_VIDEO_CAPTURE)) {
                                need.add(Manifest.permission.CAMERA);
                            }
                            if (!need.isEmpty()) {
                                // 与启动流程一致：先显示用途说明，再拉起系统授权。
                                // 音视频同时申请时，一个卡片里列出两条说明。
                                List<String> infoKeys = new ArrayList<>();
                                for (String permission : need) {
                                    String infoKey = _promptKeyForPermission(permission);
                                    if (infoKey != null && !infoKeys.contains(infoKey)) {
                                        infoKeys.add(infoKey);
                                    }
                                }
                                final String[] permissions = need.toArray(new String[0]);
                                _showNoticeThen(infoKeys,
                                        () -> mPermissionRequestLauncher.launch(permissions));
                            } else {
                                request.deny();
                                mWebPermissionRequest = null;
                            }
                            return;
                        }

                        // —— 常规路径：运行时权限已授予，且启动时已弹过对应权限的说明 → 直接 grant
                        // （说明弹窗只在启动的逐项流程里出现，避免 H5 调用时重复打扰用户）
                        request.grant(resources);
                    } catch (Exception e) {
                        Log.e(TAG, "onPermissionRequest error: " + e.getMessage(), e);
                        try { if (request != null) request.deny(); } catch (Exception ignored) { /* noop */ }
                    }
                }

                @Override
                public void onPermissionRequestCanceled(PermissionRequest request) {
                    super.onPermissionRequestCanceled(request);
                    // 内核自己 cancel 了（超时、页面跳转、H5 主动 cancel），清空我们 saved request
                    if (mWebPermissionRequest != null && mWebPermissionRequest == request) {
                        mWebPermissionRequest = null;
                    }
                }

                /**
                 * 页面用 navigator.geolocation 时才申请定位权限：先显示顶部用途说明，再拉起系统授权，
                 * 结果回来后在权限回调里答复这个 origin。
                 */
                @Override
                public void onGeolocationPermissionsShowPrompt(String origin,
                                                               GeolocationPermissions.Callback callback) {
                    if (callback == null) {
                        return;
                    }
                    if (ContextCompat.checkSelfPermission(MainActivity.this, Manifest.permission.ACCESS_FINE_LOCATION)
                            == PackageManager.PERMISSION_GRANTED) {
                        callback.invoke(origin, true, false);
                        return;
                    }
                    mGeoOrigin = origin;
                    mGeoCallback = callback;
                    _showNoticeThen(Collections.singletonList("location"), () -> {
                        // 标记这是页面触发的单权限请求，回调里才能记拒绝状态并按结果答复定位
                        mPendingSinglePermission = Manifest.permission.ACCESS_FINE_LOCATION;
                        mPermissionRequestLauncher.launch(
                                new String[]{Manifest.permission.ACCESS_FINE_LOCATION});
                    });
                }


                /**
                 * H5 <video> 进入全屏：把内核给的自定义 View 铺满窗口。
                 */
                @Override
                public void onShowCustomView(View view, CustomViewCallback callback) {
                    if (!VIDEO_FULLSCREEN) {
                        super.onShowCustomView(view, callback);
                        return;
                    }
                    if (mCustomView != null) {
                        callback.onCustomViewHidden();
                        return;
                    }
                    mCustomView = view;
                    mCustomViewCallback = callback;
                    try {
                        FrameLayout decor = (FrameLayout) getWindow().getDecorView();
                        decor.addView(view, new FrameLayout.LayoutParams(
                                ViewGroup.LayoutParams.MATCH_PARENT,
                                ViewGroup.LayoutParams.MATCH_PARENT));
                        if (mWebView != null) {
                            mWebView.setVisibility(View.GONE);
                        }
                        // 视频全屏期间连状态栏 / 导航栏一起藏掉，退出时按启动配置还原
                        _applyVideoFullscreenBars(true);
                    } catch (Exception e) {
                        Log.e(TAG, "onShowCustomView error: " + e.getMessage(), e);
                        mCustomView = null;
                        mCustomViewCallback = null;
                        try { callback.onCustomViewHidden(); } catch (Exception ignored) { }
                    }
                }

                @Override
                public void onHideCustomView() {
                    if (mCustomView == null) {
                        return;
                    }
                    try {
                        FrameLayout decor = (FrameLayout) getWindow().getDecorView();
                        decor.removeView(mCustomView);
                    } catch (Exception e) {
                        Log.w(TAG, "onHideCustomView error: " + e.getMessage());
                    }
                    mCustomView = null;
                    if (mWebView != null) {
                        mWebView.setVisibility(View.VISIBLE);
                    }
                    _applyVideoFullscreenBars(false);
                    if (mCustomViewCallback != null) {
                        mCustomViewCallback.onCustomViewHidden();
                        mCustomViewCallback = null;
                    }
                }

                /**
                 * 控制台日志：在 logcat 里把 H5 的 console.log / warn / error 原样打出来。
                 * VIA 默认也输出到 logcat（开了 setWebContentsDebuggingEnabled 之后也能看）。
                 */
                @Override
                public boolean onConsoleMessage(ConsoleMessage cm) {
                    if (cm == null) return false;
                    String msg = cm.message() + " -- From line " + cm.lineNumber() + " of " + cm.sourceId();
                    switch (cm.messageLevel()) {
                        case ERROR: Log.e(TAG + "-js", msg); break;
                        case WARNING: Log.w(TAG + "-js", msg); break;
                        case TIP: Log.d(TAG + "-js", msg); break;
                        default: Log.i(TAG + "-js", msg); break;
                    }
                    return true;
                }

                /**
                 * H5 <input type="file" accept="image/*;capture=camera"> → 拉起系统文件选择 + 相机拍照。
                 * VIA 这里的实现几乎一样，核心：ValueCallback<Uri[]> + Intent.createChooser + 500ms 超时兜底。
                 */
                @Override
                public boolean onShowFileChooser(WebView webView, ValueCallback<Uri[]> filePathCallback,
                                                 FileChooserParams fileChooserParams) {
                    // 上一轮没回传的旧 callback 必须先塞 null（否则第二次选图内核直接崩）
                    if (mFilePathCallback != null) {
                        mFilePathCallback.onReceiveValue(null);
                        mFilePathCallback = null;
                    }
                    mFilePathCallback = filePathCallback;
                    // 上一轮拍照留下的临时文件不能复用，否则「取消拍照」会被当成拍到了旧照片
                    mCameraImageFile = null;

                    Intent captureIntent = null;
                    try {
                        // 如果声明了 capture=camera，优先把 ACTION_IMAGE_CAPTURE 作为 initial intent
                        boolean wantCamera = (fileChooserParams != null && fileChooserParams.isCaptureEnabled());
                        if (wantCamera) {
                            try {
                                captureIntent = new Intent(MediaStore.ACTION_IMAGE_CAPTURE);
                                if (captureIntent.resolveActivity(getPackageManager()) != null) {
                                    // 创建临时文件（getExternalFilesDir(PICTURES)/IMG_yyyyMMdd_HHmmss.jpg）
                                    mCameraImageFile = createImageFile();
                                    if (mCameraImageFile != null) {
                                        Uri photoUri = FileProvider.getUriForFile(
                                                MainActivity.this,
                                                getPackageName() + ".fileprovider",
                                                mCameraImageFile
                                        );
                                        captureIntent.putExtra(MediaStore.EXTRA_OUTPUT, photoUri);
                                        // 允许相机临时写入 FileProvider 共享的 Uri
                                        captureIntent.addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
                                    } else {
                                        captureIntent = null;
                                    }
                                }
                            } catch (Exception ex) {
                                captureIntent = null;
                                Log.w(TAG, "camera intent prepare failed, fallback to gallery only: " + ex.getMessage());
                            }
                        }

                        // 第二个 Intent：系统文件选择 / 相册
                        Intent content = new Intent(Intent.ACTION_GET_CONTENT);
                        content.addCategory(Intent.CATEGORY_OPENABLE);
                        content.setType("*/*");
                        if (fileChooserParams != null && fileChooserParams.getAcceptTypes() != null
                                && fileChooserParams.getAcceptTypes().length > 0) {
                            String[] accept = fileChooserParams.getAcceptTypes();
                            // 过滤掉空字符串 accept
                            List<String> good = new ArrayList<>();
                            for (String a : accept) if (a != null && !a.isEmpty()) good.add(a);
                            if (!good.isEmpty()) {
                                content.putExtra(Intent.EXTRA_MIME_TYPES, good.toArray(new String[0]));
                            }
                        }

                        Intent chooser = Intent.createChooser(content, getString(R.string.file_chooser_title));
                        if (captureIntent != null) {
                            chooser.putExtra(Intent.EXTRA_INITIAL_INTENTS, new Intent[]{ captureIntent });
                        }

                        // 回调必须留到 ActivityResult 回来为止：用户在选择器里停留通常远超 500ms，
                        // 提前置空会让真实结果被丢弃，H5 的 <input type=file> 永远等不到值（表现为「点了没反应」）。
                        mFileChooserLauncher.launch(chooser);
                        return true;
                    } catch (Exception e) {
                        Log.e(TAG, "onShowFileChooser error: " + e.getMessage(), e);
                        if (mFilePathCallback != null) {
                            mFilePathCallback.onReceiveValue(null);
                            mFilePathCallback = null;
                        }
                        return false;
                    }
                }
            });

            // 最后加载默认 URL：本地服务已启动时走 127.0.0.1，否则用打包时注入的地址。
            // 启用启动屏时先展示静态 HTML，到点后再切到真正的页面。
            String targetUrl = withStartQuery(mLocalServerUrl != null ? mLocalServerUrl : DEFAULT_URL);
            if (!Splash.loadInto(this, mWebView, targetUrl)) {
                mWebView.loadUrl(targetUrl);
            }
        } catch (Exception e) {
            Log.e(TAG, "initWebView fatal: " + e.getMessage(), e);
            throw new RuntimeException(e);
        }
    }

    // ========================================================================
    // 首次安装场景：运行时权限回调后，再把 pending 的 Web PermissionRequest grant/deny
    // ========================================================================
    private void _handlePendingWebPermissionRequest() {
        try {
            if (mWebPermissionRequest == null) {
                return;
            }
            PermissionRequest request = mWebPermissionRequest;
            String[] resources = request.getResources();
            List<String> grantedResources = new ArrayList<>();

            for (String r : resources) {
                if (PermissionRequest.RESOURCE_AUDIO_CAPTURE.equals(r)) {
                    if (ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO)
                            == PackageManager.PERMISSION_GRANTED) {
                        grantedResources.add(r);
                    }
                } else if (PermissionRequest.RESOURCE_VIDEO_CAPTURE.equals(r)) {
                    if (ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA)
                            == PackageManager.PERMISSION_GRANTED) {
                        grantedResources.add(r);
                    }
                } else if (PermissionRequest.RESOURCE_PROTECTED_MEDIA_ID.equals(r)
                        || PermissionRequest.RESOURCE_MIDI_SYSEX.equals(r)) {
                    grantedResources.add(r); // 不需要额外 Android 权限
                }
            }

            if (!grantedResources.isEmpty()) {
                request.grant(grantedResources.toArray(new String[0]));
            } else {
                request.deny();
            }
        } catch (Exception e) {
            Log.w(TAG, "_handlePendingWebPermissionRequest skipped: " + e.getMessage());
            try { if (mWebPermissionRequest != null) mWebPermissionRequest.deny(); } catch (Exception ignored) { /* noop */ }
        } finally {
            mWebPermissionRequest = null;
        }
    }

    // ========================================================================
    // 下载（系统 DownloadManager + 页面回传保存）
    // ========================================================================

    /** 从 URL / Content-Disposition / MIME 猜一个安全的文件名。 */
    private String _guessDownloadName(String url, String contentDisposition, String mimetype) {
        String name = "";
        try {
            if (contentDisposition != null) {
                java.util.regex.Matcher matcher = java.util.regex.Pattern
                        .compile("filename\\*?=(?:UTF-8'')?\"?([^\";]+)\"?",
                                java.util.regex.Pattern.CASE_INSENSITIVE)
                        .matcher(contentDisposition);
                if (matcher.find()) {
                    name = Uri.decode(matcher.group(1).trim());
                }
            }
            // blob: / data: 的「最后一段」是内核生成的随机 id，不是文件名，直接跳过
            String scheme = Uri.parse(url).getScheme();
            if (name.isEmpty() && ("http".equals(scheme) || "https".equals(scheme))) {
                String last = Uri.parse(url).getLastPathSegment();
                if (last != null && !last.isEmpty()) {
                    name = Uri.decode(last);
                }
            }
        } catch (Exception e) {
            Log.w(TAG, "guess download name failed: " + e.getMessage());
        }
        name = name.replaceAll("[\\\\/:*?\"<>|\\r\\n]", "_").trim();
        // 拿不到名字（页面生成的 blob、无文件名的接口等）按时间戳命名
        if (name.isEmpty()) {
            name = "JingLing_"
                    + new SimpleDateFormat("yyyy_dd_MM_ss", Locale.US).format(new Date());
        }
        if (!name.contains(".")) {
            name = name + _extensionForMime(mimetype);
        }
        return name;
    }

    private String _extensionForMime(String mimetype) {
        if (mimetype == null) {
            return ".bin";
        }
        switch (mimetype.toLowerCase(Locale.US)) {
            case "text/plain": return ".txt";
            case "text/html": return ".html";
            case "application/pdf": return ".pdf";
            case "application/json": return ".json";
            case "image/jpeg": return ".jpg";
            case "image/png": return ".png";
            case "image/webp": return ".webp";
            case "audio/mpeg": return ".mp3";
            case "video/mp4": return ".mp4";
            case "application/zip": return ".zip";
            default:
                int slash = mimetype.indexOf('/');
                if (slash > 0 && slash + 1 < mimetype.length()) {
                    return "." + mimetype.substring(slash + 1).replaceAll("[^a-zA-Z0-9]", "");
                }
                return ".bin";
        }
    }

    /** appassets / blob / data 这类 URL 系统下载器取不到，只能让页面自己读内容。 */
    private boolean _isPageOnlyDownloadUrl(String url) {
        try {
            Uri uri = Uri.parse(url);
            String scheme = uri.getScheme();
            if (scheme == null) {
                return false;
            }
            if (!"http".equals(scheme) && !"https".equals(scheme)) {
                return true;
            }
            return "appassets.androidplatform.net".equals(uri.getHost());
        } catch (Exception e) {
            return true;
        }
    }

    /** 交给系统 DownloadManager：进度通知 + 保存到公共「下载」目录。 */
    private boolean _enqueueSystemDownload(String url, String fileName, String userAgent,
                                           String mimetype) {
        if (_isPageOnlyDownloadUrl(url)) {
            return false;
        }
        try {
            DownloadManager manager = (DownloadManager) getSystemService(DOWNLOAD_SERVICE);
            if (manager == null) {
                return false;
            }
            DownloadManager.Request request = new DownloadManager.Request(Uri.parse(url));
            if (mimetype != null && !mimetype.isEmpty()) {
                request.setMimeType(mimetype);
            }
            if (userAgent != null && !userAgent.isEmpty()) {
                request.addRequestHeader("User-Agent", userAgent);
            }
            String cookie = CookieManager.getInstance().getCookie(url);
            if (cookie != null && !cookie.isEmpty()) {
                request.addRequestHeader("Cookie", cookie);
            }
            request.setTitle(fileName);
            request.setDescription("正在下载");
            request.allowScanningByMediaScanner();
            request.setNotificationVisibility(
                    DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED);
            request.setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, fileName);
            long id = manager.enqueue(request);
            Log.i(TAG, "download enqueued: id=" + id + " name=" + fileName);
            return true;
        } catch (Exception e) {
            Log.w(TAG, "system download failed, fallback to page: " + e.getMessage());
            return false;
        }
    }

    /** 让页面 fetch 出内容（base64）回传给壳保存，覆盖 blob / data / assets 等情况。 */
    private void _requestPageSaveDownload(String url, String fileName, String mimetype,
                                          long contentLength) {
        if (mWebView == null) {
            return;
        }
        if (contentLength > 64L * 1024 * 1024) {
            Log.w(TAG, "download too large for page fallback: " + contentLength);
            Toast.makeText(this, "文件过大，暂不支持该下载", Toast.LENGTH_SHORT).show();
            return;
        }
        String js = "(function(){var u=" + JSONObject.quote(url)
                + ",n=" + JSONObject.quote(fileName)
                + ",m=" + JSONObject.quote(mimetype == null ? "" : mimetype) + ";"
                + "fetch(u).then(function(r){return r.blob();}).then(function(b){"
                + "var fr=new FileReader();fr.onload=function(){var s=String(fr.result||'');"
                + "var i=s.indexOf(',');JianLing.saveDownload(n,m,i>=0?s.substring(i+1):'');};"
                + "fr.readAsDataURL(b);})"
                + ".catch(function(e){console.error('saveDownload failed: '+e);});})();";
        mWebView.evaluateJavascript(js, null);
    }

    /** Android 13+ 下载通知需要 POST_NOTIFICATIONS；打包时勾了通知权限才申请。 */
    private void _ensureNotificationPermission() {
        try {
            if (Build.VERSION.SDK_INT < 33) {
                return;
            }
            if (ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS)
                    == PackageManager.PERMISSION_GRANTED) {
                return;
            }
            if (!_isPermissionDeclared(Manifest.permission.POST_NOTIFICATIONS)) {
                Log.i(TAG, "POST_NOTIFICATIONS not packaged, download notification will be silent");
                return;
            }
            if (mNotificationPermissionLauncher != null) {
                mNotificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS);
            }
        } catch (Exception e) {
            Log.w(TAG, "ensure notification permission failed: " + e.getMessage());
        }
    }

    private boolean _isPermissionDeclared(String permission) {
        try {
            PackageInfo info = getPackageManager().getPackageInfo(getPackageName(),
                    PackageManager.GET_PERMISSIONS);
            if (info.requestedPermissions != null) {
                for (String declared : info.requestedPermissions) {
                    if (permission.equals(declared)) {
                        return true;
                    }
                }
            }
        } catch (Exception e) {
            Log.w(TAG, "isPermissionDeclared failed: " + e.getMessage());
        }
        return false;
    }

    /** 把页面回传的 base64 写成文件：Android 10+ 走 MediaStore.Downloads（无需存储权限）。 */
    private String _saveBase64ToDownloads(String fileName, String mimetype, String base64) {
        try {
            byte[] bytes = Base64.decode(base64 == null ? "" : base64, Base64.DEFAULT);
            String name = (fileName == null || fileName.trim().isEmpty())
                    ? "download" : fileName.trim();
            String mime = (mimetype == null || mimetype.trim().isEmpty())
                    ? "application/octet-stream" : mimetype.trim();
            File target;
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                ContentValues values = new ContentValues();
                values.put(MediaStore.Downloads.DISPLAY_NAME, name);
                values.put(MediaStore.Downloads.MIME_TYPE, mime);
                values.put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS);
                values.put(MediaStore.Downloads.IS_PENDING, 1);
                Uri item = getContentResolver()
                        .insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values);
                if (item == null) {
                    return "";
                }
                try (OutputStream out = getContentResolver().openOutputStream(item)) {
                    if (out != null) {
                        out.write(bytes);
                    }
                }
                ContentValues done = new ContentValues();
                done.put(MediaStore.Downloads.IS_PENDING, 0);
                getContentResolver().update(item, done, null, null);
                target = new File(Environment.getExternalStoragePublicDirectory(
                        Environment.DIRECTORY_DOWNLOADS), name);
            } else {
                File dir = Environment.getExternalStoragePublicDirectory(
                        Environment.DIRECTORY_DOWNLOADS);
                if (!dir.exists() && !dir.mkdirs()) {
                    return "";
                }
                target = new File(dir, name);
                try (FileOutputStream out = new FileOutputStream(target)) {
                    out.write(bytes);
                }
                MediaScannerConnection.scanFile(this,
                        new String[]{target.getAbsolutePath()}, new String[]{mime}, null);
            }
            _notifyDownloadDone(name, bytes.length);
            Log.i(TAG, "download saved: " + target.getAbsolutePath() + " (" + bytes.length + "B)");
            return target.getAbsolutePath();
        } catch (Exception e) {
            Log.e(TAG, "save download failed: " + e.getMessage());
            return "";
        }
    }

    private void _notifyDownloadDone(String name, int size) {
        try {
            NotificationManager manager =
                    (NotificationManager) getSystemService(NOTIFICATION_SERVICE);
            if (manager == null) {
                return;
            }
            String channelId = "jianling-downloads";
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                NotificationChannel channel = new NotificationChannel(
                        channelId, "下载", NotificationManager.IMPORTANCE_DEFAULT);
                manager.createNotificationChannel(channel);
            }
            Notification.Builder builder = Build.VERSION.SDK_INT >= Build.VERSION_CODES.O
                    ? new Notification.Builder(this, channelId)
                    : new Notification.Builder(this);
            builder.setContentTitle("下载完成")
                    .setContentText(name + " 已保存到「下载」目录（" + size + " 字节）")
                    .setSmallIcon(android.R.drawable.stat_sys_download_done)
                    .setAutoCancel(true);
            manager.notify((int) (System.currentTimeMillis() % 100000), builder.build());
        } catch (Exception e) {
            Log.w(TAG, "notify download failed: " + e.getMessage());
        }
    }

    // ========================================================================
    // 小工具（containsResource / createImageFile / 磁盘清理）
    // ========================================================================
    /**
     * 判断 PermissionRequest 资源集合里是否包含某类资源（RESOURCE_AUDIO_CAPTURE 等）
     */
    private static boolean _containsResource(@NonNull String[] resources, @NonNull String target) {
        for (String r : resources) {
            if (target.equals(r)) return true;
        }
        return false;
    }

    /**
     * 创建拍照临时文件（JPG），存在 getExternalFilesDir(PICTURES) 下（FileProvider 已配置授权）。
     */
    @Nullable
    private File createImageFile() throws IOException {
        try {
            String timeStamp = new SimpleDateFormat("yyyyMMdd_HHmmss", Locale.US).format(new Date());
            String imageFileName = "IMG_" + timeStamp + "_";
            File dir = getExternalFilesDir(Environment.DIRECTORY_PICTURES);
            if (dir == null) {
                return null;
            }
            if (!dir.exists() && !dir.mkdirs()) {
                return null;
            }
            return File.createTempFile(imageFileName, ".jpg", dir);
        } catch (Exception e) {
            Log.w(TAG, "createImageFile skipped: " + e.getMessage());
            return null;
        }
    }

    /**
     * 启动时清理 7 天以上未访问的临时拍照文件（files/Pictures/IMG_*.jpg）。
     * 不是 VIA 必须，但磁盘卫生可以长期保持 APP 大小稳定，避免用户体感"越用越大=卡"。
     */
    private void _cleanupOldCameraTempFiles() {
        try {
            File dir = getExternalFilesDir(Environment.DIRECTORY_PICTURES);
            if (dir == null || !dir.exists()) {
                return;
            }
            File[] files = dir.listFiles();
            if (files == null || files.length == 0) {
                return;
            }
            long deadline = System.currentTimeMillis() - 7L * 24L * 60L * 60L * 1000L;
            int deleted = 0;
            for (File f : files) {
                if (f == null || !f.isFile()) continue;
                String name = f.getName();
                if (!name.startsWith("IMG_") || !name.toLowerCase(Locale.ROOT).endsWith(".jpg")) continue;
                if (f.lastModified() > deadline) continue;
                if (f.delete()) deleted++;
            }
            if (deleted > 0) {
                Log.i(TAG, "_cleanupOldCameraTempFiles: deleted " + deleted + " old jpg(s)");
            }
        } catch (Exception e) {
            Log.w(TAG, "_cleanupOldCameraTempFiles skipped: " + e.getMessage());
        }
    }

    // ========================================================================
    // 返回键 / 生命周期 / 销毁链
    // ========================================================================
    @Override
    public boolean onKeyDown(int keyCode, KeyEvent event) {
        try {
            // 视频全屏时返回键只退出全屏：不能落到下面的网页后退 / 再按一次退出
            if (keyCode == KeyEvent.KEYCODE_BACK && mCustomView != null) {
                _exitVideoFullscreen();
                return true;
            }
            if (keyCode == KeyEvent.KEYCODE_BACK && mWebView != null && mWebView.canGoBack()) {
                mWebView.goBack();
                return true;
            }
            if (keyCode == KeyEvent.KEYCODE_BACK && DISABLE_BACK_EXIT) {
                // 已经无处可退，按配置把返回键吃掉
                return true;
            }
            if (keyCode == KeyEvent.KEYCODE_BACK && EXIT_ON_BACK && mWebView != null) {
                long now = System.currentTimeMillis();
                if (now - mLastBackPressedAt < 2000L) {
                    finish();
                    return true;
                }
                mLastBackPressedAt = now;
                Toast.makeText(this, "再按一次退出", Toast.LENGTH_SHORT).show();
                return true;
            }
        } catch (Exception e) {
            Log.w(TAG, "onKeyDown error: " + e.getMessage());
        }
        return super.onKeyDown(keyCode, event);
    }

    /**
     * 退出视频全屏：先让内核收掉全屏（它会回调 onHideCustomView 做清理），
     * 内核没回调时自己兜底移除，避免全屏黑层一直挡着页面。
     */
    private void _exitVideoFullscreen() {
        WebChromeClient.CustomViewCallback callback = mCustomViewCallback;
        if (callback != null) {
            try {
                callback.onCustomViewHidden();
            } catch (Exception e) {
                Log.w(TAG, "exit video fullscreen callback error: " + e.getMessage());
            }
        }
        if (mCustomView != null) {
            View view = mCustomView;
            mCustomView = null;
            mCustomViewCallback = null;
            try {
                Object parent = view.getParent();
                if (parent instanceof ViewGroup) {
                    ((ViewGroup) parent).removeView(view);
                }
            } catch (Exception e) {
                Log.w(TAG, "exit video fullscreen remove error: " + e.getMessage());
            }
            if (mWebView != null) {
                mWebView.setVisibility(View.VISIBLE);
            }
            _applyVideoFullscreenBars(false);
        }
    }

    /**
     * 视频全屏的系统栏：进入时状态栏 + 导航栏都隐藏（上滑临时呼出），
     * 退出时恢复成启动配置的样子（全屏启动仍隐藏，只隐藏导航栏的开关仍只隐藏导航栏）。
     */
    private void _applyVideoFullscreenBars(boolean fullscreen) {
        try {
            WindowInsetsControllerCompat controller =
                    WindowCompat.getInsetsController(getWindow(), getWindow().getDecorView());
            if (controller == null) {
                return;
            }
            if (fullscreen) {
                controller.hide(WindowInsetsCompat.Type.systemBars());
                controller.setSystemBarsBehavior(
                        WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                return;
            }
            if (FULLSCREEN) {
                controller.hide(WindowInsetsCompat.Type.systemBars());
                controller.setSystemBarsBehavior(
                        WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                return;
            }
            controller.show(WindowInsetsCompat.Type.statusBars());
            if (HIDE_NAV_BAR) {
                controller.hide(WindowInsetsCompat.Type.navigationBars());
                controller.setSystemBarsBehavior(
                        WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            } else {
                controller.show(WindowInsetsCompat.Type.navigationBars());
            }
        } catch (Exception e) {
            Log.w(TAG, "applyVideoFullscreenBars error: " + e.getMessage());
        }
    }

    // ========================================================================
    // 进度条 / 错误页 / 来源校验 / 原生桥
    // ========================================================================
    private void showErrorView() {
        if (mPageProgress != null) {
            mPageProgress.setVisibility(View.GONE);
        }
        if (mSwipeRefresh != null && mSwipeRefresh.isRefreshing()) {
            mSwipeRefresh.setRefreshing(false);
        }
        if (mErrorView != null) {
            mErrorView.setVisibility(View.VISIBLE);
        }
    }

    private void hideErrorView() {
        if (mErrorView != null) {
            mErrorView.setVisibility(View.GONE);
        }
    }

    /** 缓存模式配置 → WebSettings 常量 */
    private static int cacheModeValue() {
        if ("no_cache".equals(CACHE_MODE)) {
            return WebSettings.LOAD_NO_CACHE;
        }
        if ("cache_first".equals(CACHE_MODE)) {
            return WebSettings.LOAD_CACHE_ELSE_NETWORK;
        }
        return WebSettings.LOAD_DEFAULT;
    }

    /**
     * 是否为应用自己的页面：本地服务 / 资源加载器 / 配置的首页域名。
     * 原生桥只对这些来源生效，避免第三方页面拿到壳能力。
     */
    private boolean isTrustedUrl(String url) {
        if (url == null || url.isEmpty()) {
            return false;
        }
        try {
            Uri uri = Uri.parse(url);
            String host = uri.getHost();
            if (host == null) {
                return false;
            }
            if ("127.0.0.1".equals(host) || "localhost".equals(host)
                    || "appassets.androidplatform.net".equals(host)) {
                return true;
            }
            String baseHost = Uri.parse(DEFAULT_URL).getHost();
            return baseHost != null && baseHost.equalsIgnoreCase(host);
        } catch (Exception e) {
            return false;
        }
    }

    private boolean isTrustedHost(Uri uri) {
        return uri != null && isTrustedUrl(uri.toString());
    }

    /**
     * 静态资源加载：优先读外部推送目录（files/www），没有该文件再回退 APK 内 assets。
     * 用于关闭本地服务时（WebViewAssetLoader 走 appassets.androidplatform.net）的调试热推。
     */
    private static final class PushedAssetsHandler implements WebViewAssetLoader.PathHandler {
        private final File root;
        private final WebViewAssetLoader.PathHandler fallback;

        PushedAssetsHandler(File root, WebViewAssetLoader.PathHandler fallback) {
            this.root = root;
            this.fallback = fallback;
        }

        @Override
        public WebResourceResponse handle(String path) {
            if (root != null) {
                try {
                    File file = new File(root, path);
                    String base = root.getCanonicalPath();
                    String full = file.getCanonicalPath();
                    if (full.startsWith(base + File.separator) && file.isFile()) {
                        String mime = URLConnection.guessContentTypeFromName(path);
                        if (mime == null) {
                            mime = "application/octet-stream";
                        }
                        String encoding = mime.startsWith("text/")
                                || mime.contains("javascript")
                                || mime.contains("json") ? "UTF-8" : null;
                        return new WebResourceResponse(mime, encoding, new FileInputStream(file));
                    }
                } catch (Exception ignored) {
                    // 读不到就走 assets
                }
            }
            return fallback.handle(path);
        }
    }

    /**
     * H5 可用的原生能力（window.JianLing.xxx）。
     * 每个方法都校验当前页面来源，非本站页面调用直接忽略。
     */
    public class JianLingBridge {
        private boolean trusted() {
            // 桥方法在后台线程回调，不能调 WebView.getUrl()，用 UI 线程缓存的地址
            return isTrustedUrl(mCurrentPageUrl);
        }

        @JavascriptInterface
        public String getVersion() {
            if (!trusted()) {
                return "";
            }
            try {
                return getPackageManager().getPackageInfo(getPackageName(), 0).versionName;
            } catch (Exception e) {
                return "";
            }
        }

        @JavascriptInterface
        public String saveDownload(String fileName, String mimeType, String base64) {
            if (!trusted()) {
                return "";
            }
            return _saveBase64ToDownloads(fileName, mimeType, base64);
        }

        @JavascriptInterface
        public String getDeviceInfo() {
            if (!trusted()) {
                return "{}";
            }
            JSONObject info = new JSONObject();
            try {
                info.put("packageName", getPackageName());
                info.put("versionName", getPackageManager().getPackageInfo(getPackageName(), 0).versionName);
                info.put("brand", Build.BRAND);
                info.put("model", Build.MODEL);
                info.put("manufacturer", Build.MANUFACTURER);
                info.put("sdkInt", Build.VERSION.SDK_INT);
                info.put("androidVersion", Build.VERSION.RELEASE);
                try {
                    android.content.pm.PackageInfo webview =
                            WebViewCompat.getCurrentWebViewPackage(MainActivity.this);
                    if (webview != null) {
                        info.put("webViewPackage", webview.packageName);
                        info.put("webViewVersion", webview.versionName);
                    }
                } catch (Exception ignored) {
                    /* 旧系统拿不到就不填 */
                }
            } catch (Exception e) {
                Log.w(TAG, "getDeviceInfo failed: " + e.getMessage());
            }
            return info.toString();
        }

        @JavascriptInterface
        public void toast(final String message) {
            if (!trusted() || message == null) {
                return;
            }
            runOnUiThread(() -> Toast.makeText(MainActivity.this, message, Toast.LENGTH_SHORT).show());
        }

        @JavascriptInterface
        public void vibrate(final int milliseconds) {
            if (!trusted()) {
                return;
            }
            try {
                android.os.Vibrator vibrator =
                        (android.os.Vibrator) getSystemService(VIBRATOR_SERVICE);
                if (vibrator != null) {
                    vibrator.vibrate(Math.max(1, Math.min(milliseconds, 1000)));
                }
            } catch (Exception e) {
                Log.w(TAG, "vibrate failed: " + e.getMessage());
            }
        }

        @JavascriptInterface
        public boolean openExternal(String url) {
            if (!trusted() || url == null || url.isEmpty()) {
                return false;
            }
            try {
                startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
                return true;
            } catch (Exception e) {
                Log.w(TAG, "openExternal failed: " + e.getMessage());
                return false;
            }
        }

        @JavascriptInterface
        public boolean copyText(String text) {
            if (!trusted() || text == null) {
                return false;
            }
            try {
                android.content.ClipboardManager manager =
                        (android.content.ClipboardManager) getSystemService(CLIPBOARD_SERVICE);
                if (manager != null) {
                    manager.setPrimaryClip(android.content.ClipData.newPlainText("text", text));
                    return true;
                }
            } catch (Exception e) {
                Log.w(TAG, "copyText failed: " + e.getMessage());
            }
            return false;
        }

        @JavascriptInterface
        public void clearCache() {
            if (!trusted()) {
                return;
            }
            runOnUiThread(() -> {
                try {
                    if (mWebView != null) {
                        mWebView.clearCache(true);
                        mWebView.clearFormData();
                    }
                    CookieManager.getInstance().flush();
                } catch (Exception e) {
                    Log.w(TAG, "clearCache failed: " + e.getMessage());
                }
            });
        }

        @JavascriptInterface
        public void exitApp() {
            if (!trusted()) {
                return;
            }
            runOnUiThread(MainActivity.this::finish);
        }

        /**
         * 页面主动申请一项运行时权限（先显示顶部用途说明，再拉起系统授权）。
         * key：camera | microphone | location | notification
         *
         * 返回值：requested（已拉起系统授权）| granted（原本就授予，无需弹框）|
         *         unsupported（不认识的 key 或页面来源不可信）
         * WebView 不支持 Notification API，通知权限只能通过这个入口申请。
         */
        @JavascriptInterface
        public String requestPermission(String key) {
            if (!trusted() || key == null) {
                return "unsupported";
            }
            final String permission;
            switch (key) {
                case "camera":
                    permission = Manifest.permission.CAMERA;
                    break;
                case "microphone":
                    permission = Manifest.permission.RECORD_AUDIO;
                    break;
                case "location":
                    permission = Manifest.permission.ACCESS_FINE_LOCATION;
                    break;
                case "notification":
                    permission = Manifest.permission.POST_NOTIFICATIONS;
                    break;
                default:
                    return "unsupported";
            }
            if (Manifest.permission.POST_NOTIFICATIONS.equals(permission)
                    && Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
                // Android 13 以下没有通知运行时权限，安装即授予
                return "granted";
            }
            if (ContextCompat.checkSelfPermission(MainActivity.this, permission)
                    == PackageManager.PERMISSION_GRANTED) {
                return "granted";
            }
            runOnUiThread(() -> {
                mPendingSinglePermission = permission;
                _showNoticeThen(Collections.singletonList(key),
                        () -> mPermissionRequestLauncher.launch(new String[]{permission}));
            });
            return "requested";
        }
    }

    @Override
    protected void onResume() {
        super.onResume();
        try {
            if (mWebView != null) {
                mWebView.onResume();
            }
            setupFullScreen();
        } catch (Exception e) {
            Log.e(TAG, "onResume error: " + e.getMessage(), e);
        }
    }

    @Override
    protected void onPause() {
        try {
            if (mWebView != null) {
                mWebView.onPause();
            }
        } catch (Exception e) {
            Log.w(TAG, "onPause webview error: " + e.getMessage());
        }
        super.onPause();
    }

    @Override
    protected void onDestroy() {
        // 规范的 WebView 销毁链 + 悬挂 callback 清理
        try {
            // 1) 兜底 pending FileChooser callback
            if (mFilePathCallback != null) {
                try {
                    mFilePathCallback.onReceiveValue(null);
                } catch (Exception e) {
                    Log.w(TAG, "onDestroy filePathCallback fallback failed: " + e.getMessage());
                }
                mFilePathCallback = null;
            }
            // 2) 兜底 pending Web permission request（直接 deny 释放内核端回调桥）
            if (mWebPermissionRequest != null) {
                try {
                    mWebPermissionRequest.deny();
                } catch (Exception e) {
                    Log.w(TAG, "onDestroy permission deny failed: " + e.getMessage());
                }
                mWebPermissionRequest = null;
            }
            // 3) 清空主线程 Handler 所有未执行的消息（FileChooser 500ms 兜底等）
            try {
                mMainHandler.removeCallbacksAndMessages(null);
            } catch (Exception e) {
                Log.w(TAG, "onDestroy clear handler failed: " + e.getMessage());
            }

            // 3.1) 收掉视频全屏的自定义 View
            if (mCustomView != null) {
                try {
                    Object parent = mCustomView.getParent();
                    if (parent instanceof ViewGroup) {
                        ((ViewGroup) parent).removeView(mCustomView);
                    }
                } catch (Exception e) {
                    Log.w(TAG, "onDestroy remove customView failed: " + e.getMessage());
                }
                mCustomView = null;
                mCustomViewCallback = null;
            }

            // 4) WebView 销毁链（VIA 同款顺序）：removeView first → stopLoading → clients=null → removeAllViews → destroy
            if (mWebView != null) {
                try {
                    Object parent = mWebView.getParent();
                    if (parent instanceof ViewGroup) {
                        ((ViewGroup) parent).removeView(mWebView);
                    }
                } catch (Exception e) {
                    Log.w(TAG, "onDestroy remove webview from parent failed: " + e.getMessage());
                }
                try { mWebView.stopLoading(); } catch (Exception e) { Log.w(TAG, "stopLoading failed: " + e.getMessage()); }
                try { mWebView.setWebChromeClient(null); } catch (Exception e) { Log.w(TAG, "setWebChromeClient null failed: " + e.getMessage()); }
                try { mWebView.setWebViewClient(null); } catch (Exception e) { Log.w(TAG, "setWebViewClient null failed: " + e.getMessage()); }
                try { mWebView.removeAllViews(); } catch (Exception e) { Log.w(TAG, "removeAllViews failed: " + e.getMessage()); }
                try { mWebView.destroy(); } catch (Exception e) { Log.w(TAG, "webview destroy failed: " + e.getMessage()); }
                mWebView = null;
            }
        } catch (Exception e) {
            Log.w(TAG, "onDestroy cleanup wrapper failed: " + e.getMessage());
        } finally {
            // 关闭本地服务（先停监听，再关 WebView 已在上面的销毁链里完成）
            if (mLocalServer != null) {
                try {
                    mLocalServer.stop();
                } catch (Exception e) {
                    Log.w(TAG, "onDestroy localServer stop failed: " + e.getMessage());
                }
                mLocalServer = null;
            }
            try { super.onDestroy(); } catch (Exception ignored) { /* noop */ }
        }
    }
}