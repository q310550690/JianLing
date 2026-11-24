use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::AppHandle;

use crate::adb;

static FORWARDS: OnceLock<Mutex<HashMap<String, u16>>> = OnceLock::new();

fn forwards() -> &'static Mutex<HashMap<String, u16>> {
    FORWARDS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevtoolsInfo {
    pub port: u16,
    pub target_id: String,
    pub title: String,
    pub url: String,
    /// 直接粘到浏览器地址栏即可打开的完整 DevTools 地址
    pub devtools_url: String,
}

fn app_pid(adb_path: &str, serial: &str, package: &str) -> Result<String, String> {
    let out = adb::run(adb_path, &["-s", serial, "shell", "pidof", package])?;
    out.split_whitespace()
        .next()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("应用未在运行（{package}），先启动它再连调试器"))
}

fn devtools_socket(adb_path: &str, serial: &str, pid: &str) -> Result<String, String> {
    let out = adb::run(adb_path, &["-s", serial, "shell", "cat", "/proc/net/unix"])?;
    let want = format!("webview_devtools_remote_{pid}");
    let mut fallback: Option<String> = None;
    for line in out.lines() {
        let Some(pos) = line.find("webview_devtools_remote") else {
            continue;
        };
        let name = &line[pos..];
        let name = name.split_whitespace().next().unwrap_or(name);
        if name == want {
            return Ok(name.to_string());
        }
        if fallback.is_none() {
            fallback = Some(name.to_string());
        }
    }
    fallback.ok_or_else(|| "设备上没有可调试的 WebView（调试开关没开？）".to_string())
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("分配本地端口失败：{e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .port();
    drop(listener);
    Ok(port)
}

#[tauri::command]
pub fn devtools_connect(
    app: AppHandle,
    serial: String,
    package: String,
) -> Result<DevtoolsInfo, String> {
    let adb_path = adb::adb_for(&app)?;
    let _ = devtools_disconnect(app.clone(), serial.clone());

    let pid = app_pid(&adb_path, &serial, &package)?;
    let socket = devtools_socket(&adb_path, &serial, &pid)?;
    let port = free_port()?;
    adb::run(
        &adb_path,
        &[
            "-s",
            &serial,
            "forward",
            &format!("tcp:{port}"),
            &format!("localabstract:{socket}"),
        ],
    )?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let text = client
        .get(format!("http://127.0.0.1:{port}/json"))
        .send()
        .and_then(|r| r.text())
        .map_err(|e| {
            let _ = adb::run(
                &adb_path,
                &["-s", &serial, "forward", "--remove", &format!("tcp:{port}")],
            );
            format!("读取调试目标失败：{e}")
        })?;
    let list: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("调试目标解析失败：{e}"))?;
    let page = list
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|t| t.get("webSocketDebuggerUrl").is_some())
                .cloned()
        })
        .ok_or("设备上没有可调试的页面")?;

    let target_id = page
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let title = page
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let url = page
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let devtools_url = format!(
        "devtools://devtools/bundled/inspector.html?ws=127.0.0.1:{port}/devtools/page/{target_id}"
    );

    forwards()
        .lock()
        .map_err(|e| e.to_string())?
        .insert(serial, port);

    Ok(DevtoolsInfo {
        port,
        target_id,
        title,
        url,
        devtools_url,
    })
}

#[tauri::command]
pub fn devtools_disconnect(app: AppHandle, serial: String) -> Result<(), String> {
    let port = forwards()
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&serial);
    let Some(port) = port else {
        return Ok(());
    };
    if let Ok(adb_path) = adb::adb_for(&app) {
        let _ = adb::run(
            &adb_path,
            &["-s", &serial, "forward", "--remove", &format!("tcp:{port}")],
        );
    }
    Ok(())
}
