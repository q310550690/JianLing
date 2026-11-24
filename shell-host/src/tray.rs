//! 托盘图标（Shell_NotifyIcon）：左键还原窗口，右键弹「打开窗口 / 退出」。
//!
//! 安装失败时返回 None，宿主继续按普通窗口运行。

#![cfg(windows)]

use std::ffi::c_void;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{
    ExtractIconExW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    GetCursorPos, LoadIconW, PostMessageW, RegisterClassW, SetForegroundWindow, ShowWindow,
    TrackPopupMenu, MF_STRING, SW_RESTORE, SW_SHOW, TPM_RETURNCMD, WINDOW_EX_STYLE, WM_CLOSE,
    WM_COMMAND, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};

const WM_TRAY: u32 = 0x8000 + 1;
const MENU_OPEN: usize = 1001;
const MENU_EXIT: usize = 1002;

static TARGET: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static FORCE_EXIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub struct Tray {
    hwnd: HWND,
    nid: Box<NOTIFYICONDATAW>,
}

pub fn install(tip: &str, target_hwnd: isize) -> Option<Tray> {
    TARGET.store(target_hwnd, std::sync::atomic::Ordering::SeqCst);

    let class = to_wide("JianlingTrayWindow");
    let wnd_class = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };
    unsafe { RegisterClassW(&wnd_class) };
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class.as_ptr()),
            PCWSTR(class.as_ptr()),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        )
    }
    .ok()?;

    let icon = load_icon();
    let mut nid = Box::new(NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    });
    let tip_wide: Vec<u16> = tip.encode_utf16().take(127).collect();
    nid.szTip[..tip_wide.len()].copy_from_slice(&tip_wide);

    if !unsafe { Shell_NotifyIconW(NIM_ADD, &*nid).as_bool() } {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return None;
    }
    Some(Tray { hwnd, nid })
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &*self.nid);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn load_icon() -> windows::Win32::UI::WindowsAndMessaging::HICON {
    let exe = std::env::current_exe().unwrap_or_default();
    let wide = to_wide(&exe.to_string_lossy());
    let mut large = Default::default();
    let mut small = Default::default();
    let count = unsafe { ExtractIconExW(PCWSTR(wide.as_ptr()), 0, Some(&mut large), Some(&mut small), 1) };
    if count > 0 && !large.is_invalid() {
        return large;
    }
    unsafe { LoadIconW(None, PCWSTR(32512usize as *const u16)) }.unwrap_or_default()
}

fn target_hwnd() -> Option<HWND> {
    let raw = TARGET.load(std::sync::atomic::Ordering::SeqCst);
    if raw == 0 {
        None
    } else {
        Some(HWND(raw as *mut c_void))
    }
}

fn show_window() {
    let Some(hwnd) = target_hwnd() else {
        return;
    };
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
}

fn request_exit() {
    FORCE_EXIT.store(true, std::sync::atomic::Ordering::SeqCst);
    if let Some(hwnd) = target_hwnd() {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

fn popup_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let open = to_wide("打开窗口");
        let exit = to_wide("退出");
        let _ = AppendMenuW(menu, MF_STRING, MENU_OPEN, PCWSTR(open.as_ptr()));
        let _ = AppendMenuW(menu, MF_STRING, MENU_EXIT, PCWSTR(exit.as_ptr()));
        let mut point = Default::default();
        let _ = GetCursorPos(&mut point);
        // 菜单弹出前把窗口置前，否则点空白处菜单不会消失
        let _ = SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD,
            point.x,
            point.y,
            Some(0),
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        match command.0 as usize {
            MENU_OPEN => show_window(),
            MENU_EXIT => request_exit(),
            _ => {}
        }
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_TRAY {
        match lparam.0 as u32 & 0xFFFF {
            WM_LBUTTONUP => show_window(),
            WM_RBUTTONUP => popup_menu(hwnd),
            _ => {}
        }
        return LRESULT(0);
    }
    if msg == WM_COMMAND {
        match wparam.0 & 0xFFFF {
            MENU_OPEN => show_window(),
            MENU_EXIT => request_exit(),
            _ => {}
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn to_wide(text: &str) -> Vec<u16> {
    let mut out: Vec<u16> = text.encode_utf16().collect();
    out.push(0);
    out
}

/// 托盘菜单点了「退出」：主循环据此强制退出，而不是继续走「最小化到托盘」。
pub fn exit_requested() -> bool {
    FORCE_EXIT.load(std::sync::atomic::Ordering::SeqCst)
}
