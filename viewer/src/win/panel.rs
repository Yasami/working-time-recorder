//! タスクトレイのアイコンをクリックしたときに出るパネル

use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use chrono::{Datelike, Duration, Local};
use windows_sys::core::{w, PCWSTR};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, InvalidateRect, MonitorFromPoint, PtInRect, DT_CENTER, DT_END_ELLIPSIS,
    DT_LEFT, DT_RIGHT, DT_VCENTER, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ESCAPE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::Win32::System::SystemInformation::GetTickCount;

use super::gdi::{paint_buffered, rect, task_color, Font, Scale};
use super::{bar, hinstance, history, reload_records, with_state};
use crate::record::{format_hm, weekday_ja, DaySummary, WORKDAY_SECONDS};

pub const CLASS: PCWSTR = w!("WorkingTimeViewer.Panel");

const TIMER_REFRESH: usize = 1;
const REFRESH_INTERVAL_MS: u32 = 30_000;
const WA_INACTIVE: usize = 0;
const WM_MOUSELEAVE: u32 = 0x02A3;

const WIDTH: i32 = 360;
const PAD: i32 = 16;
const ROW_H: i32 = 24;
const MAX_ROWS: usize = 8;

struct Layout {
    header: RECT,
    total: RECT,
    status: RECT,
    bar: RECT,
    bar_labels: RECT,
    legend_top: i32,
    separator_y: i32,
    button: RECT,
    height: i32,
}

/// 凡例の行数 (多すぎる分は「ほか N 件」の 1 行にまとめる)
fn legend_rows(task_count: usize) -> usize {
    if task_count > MAX_ROWS {
        MAX_ROWS + 1
    } else {
        task_count.max(1)
    }
}

fn layout(scale: Scale, task_count: usize) -> Layout {
    let s = |dip: i32| scale.px(dip);
    let (left, right) = (s(PAD), s(WIDTH - PAD));
    let legend_top = 160;
    let separator_y = legend_top + legend_rows(task_count) as i32 * ROW_H + 10;
    let button_top = separator_y + 12;
    Layout {
        header: rect(left, s(16), right, s(36)),
        total: rect(left, s(40), right, s(80)),
        status: rect(left, s(82), right, s(102)),
        bar: rect(left, s(114), right, s(134)),
        bar_labels: rect(left, s(137), right, s(153)),
        legend_top: s(legend_top),
        separator_y: s(separator_y),
        button: rect(left, s(button_top), right, s(button_top + 32)),
        height: s(button_top + 32 + PAD),
    }
}

struct Snapshot {
    today: DaySummary,
    current_task: Option<String>,
    error: Option<String>,
}

fn snapshot() -> Snapshot {
    let now = Local::now();
    with_state(|s| match &s.records {
        Ok(records) => Snapshot {
            today: records.day_summary(now.date_naive(), now),
            current_task: records.current_task().map(str::to_owned),
            error: None,
        },
        Err(e) => Snapshot {
            today: DaySummary::empty(now.date_naive()),
            current_task: None,
            error: Some(e.clone()),
        },
    })
}

pub fn create() -> HWND {
    unsafe {
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            CLASS,
            w!("今日の作業時間"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            null_mut(),
            null_mut(),
            hinstance(),
            null(),
        );
        let preference = DWMWCP_ROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &preference as *const _ as *const _,
            size_of::<i32>() as u32,
        );
        hwnd
    }
}

pub fn toggle() {
    let (hwnd, hidden_at) = with_state(|s| (s.panel, s.panel_hidden_at));
    unsafe {
        if IsWindowVisible(hwnd) != 0 {
            hide();
        } else if GetTickCount().wrapping_sub(hidden_at) > 500 {
            // アイコンのクリックでフォーカスが外れて閉じた直後なら開き直さない
            show();
        }
    }
}

pub fn show() {
    reload_records();
    let hwnd = with_state(|s| s.panel);
    unsafe {
        let mut cursor: POINT = zeroed();
        GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        with_state(|s| {
            s.panel_monitor = monitor;
            s.panel_hover = false;
        });
        place(hwnd);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        InvalidateRect(hwnd, null(), 0);
        SetTimer(hwnd, TIMER_REFRESH, REFRESH_INTERVAL_MS, None);
    }
}

fn hide() {
    let hwnd = with_state(|s| s.panel);
    unsafe {
        KillTimer(hwnd, TIMER_REFRESH);
        ShowWindow(hwnd, SW_HIDE);
        let now = GetTickCount();
        with_state(|s| {
            s.panel_hidden_at = now;
            s.panel_hover = false;
        });
    }
}

fn today_task_count() -> usize {
    let now = Local::now();
    with_state(|s| match &s.records {
        Ok(records) => records.day_summary(now.date_naive(), now).tasks.len(),
        Err(_) => 0,
    })
}

/// タスクバーの近く (作業領域の端) に置く
fn place(hwnd: HWND) {
    let monitor = with_state(|s| s.panel_monitor);
    let task_count = today_task_count();
    unsafe {
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(monitor, &mut info);
        let (mut dpi_x, mut dpi_y) = (96, 96);
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);

        let scale = Scale(dpi_x);
        let width = scale.px(WIDTH);
        let height = layout(scale, task_count).height;
        let margin = scale.px(12);
        let (work, full) = (info.rcWork, info.rcMonitor);
        let x = if work.left > full.left {
            work.left + margin
        } else {
            work.right - width - margin
        };
        let y = if work.top > full.top {
            work.top + margin
        } else {
            work.bottom - height - margin
        };
        SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE);
    }
}

fn button_rect(hwnd: HWND) -> RECT {
    let scale = Scale(unsafe { GetDpiForWindow(hwnd) });
    layout(scale, today_task_count()).button
}

fn point_from_lparam(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam & 0xFFFF) as u16 as i16 as i32,
        y: ((lparam >> 16) & 0xFFFF) as u16 as i16 as i32,
    }
}

fn paint(hwnd: HWND) {
    let snap = snapshot();
    let hover = with_state(|s| s.panel_hover);

    paint_buffered(hwnd, |p, client| {
        let t = p.theme;
        let s = |dip: i32| p.px(dip);
        let l = layout(p.scale, snap.today.tasks.len());
        let title_font = Font::new(p.scale, 14, true);
        let big_font = Font::new(p.scale, 30, true);
        let body_font = Font::new(p.scale, 13, false);
        let small_font = Font::new(p.scale, 11, false);

        p.fill(client, t.bg);

        // 見出しと日付
        let date = snap.today.date;
        p.text("今日の作業時間", l.header, &title_font, t.text, DT_LEFT | DT_VCENTER);
        let date_text = format!("{}月{}日 ({})", date.month(), date.day(), weekday_ja(date));
        p.text(&date_text, l.header, &body_font, t.subtext, DT_RIGHT | DT_VCENTER);

        // 合計
        let total = snap.today.total();
        let total_text = format_hm(total);
        p.text(&total_text, l.total, &big_font, t.text, DT_LEFT | DT_VCENTER);
        let rest = RECT {
            left: l.total.left + p.text_width(&total_text, &big_font) + s(6),
            ..l.total
        };
        p.text("/ 8:00", rest, &body_font, t.subtext, DT_LEFT | DT_VCENTER);
        let overtime = total - Duration::seconds(WORKDAY_SECONDS);
        if overtime > Duration::zero() {
            let text = format!("+{} 超過", format_hm(overtime));
            p.text(&text, l.total, &body_font, t.danger, DT_RIGHT | DT_VCENTER);
        }

        // 状態
        if let Some(error) = &snap.error {
            p.text(error, l.status, &small_font, t.danger, DT_LEFT | DT_VCENTER | DT_END_ELLIPSIS);
        } else if let Some(task) = &snap.current_task {
            p.text("●", l.status, &small_font, t.active, DT_LEFT | DT_VCENTER);
            let text_rect = RECT {
                left: l.status.left + p.text_width("●", &small_font) + s(4),
                ..l.status
            };
            let text = format!("作業中: {}", task);
            p.text(&text, text_rect, &body_font, t.text, DT_LEFT | DT_VCENTER | DT_END_ELLIPSIS);
        } else {
            p.text("停止中", l.status, &body_font, t.subtext, DT_LEFT | DT_VCENTER);
        }

        // 横バー
        bar::draw(p, l.bar, &snap.today);
        p.text("0h", l.bar_labels, &small_font, t.subtext, DT_LEFT | DT_VCENTER);
        p.text("4h", l.bar_labels, &small_font, t.subtext, DT_CENTER | DT_VCENTER);
        p.text("8h", l.bar_labels, &small_font, t.subtext, DT_RIGHT | DT_VCENTER);

        // 凡例
        let row = |i: usize| {
            let top = l.legend_top + s(ROW_H) * i as i32;
            rect(l.bar.left, top, l.bar.right, top + s(ROW_H))
        };
        let tasks = &snap.today.tasks;
        if tasks.is_empty() {
            p.text("今日の記録はまだありません", row(0), &body_font, t.subtext, DT_LEFT | DT_VCENTER);
        }
        let visible = if tasks.len() > MAX_ROWS { MAX_ROWS } else { tasks.len() };
        for (i, task) in tasks.iter().take(visible).enumerate() {
            let r = row(i);
            let marker_top = (r.top + r.bottom - s(10)) / 2;
            p.fill_round(rect(r.left, marker_top, r.left + s(10), marker_top + s(10)), 2, task_color(task.color));
            let name_rect = rect(r.left + s(18), r.top, r.right - s(64), r.bottom);
            p.text(&task.name, name_rect, &body_font, t.text, DT_LEFT | DT_VCENTER | DT_END_ELLIPSIS);
            p.text(&format_hm(task.duration), r, &body_font, t.subtext, DT_RIGHT | DT_VCENTER);
        }
        if tasks.len() > visible {
            let others = &tasks[visible..];
            let duration = others.iter().fold(Duration::zero(), |acc, t| acc + t.duration);
            let r = row(visible);
            let label_rect = RECT { left: r.left + s(18), ..r };
            p.text(&format!("ほか {} 件", others.len()), label_rect, &body_font, t.subtext, DT_LEFT | DT_VCENTER);
            p.text(&format_hm(duration), r, &body_font, t.subtext, DT_RIGHT | DT_VCENTER);
        }

        // 履歴ボタン
        p.fill(rect(l.bar.left, l.separator_y, l.bar.right, l.separator_y + s(1).max(1)), t.separator);
        p.fill_round(l.button, 4, if hover { t.button_hover } else { t.button });
        p.text("履歴を表示", l.button, &body_font, t.text, DT_CENTER | DT_VCENTER);
    });
}

pub unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_ACTIVATE => {
            if wparam & 0xFFFF == WA_INACTIVE && IsWindowVisible(hwnd) != 0 {
                hide();
            }
            0
        }
        WM_MOUSEMOVE => {
            let hover = PtInRect(&button_rect(hwnd), point_from_lparam(lparam)) != 0;
            if with_state(|s| std::mem::replace(&mut s.panel_hover, hover)) != hover {
                InvalidateRect(hwnd, null(), 0);
            }
            let mut track = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            TrackMouseEvent(&mut track);
            0
        }
        WM_MOUSELEAVE => {
            if with_state(|s| std::mem::replace(&mut s.panel_hover, false)) {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_LBUTTONUP => {
            if PtInRect(&button_rect(hwnd), point_from_lparam(lparam)) != 0 {
                hide();
                history::open();
            }
            0
        }
        WM_KEYDOWN => {
            if wparam == VK_ESCAPE as usize {
                hide();
            }
            0
        }
        WM_TIMER => {
            if wparam == TIMER_REFRESH {
                reload_records();
                place(hwnd);
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_DPICHANGED => {
            place(hwnd);
            0
        }
        WM_SETTINGCHANGE => {
            InvalidateRect(hwnd, null(), 0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
