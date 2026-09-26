//! WinHTTP による HTTPS の GET (OS のプロキシ設定と証明書ストアを使う)

use std::ffi::c_void;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Networking::WinHttp::*;
use windows_sys::Win32::System::Diagnostics::Debug::{
    FORMAT_MESSAGE_FROM_HMODULE, FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS,
    FormatMessageW,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::core::w;

/// 応答の大きさの上限 (インストーラーが入れば十分)
const MAX_BODY_SIZE: usize = 100 * 1024 * 1024;

/// 名前解決・接続・送信・受信のタイムアウト (ミリ秒)
const TIMEOUT_MS: i32 = 30_000;

/// 閉じ忘れないよう、WinHTTP のハンドルを Drop で閉じる
struct Handle(*mut c_void);

impl Handle {
    fn new(handle: *mut c_void) -> Result<Self, String> {
        if handle.is_null() {
            Err(last_error())
        } else {
            Ok(Self(handle))
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// 直前に失敗した WinHTTP の関数のエラーメッセージ
fn last_error() -> String {
    let code = unsafe { GetLastError() };
    let mut buffer = [0u16; 512];
    // WinHTTP のエラー (12000 番台) のメッセージは winhttp.dll にある
    let len = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_FROM_HMODULE
                | FORMAT_MESSAGE_FROM_SYSTEM
                | FORMAT_MESSAGE_IGNORE_INSERTS,
            GetModuleHandleW(w!("winhttp.dll")),
            code,
            0,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            null(),
        )
    };
    let message = String::from_utf16_lossy(&buffer[..len as usize]);
    let message = message.trim();
    if message.is_empty() {
        format!("通信に失敗しました (エラー {code})")
    } else {
        format!("通信に失敗しました: {message} (エラー {code})")
    }
}

/// `url` を GET して本文を返す。`headers` は `\r\n` 区切りの追加ヘッダー (無ければ空文字列)。
/// リダイレクトはたどる。HTTPS 以外の URL と、200 以外の応答はエラー
pub fn get(url: &str, headers: &str) -> Result<Vec<u8>, String> {
    let url_wide = wide(url);
    let mut parts = URL_COMPONENTS {
        dwStructSize: size_of::<URL_COMPONENTS>() as u32,
        // 0 以外にすると、各部分の位置と長さを url_wide の中から返す
        dwHostNameLength: u32::MAX,
        dwUrlPathLength: u32::MAX,
        dwExtraInfoLength: u32::MAX,
        ..Default::default()
    };
    if unsafe { WinHttpCrackUrl(url_wide.as_ptr(), 0, 0, &mut parts) } == 0 {
        return Err(format!("URL が正しくありません: {url}"));
    }
    if parts.nScheme != WINHTTP_INTERNET_SCHEME_HTTPS {
        return Err(format!("HTTPS ではない URL には接続しません: {url}"));
    }
    // ホスト名は NUL 終端が必要。パスとクエリ (ExtraInfo) は URL の中で隣り合っている
    let host: Vec<u16> =
        unsafe { std::slice::from_raw_parts(parts.lpszHostName, parts.dwHostNameLength as usize) }
            .iter()
            .copied()
            .chain(Some(0))
            .collect();
    let path: Vec<u16> = unsafe {
        std::slice::from_raw_parts(
            parts.lpszUrlPath,
            (parts.dwUrlPathLength + parts.dwExtraInfoLength) as usize,
        )
    }
    .iter()
    .copied()
    .chain(Some(0))
    .collect();
    let headers = (!headers.is_empty()).then(|| wide(headers));

    unsafe {
        let session = Handle::new(WinHttpOpen(
            w!("working-time-viewer"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS);
        let connection = Handle::new(WinHttpConnect(session.0, host.as_ptr(), parts.nPort, 0))?;
        let request = Handle::new(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            path.as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        // ヘッダーの長さに u32::MAX (-1) を渡すと NUL 終端として扱われる
        let (headers, headers_len) = match &headers {
            Some(headers) => (headers.as_ptr(), u32::MAX),
            None => (null(), 0),
        };
        if WinHttpSendRequest(request.0, headers, headers_len, null(), 0, 0, 0) == 0
            || WinHttpReceiveResponse(request.0, null_mut()) == 0
        {
            return Err(last_error());
        }

        let mut status: u32 = 0;
        let mut status_size = size_of::<u32>() as u32;
        if WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&raw mut status).cast(),
            &mut status_size,
            null_mut(),
        ) == 0
        {
            return Err(last_error());
        }
        if status != 200 {
            return Err(format!(
                "サーバーがエラーを返しました (HTTP {status}): {url}"
            ));
        }

        let mut body = Vec::new();
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let mut read: u32 = 0;
            if WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            ) == 0
            {
                return Err(last_error());
            }
            if read == 0 {
                return Ok(body);
            }
            body.extend_from_slice(&buffer[..read as usize]);
            if body.len() > MAX_BODY_SIZE {
                return Err(format!("応答が大きすぎます: {url}"));
            }
        }
    }
}
