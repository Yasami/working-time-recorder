//! 更新の確認と、インストーラーのダウンロード・実行
//!
//! 通信は別スレッドで行い、結果は `PostMessageW` でホストウィンドウ (UI スレッド) に送る。

use std::fs;
use std::io::ErrorKind;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr::null;
use std::thread;
use std::time::{Duration, SystemTime};

use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};
use windows_sys::Win32::UI::Shell::{NIIF_ERROR, NIIF_INFO, ShellExecuteW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IDYES, MB_ICONWARNING, MB_YESNO, MessageBoxW, PostMessageW, SW_SHOWNORMAL, WM_APP,
};
use windows_sys::core::w;

use super::{http, show_balloon, with_state};
use crate::update::{self, Release, Version};

/// 更新の確認が終わった (LPARAM は `Box<Result<Release, String>>`)
pub const WM_UPDATE_CHECKED: u32 = WM_APP + 2;
/// インストーラーのダウンロードが終わった (LPARAM は `Box<Result<PathBuf, String>>`)
pub const WM_UPDATE_DOWNLOADED: u32 = WM_APP + 3;

/// 起動してから最初に確認するまでの時間
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);
/// 確認できたときに、次に確認するまでの時間
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// 確認に失敗したときに、次に確認するまでの時間
const RETRY_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// GitHub API に付けるヘッダー
const API_HEADERS: &str = "Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28";

/// ダウンロードしたインストーラーを置く、%TEMP% の下のフォルダー名
const DOWNLOAD_DIR: &str = "working-time-recorder-update";

const APP_TITLE: &str = "作業時間ビューワー";

#[derive(Clone, Copy, PartialEq)]
enum Task {
    Idle,
    /// `manual` はメニューから確認したとき true
    Checking {
        manual: bool,
    },
    Downloading,
}

pub struct UpdateState {
    /// 次に自動で確認する時刻 (スリープ中も進むよう、SystemTime で持つ)
    next_check: SystemTime,
    task: Task,
    /// 見つかった新しいバージョン
    available: Option<Release>,
    /// 最後に出した通知が更新を勧めるもの (クリックすると更新する)
    balloon_offers_update: bool,
}

impl UpdateState {
    pub fn new() -> Self {
        Self {
            next_check: SystemTime::now() + FIRST_CHECK_DELAY,
            task: Task::Idle,
            available: None,
            balloon_offers_update: false,
        }
    }
}

/// メニューの項目 (表示する文字列と、選べるかどうか)
pub fn menu_item() -> (String, bool) {
    with_state(|s| match (s.update.task, &s.update.available) {
        (Task::Checking { .. }, _) => ("更新を確認中...".to_string(), false),
        (Task::Downloading, _) => ("更新をダウンロード中...".to_string(), false),
        (Task::Idle, Some(release)) => (format!("バージョン {} に更新", release.version), true),
        (Task::Idle, None) => ("更新を確認".to_string(), true),
    })
}

/// メニューの項目が選ばれた
pub fn on_menu() {
    if with_state(|s| s.update.available.is_some()) {
        start_update();
    } else {
        start_check(true);
    }
}

/// 通知がクリックされた
pub fn on_balloon_click() {
    if with_state(|s| s.update.balloon_offers_update) {
        start_update();
    }
}

/// 毎秒呼ばれ、自動で確認する時刻になっていれば確認する
pub fn tick() {
    let now = SystemTime::now();
    let due = with_state(|s| {
        // 時計が大きく戻されたら、確認が止まらないよう予定を詰める
        if s.update.next_check > now + CHECK_INTERVAL {
            s.update.next_check = now + CHECK_INTERVAL;
        }
        s.config.check_update && s.update.task == Task::Idle && now >= s.update.next_check
    });
    if due {
        start_check(false);
    }
}

/// ワーカースレッドの結果を UI スレッドに送る。送れなければ (ウィンドウが無ければ) 捨てる
fn post_result<T>(host: usize, msg: u32, result: T) {
    let ptr = Box::into_raw(Box::new(result));
    if unsafe { PostMessageW(host as HWND, msg, 0, ptr as LPARAM) } == 0 {
        drop(unsafe { Box::from_raw(ptr) });
    }
}

/// WM_UPDATE_CHECKED / WM_UPDATE_DOWNLOADED の LPARAM から結果を取り出す
///
/// # Safety
/// `lparam` は `post_result` が同じ型 `T` で送ったものであること
unsafe fn take_result<T>(lparam: LPARAM) -> T {
    *unsafe { Box::from_raw(lparam as *mut T) }
}

fn start_check(manual: bool) {
    let host = with_state(|s| {
        if s.update.task != Task::Idle {
            return None;
        }
        s.update.task = Task::Checking { manual };
        Some(s.host as usize)
    });
    let Some(host) = host else {
        return;
    };
    thread::spawn(move || {
        let result = http::get(update::LATEST_RELEASE_URL, API_HEADERS)
            .and_then(|body| update::parse_release(&String::from_utf8_lossy(&body)));
        post_result(host, WM_UPDATE_CHECKED, result);
    });
}

pub fn on_checked(lparam: LPARAM) {
    let result: Result<Release, String> = unsafe { take_result(lparam) };
    let now = SystemTime::now();
    let current = Version::current();
    // 出す通知 (タイトル, 本文, エラーか, 更新を勧めるか)
    let balloon = with_state(|s| {
        let manual = matches!(s.update.task, Task::Checking { manual: true });
        s.update.task = Task::Idle;
        match result {
            Ok(release) => {
                s.update.next_check = now + CHECK_INTERVAL;
                if release.version > current {
                    let version = release.version;
                    let is_new = s.update.available.as_ref().map(|r| r.version) != Some(version);
                    s.update.available = Some(release);
                    (manual || is_new).then(|| {
                        (
                            format!("バージョン {version} が公開されています"),
                            "ここをクリックすると更新します。".to_string(),
                            false,
                            true,
                        )
                    })
                } else {
                    s.update.available = None;
                    manual.then(|| {
                        (
                            "最新のバージョンです".to_string(),
                            format!("バージョン {current} は最新です。"),
                            false,
                            false,
                        )
                    })
                }
            }
            Err(e) => {
                s.update.next_check = now + RETRY_INTERVAL;
                manual.then(|| ("更新を確認できませんでした".to_string(), e, true, false))
            }
        }
    });
    if let Some((title, text, error, offers_update)) = balloon {
        with_state(|s| s.update.balloon_offers_update = offers_update);
        show_balloon(&title, &text, if error { NIIF_ERROR } else { NIIF_INFO });
    }
}

fn start_update() {
    let target = with_state(|s| {
        if s.update.task != Task::Idle {
            return None;
        }
        let release = s.update.available.clone()?;
        s.update.task = Task::Downloading;
        Some((s.host as usize, release))
    });
    let Some((host, release)) = target else {
        return;
    };
    thread::spawn(move || {
        post_result(host, WM_UPDATE_DOWNLOADED, download(&release));
    });
}

/// インストーラーをダウンロードし、ハッシュ値を照合して一時フォルダーに保存する
fn download(release: &Release) -> Result<PathBuf, String> {
    let body = http::get(&release.installer_url, "")?;
    if let Some(expected) = release.installer_sha256
        && sha256(&body)? != expected
    {
        return Err(
            "ダウンロードしたインストーラーのハッシュ値が、リリースのものと一致しません。"
                .to_string(),
        );
    }
    let dir = std::env::temp_dir().join(DOWNLOAD_DIR);
    // 前回の更新で残ったファイルを消す
    match fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(format!("一時フォルダーを削除できません: {e}")),
    }
    fs::create_dir_all(&dir).map_err(|e| format!("一時フォルダーを作成できません: {e}"))?;
    let path = dir.join(format!(
        "working-time-recorder-v{}-setup.exe",
        release.version
    ));
    fs::write(&path, &body).map_err(|e| format!("インストーラーを保存できません: {e}"))?;
    Ok(path)
}

fn sha256(data: &[u8]) -> Result<[u8; 32], String> {
    let len = u32::try_from(data.len()).map_err(|_| "データが大きすぎます".to_string())?;
    let mut hash = [0u8; 32];
    let status = unsafe {
        BCryptHash(
            BCRYPT_SHA256_ALG_HANDLE,
            null(),
            0,
            data.as_ptr(),
            len,
            hash.as_mut_ptr(),
            hash.len() as u32,
        )
    };
    if status != 0 {
        return Err(format!(
            "ハッシュ値を計算できません (エラー 0x{:08X})",
            status as u32
        ));
    }
    Ok(hash)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub fn on_downloaded(lparam: LPARAM) {
    let result: Result<PathBuf, String> = unsafe { take_result(lparam) };
    let host = with_state(|s| {
        s.update.task = Task::Idle;
        s.host
    });
    let error = match result {
        Ok(path) => {
            let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            // インストーラーがこのビューワーを終了させ、インストール後に起動し直す
            let code = unsafe {
                ShellExecuteW(
                    host,
                    w!("open"),
                    file.as_ptr(),
                    w!("/SILENT /SP- /NORESTART"),
                    null(),
                    SW_SHOWNORMAL,
                )
            };
            // 32 以下はエラー
            if code as usize > 32 {
                return;
            }
            format!("インストーラーを実行できません (エラー {})", code as usize)
        }
        Err(e) => e,
    };
    offer_release_page(host, &error);
}

/// 自動で更新できなかったことを伝え、リリースのページを開くか聞く
fn offer_release_page(host: HWND, error: &str) {
    let text = wide(&format!(
        "更新できませんでした。\n\n{error}\n\n\
         リリースのページを開いて、インストーラーを手動でダウンロードしますか?"
    ));
    let title = wide(APP_TITLE);
    unsafe {
        if MessageBoxW(
            host,
            text.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONWARNING,
        ) == IDYES
        {
            let url = wide(update::LATEST_RELEASE_PAGE);
            ShellExecuteW(
                host,
                w!("open"),
                url.as_ptr(),
                null(),
                null(),
                SW_SHOWNORMAL,
            );
        }
    }
}
