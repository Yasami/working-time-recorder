//! 日ごとの作業時間の履歴ウィンドウ

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use chrono::{Datelike, Duration, Local};
use windows_sys::core::{w, PCWSTR};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows_sys::Win32::Graphics::Gdi::{
    InvalidateRect, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_VCENTER,
};
use windows_sys::Win32::UI::Controls::SetScrollInfo;
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_NEXT, VK_PRIOR, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::gdi::{paint_buffered, rect, task_color, Font, Scale, Theme};
use super::{bar, hinstance, reload_records, with_state};
use crate::record::{format_hm, weekday_ja, DaySummary, IDLE_LABEL, WORKDAY_SECONDS};

pub const CLASS: PCWSTR = w!("WorkingTimeViewer.History");

const PAD: i32 = 20;
const HEADER_H: i32 = 60;
const ROW_H: i32 = 68;
const DATE_COL: i32 = 150;

fn snapshot() -> Result<Vec<DaySummary>, String> {
    let now = Local::now();
    with_state(|s| match &s.records {
        Ok(records) => Ok(records.daily_summaries(now).into_values().rev().collect()),
        Err(e) => Err(e.clone()),
    })
}

fn content_height(scale: Scale, day_count: usize) -> i32 {
    scale.px(HEADER_H) + scale.px(ROW_H) * day_count.max(1) as i32 + scale.px(PAD)
}

pub fn open() {
    let existing = with_state(|s| s.history);
    unsafe {
        if !existing.is_null() {
            if IsIconic(existing) != 0 {
                ShowWindow(existing, SW_RESTORE);
            }
            SetForegroundWindow(existing);
            return;
        }

        reload_records();
        let scale = Scale(GetDpiForSystem());
        with_state(|s| s.history_scroll = 0);
        let hwnd = CreateWindowExW(
            0,
            CLASS,
            w!("作業時間の履歴"),
            WS_OVERLAPPEDWINDOW | WS_VSCROLL,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale.px(720),
            scale.px(600),
            null_mut(),
            null_mut(),
            hinstance(),
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        with_state(|s| s.history = hwnd);
        apply_title_bar_theme(hwnd);
        update_scrollbar(hwnd);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
    }
}

fn apply_title_bar_theme(hwnd: HWND) {
    let dark = Theme::current().dark as i32;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &dark as *const i32 as *const c_void,
            size_of::<i32>() as u32,
        );
    }
}

fn client_height(hwnd: HWND) -> i32 {
    let mut client: RECT = unsafe { zeroed() };
    unsafe { GetClientRect(hwnd, &mut client) };
    client.bottom - client.top
}

fn update_scrollbar(hwnd: HWND) {
    let day_count = snapshot().map_or(0, |days| days.len());
    let scale = Scale(unsafe { GetDpiForWindow(hwnd) });
    let content = content_height(scale, day_count);
    let page = client_height(hwnd).max(0);
    let max_position = (content - page).max(0);
    let position = with_state(|s| {
        s.history_scroll = s.history_scroll.clamp(0, max_position);
        s.history_scroll
    });
    let info = SCROLLINFO {
        cbSize: size_of::<SCROLLINFO>() as u32,
        fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
        nMin: 0,
        nMax: content - 1,
        nPage: page as u32,
        nPos: position,
        nTrackPos: 0,
    };
    unsafe {
        SetScrollInfo(hwnd, SB_VERT, &info, 1);
    }
}

fn scroll_to(hwnd: HWND, position: i32) {
    with_state(|s| s.history_scroll = position.max(0));
    update_scrollbar(hwnd);
    unsafe {
        InvalidateRect(hwnd, null(), 0);
    }
}

fn scroll_by(hwnd: HWND, delta: i32) {
    let current = with_state(|s| s.history_scroll);
    scroll_to(hwnd, current.saturating_add(delta));
}

fn paint(hwnd: HWND) {
    let days = snapshot();
    let scroll = with_state(|s| s.history_scroll);

    paint_buffered(hwnd, |p, client| {
        let t = p.theme;
        let s = |dip: i32| p.px(dip);
        let (left, right) = (s(PAD), client.right - s(PAD));
        let title_font = Font::new(p.scale, 18, true);
        let date_font = Font::new(p.scale, 14, true);
        let body_font = Font::new(p.scale, 13, false);
        let small_font = Font::new(p.scale, 12, false);

        p.fill(client, t.bg);

        let origin = -scroll;
        let header = rect(left, origin + s(16), right, origin + s(46));
        p.text("作業時間の履歴", header, &title_font, t.text, DT_LEFT | DT_VCENTER);

        let first_row = rect(left, origin + s(HEADER_H), right, origin + s(HEADER_H + ROW_H));
        let days = match days {
            Err(error) => {
                p.text(&error, first_row, &body_font, t.danger, DT_LEFT | DT_VCENTER | DT_END_ELLIPSIS);
                return;
            }
            Ok(days) if days.is_empty() => {
                p.text("記録はまだありません", first_row, &body_font, t.subtext, DT_LEFT | DT_VCENTER);
                return;
            }
            Ok(days) => days,
        };
        p.text(&format!("{} 日分", days.len()), header, &body_font, t.subtext, DT_RIGHT | DT_VCENTER);

        for (i, day) in days.iter().enumerate() {
            let top = origin + s(HEADER_H) + s(ROW_H) * i as i32;
            if top + s(ROW_H) < 0 {
                continue;
            }
            if top > client.bottom {
                break;
            }
            p.fill(rect(left, top, right, top + s(1).max(1)), t.separator);

            // 日付と合計
            let date = day.date;
            let date_text = format!("{}/{:02}/{:02} ({})", date.year(), date.month(), date.day(), weekday_ja(date));
            p.text(&date_text, rect(left, top + s(12), left + s(DATE_COL), top + s(34)), &date_font, t.text, DT_LEFT | DT_VCENTER);
            let total = day.total();
            let over = total > Duration::seconds(WORKDAY_SECONDS);
            p.text(
                &format!("{} / 8:00", format_hm(total)),
                rect(left, top + s(36), left + s(DATE_COL), top + s(56)),
                &body_font,
                if over { t.danger } else { t.subtext },
                DT_LEFT | DT_VCENTER,
            );

            // 横バーとタスクごとの内訳
            let bar_left = left + s(DATE_COL);
            if bar_left >= right {
                continue;
            }
            bar::draw(p, rect(bar_left, top + s(16), right, top + s(30)), day);

            let (legend_top, legend_bottom) = (top + s(36), top + s(56));
            let mut x = bar_left;
            let idle = day.idle();
            let items = day.tasks.iter().map(Some).chain((idle > Duration::zero()).then_some(None));
            for item in items {
                let label = match item {
                    Some(task) => format!("{} {}", task.label, format_hm(task.duration)),
                    None => format!("{} {}", IDLE_LABEL, format_hm(idle)),
                };
                let needed = s(12) + p.text_width(&label, &small_font);
                let fits = x + needed <= right;
                if !fits && right - x < s(60) {
                    p.text("…", rect(x, legend_top, right, legend_bottom), &small_font, t.subtext, DT_LEFT | DT_VCENTER);
                    break;
                }
                let marker_top = (legend_top + legend_bottom - s(8)) / 2;
                let marker = rect(x, marker_top, x + s(8), marker_top + s(8));
                match item {
                    Some(task) => p.fill_round(marker, 2, task_color(task)),
                    None => bar::draw_idle_marker(p, marker),
                }
                let label_rect = rect(x + s(12), legend_top, (x + needed).min(right), legend_bottom);
                p.text(&label, label_rect, &small_font, t.subtext, DT_LEFT | DT_VCENTER | DT_END_ELLIPSIS);
                if !fits {
                    break;
                }
                x += needed + s(14);
            }
        }
    });
}

pub unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_SIZE => {
            update_scrollbar(hwnd);
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_ACTIVATE => {
            if wparam & 0xFFFF != 0 {
                reload_records();
                update_scrollbar(hwnd);
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_VSCROLL => {
            let scale = Scale(GetDpiForWindow(hwnd));
            let page = client_height(hwnd);
            match (wparam & 0xFFFF) as i32 {
                SB_LINEUP => scroll_by(hwnd, -scale.px(40)),
                SB_LINEDOWN => scroll_by(hwnd, scale.px(40)),
                SB_PAGEUP => scroll_by(hwnd, -page),
                SB_PAGEDOWN => scroll_by(hwnd, page),
                SB_TOP => scroll_to(hwnd, 0),
                SB_BOTTOM => scroll_to(hwnd, i32::MAX),
                SB_THUMBTRACK | SB_THUMBPOSITION => {
                    let mut info: SCROLLINFO = zeroed();
                    info.cbSize = size_of::<SCROLLINFO>() as u32;
                    info.fMask = SIF_TRACKPOS;
                    GetScrollInfo(hwnd, SB_VERT, &mut info);
                    scroll_to(hwnd, info.nTrackPos);
                }
                _ => {}
            }
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) & 0xFFFF) as u16 as i16 as i32;
            let scale = Scale(GetDpiForWindow(hwnd));
            scroll_by(hwnd, -delta * scale.px(ROW_H) / 120);
            0
        }
        WM_KEYDOWN => {
            let scale = Scale(GetDpiForWindow(hwnd));
            let page = client_height(hwnd);
            match wparam as u16 {
                VK_UP => scroll_by(hwnd, -scale.px(40)),
                VK_DOWN => scroll_by(hwnd, scale.px(40)),
                VK_PRIOR => scroll_by(hwnd, -page),
                VK_NEXT => scroll_by(hwnd, page),
                VK_HOME => scroll_to(hwnd, 0),
                VK_END => scroll_to(hwnd, i32::MAX),
                VK_ESCAPE => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_GETMINMAXINFO => {
            let scale = Scale(GetDpiForWindow(hwnd));
            let info = &mut *(lparam as *mut MINMAXINFO);
            info.ptMinTrackSize.x = scale.px(420);
            info.ptMinTrackSize.y = scale.px(300);
            0
        }
        WM_DPICHANGED => {
            let suggested = &*(lparam as *const RECT);
            SetWindowPos(
                hwnd,
                null_mut(),
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            0
        }
        WM_SETTINGCHANGE => {
            apply_title_bar_theme(hwnd);
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_DESTROY => {
            with_state(|s| s.history = null_mut());
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
