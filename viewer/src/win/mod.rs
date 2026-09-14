//! タスクトレイ常駐とウィンドウ管理 (Win32)

mod bar;
mod gdi;
mod history;
mod panel;

use std::cell::RefCell;
use std::fs;
use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::time::{Instant, SystemTime};

use windows_sys::core::{w, PCWSTR};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::HMONITOR;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{
    GetDpiForSystem, GetSystemMetricsForDpi, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE,
    NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::config::{self, Config};
use crate::record::{self, Records};

const HOST_CLASS: PCWSTR = w!("WorkingTimeViewer.Host");
const WM_TRAY: u32 = WM_APP + 1;
const TIMER_WATCH: usize = 1;
const WATCH_INTERVAL_MS: u32 = 1_000;
const TRAY_UID: u32 = 1;
const NIN_SELECT: u32 = WM_USER;
const NIN_KEYSELECT: u32 = WM_USER + 1;

const ID_MENU_TODAY: usize = 1;
const ID_MENU_HISTORY: usize = 2;
const ID_MENU_EXIT: usize = 3;

pub struct State {
    pub host: HWND,
    pub panel: HWND,
    pub history: HWND,
    pub record_path: PathBuf,
    pub records: Result<Records, String>,
    pub config: Config,
    pub config_error: Option<String>,
    /// 最後に読み込んだときの記録ファイル・設定ファイルの状態
    pub record_stamp: Option<FileStamp>,
    pub config_stamp: Option<FileStamp>,
    /// 記録ファイルが最後に変化した (または自動でパネルを表示した) 時刻
    pub last_change: Instant,
    pub tray_icon: HICON,
    pub taskbar_created: u32,
    pub panel_monitor: HMONITOR,
    pub panel_hover: bool,
    pub panel_hidden_at: u32,
    /// パネルを自動で表示中 (まだ操作されていない)
    pub panel_auto: bool,
    pub history_scroll: i32,
}

/// ファイルの変化を調べるための更新日時とサイズ
#[derive(Clone, Copy, PartialEq)]
pub struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

fn file_stamp(path: &Path) -> Option<FileStamp> {
    let metadata = fs::metadata(path).ok()?;
    Some(FileStamp { modified: metadata.modified().ok(), len: metadata.len() })
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// 状態へのアクセス。
/// クロージャ内でメッセージを送る Win32 API (ShowWindow など) を呼ぶと
/// ウィンドウプロシージャが再入して二重借用になるので、値の読み書きだけにすること。
pub fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|state| f(state.borrow_mut().as_mut().expect("state is not initialized")))
}

/// 記録ファイルと設定ファイルを読み直す
pub fn reload_records() {
    let path = with_state(|s| s.record_path.clone());
    let config_path = config::config_path(&path);
    // 読み込み中に変化しても取りこぼさないよう、先に状態を控える
    let record_stamp = file_stamp(&path);
    let config_stamp = file_stamp(&config_path);
    let (config, config_error) = match config::load(&config_path) {
        Ok(config) => (config, None),
        Err(e) => (Config::default(), Some(e)),
    };
    let records = record::load(&path).map(|r| r.with_labels(config.labels.clone()));
    with_state(|s| {
        s.records = records;
        s.config = config;
        s.config_error = config_error;
        s.record_stamp = record_stamp;
        s.config_stamp = config_stamp;
    });
}

/// 記録ファイルが変化したら、または前回の変化から設定した時間が経ったら、パネルを自動で表示する
fn watch_files() {
    let (path, record_stamp, config_stamp, last_change, popup_interval) = with_state(|s| {
        (s.record_path.clone(), s.record_stamp, s.config_stamp, s.last_change, s.config.popup_interval)
    });
    let record_changed = file_stamp(&path) != record_stamp;
    let config_changed = file_stamp(&config::config_path(&path)) != config_stamp;
    if record_changed || config_changed {
        reload_records();
        panel::refresh();
        history::refresh();
    }
    if record_changed || popup_interval.is_some_and(|interval| last_change.elapsed() >= interval) {
        with_state(|s| s.last_change = Instant::now());
        panel::show_auto();
    }
}

pub fn hinstance() -> windows_sys::Win32::Foundation::HINSTANCE {
    unsafe { GetModuleHandleW(null()) }
}

pub fn run() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let mutex = CreateMutexW(null(), 1, w!("Local\\WorkingTimeViewer.SingleInstance"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(mutex);
            return;
        }

        let dpi = GetDpiForSystem();
        let small_icon = gdi::create_app_icon(GetSystemMetricsForDpi(SM_CXSMICON, dpi));
        let large_icon = gdi::create_app_icon(GetSystemMetricsForDpi(SM_CXICON, dpi));

        STATE.with(|state| {
            *state.borrow_mut() = Some(State {
                host: null_mut(),
                panel: null_mut(),
                history: null_mut(),
                record_path: record::record_path(),
                records: Ok(Records::default()),
                config: Config::default(),
                config_error: None,
                record_stamp: None,
                config_stamp: None,
                last_change: Instant::now(),
                tray_icon: small_icon,
                taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
                panel_monitor: null_mut(),
                panel_hover: false,
                panel_hidden_at: 0,
                panel_auto: false,
                history_scroll: 0,
            })
        });

        register_class(HOST_CLASS, host_proc, null_mut(), null_mut());
        register_class(panel::CLASS, panel::wndproc, small_icon, large_icon);
        register_class(history::CLASS, history::wndproc, small_icon, large_icon);

        let host = CreateWindowExW(
            0,
            HOST_CLASS,
            w!("Working Time Viewer"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinstance(),
            null(),
        );
        let panel = panel::create();
        with_state(|s| {
            s.host = host;
            s.panel = panel;
        });

        add_tray_icon();

        // 起動時点の状態を基準に、以降の変化を監視する
        reload_records();
        SetTimer(host, TIMER_WATCH, WATCH_INTERVAL_MS, None);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        remove_tray_icon();
        DestroyIcon(small_icon);
        DestroyIcon(large_icon);
        CloseHandle(mutex);
    }
}

unsafe fn register_class(
    name: PCWSTR,
    proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
    small_icon: HICON,
    large_icon: HICON,
) {
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance(),
        hIcon: large_icon,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: null_mut(),
        lpszMenuName: null(),
        lpszClassName: name,
        hIconSm: small_icon,
    };
    RegisterClassExW(&class);
}

fn tray_data() -> NOTIFYICONDATAW {
    let (host, icon) = with_state(|s| (s.host, s.tray_icon));
    let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = host;
    data.uID = TRAY_UID;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    data.uCallbackMessage = WM_TRAY;
    data.hIcon = icon;
    let tip: Vec<u16> = "作業時間ビューワー".encode_utf16().collect();
    let len = tip.len().min(data.szTip.len() - 1);
    data.szTip[..len].copy_from_slice(&tip[..len]);
    data
}

fn add_tray_icon() {
    let mut data = tray_data();
    unsafe {
        Shell_NotifyIconW(NIM_ADD, &data);
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &data);
    }
}

fn remove_tray_icon() {
    let data = tray_data();
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &data);
    }
}

unsafe fn show_tray_menu(host: HWND) {
    let menu = CreatePopupMenu();
    AppendMenuW(menu, MF_STRING, ID_MENU_TODAY, w!("今日の作業時間"));
    AppendMenuW(menu, MF_STRING, ID_MENU_HISTORY, w!("履歴を表示"));
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, ID_MENU_EXIT, w!("終了"));

    let mut point: POINT = zeroed();
    GetCursorPos(&mut point);
    // メニュー外クリックで閉じるために必要
    SetForegroundWindow(host);
    TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RIGHTALIGN,
        point.x,
        point.y,
        0,
        host,
        null(),
    );
    PostMessageW(host, WM_NULL, 0, 0);
    DestroyMenu(menu);
}

unsafe extern "system" fn host_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            match (lparam & 0xFFFF) as u32 {
                NIN_SELECT | NIN_KEYSELECT => panel::toggle(),
                WM_CONTEXTMENU => show_tray_menu(hwnd),
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            match wparam & 0xFFFF {
                ID_MENU_TODAY => panel::show(),
                ID_MENU_HISTORY => history::open(),
                ID_MENU_EXIT => {
                    let history = with_state(|s| s.history);
                    if !history.is_null() {
                        DestroyWindow(history);
                    }
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_TIMER => {
            if wparam == TIMER_WATCH {
                watch_files();
            }
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_WATCH);
            PostQuitMessage(0);
            0
        }
        _ => {
            // エクスプローラーが再起動したらアイコンを登録し直す
            if msg != 0 && msg == with_state(|s| s.taskbar_created) {
                add_tray_icon();
                return 0;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}
