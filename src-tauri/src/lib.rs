// shell.json 的字段较多，json! 宏展开需要更高递归上限
#![recursion_limit = "256"]

mod adb;
mod cdp;
mod config;
mod env_check;
mod installer;
mod kernel;
mod pipeline;
mod secret;
mod winres;
mod watermark;

use env_check::EnvReport;

#[tauri::command]
fn check_environment(jdk_path: Option<String>, sdk_path: Option<String>) -> EnvReport {
    env_check::check(jdk_path, sdk_path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            check_environment,
            config::load_settings,
            config::save_settings,
            config::read_build_log,
            config::traffic_log_dir,
            config::delete_build_files,
            pipeline::build_android,
            pipeline::build_windows,
            pipeline::build_macos,
            pipeline::build_linux,
            pipeline::build_harmony,
            pipeline::build_ios,
            kernel::fetch_windows_kernel_versions,
            kernel::download_windows_kernel,
            kernel::import_windows_kernel,
            kernel::delete_windows_kernel,
            kernel::windows_kernel_dir,
            installer::install_jdk_command,
            installer::install_sdk_command,
            adb::adb_devices,
            adb::adb_webview_version,
            adb::adb_logcat_tail,
            adb::adb_logcat_clear,
            adb::adb_install_apk,
            adb::adb_launch_app,
            adb::adb_push_www,
            adb::copy_to_clipboard,
            adb::adb_keep_screen,
            adb::adb_installed_packages,
            adb::adb_pull_logs,
            cdp::devtools_connect,
            cdp::devtools_disconnect
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
