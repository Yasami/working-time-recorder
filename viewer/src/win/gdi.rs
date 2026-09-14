//! GDI 描画ヘルパー、配色、アイコン生成

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::core::w;
use windows_sys::Win32::Foundation::{COLORREF, ERROR_SUCCESS, HWND, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, GetClientRect, HICON, LR_DEFAULTCOLOR,
};

pub const fn rgb(hex: u32) -> COLORREF {
    ((hex >> 16) & 0xFF) | (hex & 0xFF00) | ((hex & 0xFF) << 16)
}

const PALETTE: [u32; 10] = [
    0x4E79A7, 0xF28E2B, 0xE15759, 0x76B7B2, 0x59A14F, 0xEDC948, 0xB07AA1, 0xFF9DA7, 0x9C755F,
    0xBAB0AC,
];

pub fn task_color(index: usize) -> COLORREF {
    rgb(PALETTE[index % PALETTE.len()])
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub dark: bool,
    pub bg: COLORREF,
    pub text: COLORREF,
    pub subtext: COLORREF,
    pub track: COLORREF,
    pub tick: COLORREF,
    pub separator: COLORREF,
    pub button: COLORREF,
    pub button_hover: COLORREF,
    pub active: COLORREF,
    pub danger: COLORREF,
}

const LIGHT: Theme = Theme {
    dark: false,
    bg: rgb(0xF9F9F9),
    text: rgb(0x1A1A1A),
    subtext: rgb(0x5F5F5F),
    track: rgb(0xE6E6E6),
    tick: rgb(0xD2D2D2),
    separator: rgb(0xE3E3E3),
    button: rgb(0xEBEBEB),
    button_hover: rgb(0xDDDDDD),
    active: rgb(0x2E7D32),
    danger: rgb(0xC42B1C),
};

const DARK: Theme = Theme {
    dark: true,
    bg: rgb(0x2B2B2B),
    text: rgb(0xFFFFFF),
    subtext: rgb(0xB3B3B3),
    track: rgb(0x3E3E3E),
    tick: rgb(0x535353),
    separator: rgb(0x3A3A3A),
    button: rgb(0x3A3A3A),
    button_hover: rgb(0x474747),
    active: rgb(0x6CCB5F),
    danger: rgb(0xFF99A4),
};

impl Theme {
    /// Windows のアプリモード (ライト / ダーク) に合わせる
    pub fn current() -> Theme {
        let mut value: u32 = 1;
        let mut size = size_of::<u32>() as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                w!("AppsUseLightTheme"),
                RRF_RT_REG_DWORD,
                null_mut(),
                &mut value as *mut u32 as *mut c_void,
                &mut size,
            )
        };
        if status == ERROR_SUCCESS && value == 0 {
            DARK
        } else {
            LIGHT
        }
    }
}

/// DIP (96 DPI 基準) からピクセルへの変換
#[derive(Clone, Copy)]
pub struct Scale(pub u32);

impl Scale {
    pub fn px(self, dip: i32) -> i32 {
        ((dip as i64 * self.0 as i64 + 48).div_euclid(96)) as i32
    }
}

pub fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT { left, top, right, bottom }
}

pub struct Font(HFONT);

impl Font {
    pub fn new(scale: Scale, size_dip: i32, bold: bool) -> Font {
        let weight = if bold { FW_SEMIBOLD } else { FW_NORMAL };
        Font(unsafe {
            CreateFontW(
                -scale.px(size_dip),
                0,
                0,
                0,
                weight as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                (DEFAULT_PITCH as u32) | (FF_DONTCARE as u32),
                w!("Yu Gothic UI"),
            )
        })
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0 as HGDIOBJ);
        }
    }
}

pub struct Painter {
    pub hdc: HDC,
    pub scale: Scale,
    pub theme: Theme,
}

impl Painter {
    pub fn px(&self, dip: i32) -> i32 {
        self.scale.px(dip)
    }

    pub fn fill(&self, r: RECT, color: COLORREF) {
        unsafe {
            let brush = CreateSolidBrush(color);
            FillRect(self.hdc, &r, brush);
            DeleteObject(brush as HGDIOBJ);
        }
    }

    pub fn fill_round(&self, r: RECT, radius_dip: i32, color: COLORREF) {
        let diameter = self.px(radius_dip * 2);
        unsafe {
            let brush = CreateSolidBrush(color);
            let old_brush = SelectObject(self.hdc, brush as HGDIOBJ);
            let old_pen = SelectObject(self.hdc, GetStockObject(NULL_PEN));
            // NULL_PEN だと右端・下端が 1px 欠けるので広げる
            RoundRect(self.hdc, r.left, r.top, r.right + 1, r.bottom + 1, diameter, diameter);
            SelectObject(self.hdc, old_pen);
            SelectObject(self.hdc, old_brush);
            DeleteObject(brush as HGDIOBJ);
        }
    }

    /// 角丸矩形でクリップして描画する
    pub fn clip_round(&self, r: RECT, radius_dip: i32, draw: impl FnOnce()) {
        let diameter = self.px(radius_dip * 2);
        unsafe {
            let region = CreateRoundRectRgn(r.left, r.top, r.right + 1, r.bottom + 1, diameter, diameter);
            SelectClipRgn(self.hdc, region);
            draw();
            SelectClipRgn(self.hdc, null_mut());
            DeleteObject(region as HGDIOBJ);
        }
    }

    pub fn text(&self, s: &str, r: RECT, font: &Font, color: COLORREF, format: DRAW_TEXT_FORMAT) {
        let mut buffer: Vec<u16> = s.encode_utf16().collect();
        let mut r = r;
        unsafe {
            SelectObject(self.hdc, font.0 as HGDIOBJ);
            SetTextColor(self.hdc, color);
            DrawTextW(
                self.hdc,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
                &mut r,
                format | DT_SINGLELINE | DT_NOPREFIX,
            );
        }
    }

    pub fn text_width(&self, s: &str, font: &Font) -> i32 {
        let buffer: Vec<u16> = s.encode_utf16().collect();
        let mut size = SIZE { cx: 0, cy: 0 };
        unsafe {
            SelectObject(self.hdc, font.0 as HGDIOBJ);
            GetTextExtentPoint32W(self.hdc, buffer.as_ptr(), buffer.len() as i32, &mut size);
        }
        size.cx
    }
}

/// ちらつき防止のためメモリ DC に描いてから転送する
pub fn paint_buffered(hwnd: HWND, draw: impl FnOnce(&Painter, RECT)) {
    unsafe {
        let mut ps: PAINTSTRUCT = zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut client: RECT = zeroed();
        GetClientRect(hwnd, &mut client);
        let width = (client.right - client.left).max(1);
        let height = (client.bottom - client.top).max(1);

        let memory = CreateCompatibleDC(hdc);
        let bitmap = CreateCompatibleBitmap(hdc, width, height);
        let old_bitmap = SelectObject(memory, bitmap as HGDIOBJ);
        let old_font = GetCurrentObject(memory, OBJ_FONT as u32);
        SetBkMode(memory, TRANSPARENT as _);

        let painter = Painter {
            hdc: memory,
            scale: Scale(GetDpiForWindow(hwnd)),
            theme: Theme::current(),
        };
        draw(&painter, client);

        BitBlt(hdc, 0, 0, width, height, memory, 0, 0, SRCCOPY);
        SelectObject(memory, old_font);
        SelectObject(memory, old_bitmap);
        DeleteObject(bitmap as HGDIOBJ);
        DeleteDC(memory);
        EndPaint(hwnd, &ps);
    }
}

/// 角丸の四角に色分けされた横バーを描いたアイコン
pub fn create_app_icon(size: i32) -> HICON {
    let s = size.max(16) as usize;
    let mut data: Vec<u8> = Vec::with_capacity(40 + s * s * 5);

    // BITMAPINFOHEADER (高さは XOR + AND で 2 倍)
    data.extend_from_slice(&40u32.to_le_bytes());
    data.extend_from_slice(&(s as i32).to_le_bytes());
    data.extend_from_slice(&((s * 2) as i32).to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&32u16.to_le_bytes());
    data.extend_from_slice(&[0u8; 24]);

    let size_f = s as f32;
    let radius = size_f * 0.22;
    let (bar_left, bar_right) = (size_f * 0.17, size_f * 0.83);
    let (bar_top, bar_bottom) = (size_f * 0.36, size_f * 0.64);
    let segments = [(0.45, 0xF28E2B), (0.72, 0x59A14F), (0.88, 0xEDC948), (1.0, 0x7F8C8D)];

    // ピクセルは下の行から
    for y in (0..s).rev() {
        for x in 0..s {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let dx = (radius - px).max(px - (size_f - radius)).max(0.0);
            let dy = (radius - py).max(py - (size_f - radius)).max(0.0);
            let argb: u32 = if dx * dx + dy * dy > radius * radius {
                0x00000000
            } else if px >= bar_left && px < bar_right && py >= bar_top && py < bar_bottom {
                let fraction = (px - bar_left) / (bar_right - bar_left);
                let color = segments.iter().find(|(end, _)| fraction < *end).map_or(0x7F8C8D, |s| s.1);
                0xFF000000 | color
            } else {
                0xFF34495E
            };
            data.extend_from_slice(&argb.to_le_bytes()); // BGRA
        }
    }

    // AND マスク (透明度はアルファで表すので全て 0)
    let mask_row = s.div_ceil(32) * 4;
    data.resize(data.len() + mask_row * s, 0);

    unsafe {
        CreateIconFromResourceEx(
            data.as_ptr(),
            data.len() as u32,
            1,
            0x0003_0000,
            s as i32,
            s as i32,
            LR_DEFAULTCOLOR,
        )
    }
}
