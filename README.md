# 茧灵（JianLing）

裹网于壳，藏灵于形。

茧灵是一个把 H5 页面封装成 Android 应用与桌面程序的图形化工具。选择本地静态包或远程网址，配置权限、转发规则与启动参数，一键产出 APK、安装包与绿色版，并内置设备调试能力。

## 功能特性

### 应用打包

- Android：内容来源支持本地静态包与远程网址；可配置包名、版本、图标、权限、状态栏、启动屏、下拉刷新、顶部操作栏、缓存策略等。
- Windows：支持单文件 exe、安装包与绿色版 zip 三种产物。
- macOS：产出 .app 与 zip，窗口、启动屏、本地服务与 Windows 共用一套配置。
- Linux：产出可执行文件目录（含 run.sh 与 .desktop）与 zip。
- 鸿蒙 NEXT：生成 DevEco Studio 工程，HAP 在 DevEco 里构建。
- iOS：生成 Swift + WKWebView 的 Xcode 工程，签名与 ipa 导出在 Xcode 里完成。
- 调试包与正式包可同时安装（调试包包名带 `.debug` 后缀）。

### 内置本地服务与接口转发

- 页面以 `http://127.0.0.1` 同源托管，`/api`、`/ws` 等相对路径由本地服务转发到后端，前端无需绑定域名，后端无需配置跨域。
- 转发规则按前缀匹配（最长优先），支持 http(s) 与 ws(s)、Origin 改写、自签证书、固定 IP 解析、追加请求头。
- 运行期可把前端构建产物推送到应用外部目录，免去每次改动都重新打包。

### 页面能力

- 页面脚本执行前注入 `window.JianLing`，可读取壳版本与设备信息，也可据此判断当前是否运行在壳内。
- 支持文件选择、下载转存、权限申请提示、状态栏高度 CSS 变量等原生能力。

### 调试工具

- 设备列表与安装启动、System WebView 版本查看、屏幕常亮。
- 一键连接完整 DevTools（复制 `devtools://` 地址），支持 Console / Network / Sources 直达。
- 实时查看设备日志，支持关键字过滤与自动刷新。
- 应用内自动检测新版本。

## 平台支持

| 平台 | 状态 | 说明 |
| --- | --- | --- |
| Android | 已实测 | 模板 + Gradle 构建 APK |
| Windows | 已实测 | shell-host + WebView2 |
| macOS | 未实测 | 需在 macOS 上编译 `shell-host-macos` |
| Linux | 未实测 | 需在 Linux 上编译 `shell-host-linux`，依赖 WebKitGTK |
| 鸿蒙 NEXT | 未实测 | 生成 DevEco Studio 工程，HAP 在 DevEco 里构建 |
| iOS | 未实测 | 生成 Xcode 工程（XcodeGen 描述），签名与 ipa 导出在 Xcode 里完成 |

macOS、Linux、鸿蒙、iOS 没有在对应系统上验证过。桌面平台共用 shell-host，先在目标系统编译好放到 `src-tauri/binaries/`：Windows 用 `npm run sync-host`；macOS、Linux 编译后命名为 `shell-host-macos`、`shell-host-linux`。iOS 不吃 shell-host，工程里的 Swift 代码直接加载本地包或远程地址。

## 界面预览

### 项目管理

集中管理多个套壳项目，包名、版本与最近构建时间一目了然。

![项目管理](截图/01-项目列表.png)

### 套壳配置

应用基础信息与 Android / Windows 平台配置分区展示，按需切换。

![套壳配置](截图/02-项目配置.png)

### 返回键策略

返回键行为收敛为一个下拉，四档策略自带说明，一般保持「自动」即可。

![返回键策略](截图/04-返回键策略.png)

### 构建历史与一键安装

每次构建保留产物路径与签名状态，可直接把 APK 安装到当前连接的 Android 设备。

![构建历史](截图/05-构建历史.png)

## 许可证

本项目基于 GNU Affero General Public License v3.0 发布，详见 [LICENSE](LICENSE)。
