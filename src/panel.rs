use std::mem::size_of;

use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateCompatibleBitmap,
    CreateCompatibleDC, CreateFontW, CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH,
    DRAW_TEXT_FORMAT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE,
    DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, Ellipse, FF_DONTCARE, FillRect, GetMonitorInfoW,
    GetStockObject, HDC, HFONT, HGDIOBJ, InvalidateRect, LineTo, MONITOR_DEFAULTTONEAREST,
    MONITORINFO, MonitorFromPoint, MoveToEx, NULL_BRUSH, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID,
    Polygon, Rectangle, RoundRect, SRCCOPY, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetWindowRect, HWND_TOPMOST, SW_SHOW, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SetForegroundWindow, SetWindowPos, ShowWindow,
};
use windows::core::w;

use crate::battery::{DeviceBattery, DeviceKind};
use crate::i18n;
use crate::util;

const HEADER: f32 = 54.0;
const ROW: f32 = 62.0;
const FOOTER: f32 = 34.0;
const WIDTH: f32 = 360.0;

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

fn level_color(percent: u8) -> COLORREF {
    if percent >= 50 {
        rgb(0x3F, 0xB9, 0x50)
    } else if percent >= 20 {
        rgb(0xD2, 0x9E, 0x22)
    } else {
        rgb(0xF8, 0x51, 0x49)
    }
}

struct Theme {
    bg: COLORREF,
    text: COLORREF,
    sub: COLORREF,
    bar: COLORREF,
    border: COLORREF,
}

#[derive(Clone, Copy)]
struct Fonts {
    name: HFONT,
    small: HFONT,
    percent: HFONT,
}

fn theme(dark: bool) -> Theme {
    if dark {
        Theme {
            bg: rgb(0x25, 0x25, 0x28),
            text: rgb(0xF2, 0xF2, 0xF4),
            sub: rgb(0x9C, 0x9C, 0xA4),
            bar: rgb(0x3C, 0x3C, 0x42),
            border: rgb(0x3C, 0x3C, 0x42),
        }
    } else {
        Theme {
            bg: rgb(0xFB, 0xFB, 0xFC),
            text: rgb(0x1B, 0x1B, 0x1F),
            sub: rgb(0x6C, 0x6C, 0x74),
            bar: rgb(0xE7, 0xE7, 0xEC),
            border: rgb(0xDC, 0xDC, 0xE2),
        }
    }
}

pub fn preferred_size(count: usize, dpi: u32) -> (i32, i32) {
    let scale = (dpi as f32 / 96.0).max(1.0);
    let rows = count.max(1) as f32;
    (
        (WIDTH * scale).round() as i32,
        ((HEADER + ROW * rows + FOOTER) * scale).round() as i32,
    )
}

pub unsafe fn show(hwnd: HWND, count: usize) {
    let dpi = {
        let dpi = GetDpiForWindow(hwnd);
        if dpi == 0 { 96 } else { dpi }
    };
    let (width, height) = preferred_size(count, dpi);

    let mut point = POINT::default();
    let _ = GetCursorPos(&mut point);
    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
    let mut monitor_info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let _ = GetMonitorInfoW(monitor, &mut monitor_info);
    let work = monitor_info.rcWork;

    let mut x = point.x - width - 12;
    if x < work.left + 8 {
        x = work.left + 8;
    }
    if x + width > work.right - 8 {
        x = work.right - width - 8;
    }
    let y = (work.bottom - height - 8).max(work.top + 8);

    let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    let _ = InvalidateRect(hwnd, None, true);
}

pub unsafe fn resize(hwnd: HWND, count: usize) {
    let dpi = {
        let dpi = GetDpiForWindow(hwnd);
        if dpi == 0 { 96 } else { dpi }
    };
    let (width, height) = preferred_size(count, dpi);
    let mut rect = RECT::default();
    let _ = GetWindowRect(hwnd, &mut rect);
    let current_width = rect.right - rect.left;
    let current_height = rect.bottom - rect.top;
    if current_width == width && current_height == height {
        return;
    }
    let _ = SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        rect.right - width,
        rect.bottom - height,
        width,
        height,
        SWP_NOACTIVATE,
    );
}

pub unsafe fn paint(hwnd: HWND, devices: &[DeviceBattery], updated: Option<(u16, u16)>) {
    let mut paint_struct = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut paint_struct);
    render(hwnd, hdc, devices, updated);
    let _ = windows::Win32::Graphics::Gdi::EndPaint(hwnd, &paint_struct);
}

pub unsafe fn paint_to(
    hwnd: HWND,
    hdc: HDC,
    devices: &[DeviceBattery],
    updated: Option<(u16, u16)>,
) {
    render(hwnd, hdc, devices, updated);
}

unsafe fn render(hwnd: HWND, hdc: HDC, devices: &[DeviceBattery], updated: Option<(u16, u16)>) {
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let (width, height) = (client.right, client.bottom);
    if width <= 0 || height <= 0 {
        return;
    }

    let memory = CreateCompatibleDC(hdc);
    let bitmap = CreateCompatibleBitmap(hdc, width, height);

    let old_bitmap = SelectObject(memory, HGDIOBJ(bitmap.0));

    let colors = theme(util::is_dark_mode());
    let text_strings = i18n::text();
    let dpi = {
        let dpi = GetDpiForWindow(hwnd);
        if dpi == 0 { 96 } else { dpi }
    };
    let scale = dpi as f32 / 96.0;
    let px = |value: f32| (value * scale).round() as i32;

    fill_rect(
        memory,
        RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        },
        colors.bg,
    );

    let title_font = font(px(16.0), 600);
    let name_font = font(px(13.5), 600);
    let small_font = font(px(10.5), 400);
    let percent_font = font(px(15.0), 700);

    let mut title = RECT {
        left: px(18.0),
        top: px(14.0),
        right: width - px(150.0),
        bottom: px(14.0) + px(22.0),
    };
    text(
        memory,
        text_strings.app,
        &mut title,
        title_font,
        colors.text,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    if let Some((hours, minutes)) = updated {
        let mut clock = RECT {
            left: width - px(160.0),
            top: px(18.0),
            right: width - px(18.0),
            bottom: px(18.0) + px(16.0),
        };
        text(
            memory,
            &text_strings.updated_at(hours, minutes),
            &mut clock,
            small_font,
            colors.sub,
            DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    }

    fill_rect(
        memory,
        RECT {
            left: px(18.0),
            top: px(46.0),
            right: width - px(18.0),
            bottom: px(47.0),
        },
        colors.border,
    );

    let fonts = Fonts {
        name: name_font,
        small: small_font,
        percent: percent_font,
    };

    let mut y = px(HEADER);
    if devices.is_empty() {
        let mut empty = RECT {
            left: px(20.0),
            top: y + px(12.0),
            right: width - px(20.0),
            bottom: y + px(36.0),
        };
        text(
            memory,
            text_strings.empty_title,
            &mut empty,
            name_font,
            colors.text,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
        let mut hint = RECT {
            left: px(20.0),
            top: y + px(38.0),
            right: width - px(20.0),
            bottom: y + px(60.0),
        };
        text(
            memory,
            text_strings.empty_hint,
            &mut hint,
            small_font,
            colors.sub,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    } else {
        for device in devices {
            draw_row(memory, &colors, scale, width, y, device, fonts);
            y += px(ROW);
        }
    }

    let mut footer = RECT {
        left: px(18.0),
        top: height - px(26.0),
        right: width - px(18.0),
        bottom: height - px(8.0),
    };
    text(
        memory,
        text_strings.footer,
        &mut footer,
        small_font,
        colors.sub,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let _ = BitBlt(hdc, 0, 0, width, height, memory, 0, 0, SRCCOPY);

    SelectObject(memory, old_bitmap);
    let _ = DeleteObject(HGDIOBJ(bitmap.0));
    let _ = DeleteDC(memory);
    for handle in [title_font, name_font, small_font, percent_font] {
        let _ = DeleteObject(HGDIOBJ(handle.0));
    }
}

unsafe fn draw_row(
    dc: HDC,
    colors: &Theme,
    scale: f32,
    width: i32,
    y: i32,
    device: &DeviceBattery,
    fonts: Fonts,
) {
    let px = |value: f32| (value * scale).round() as i32;
    let color = level_color(device.percent);

    let glyph = RECT {
        left: px(18.0),
        top: y + px(16.0),
        right: px(18.0) + px(26.0),
        bottom: y + px(16.0) + px(26.0),
    };
    draw_glyph(dc, device.kind, glyph, colors.sub, scale);

    let mut name = RECT {
        left: px(54.0),
        top: y + px(7.0),
        right: width - px(86.0),
        bottom: y + px(7.0) + px(20.0),
    };
    text(
        dc,
        &device.name,
        &mut name,
        fonts.name,
        colors.text,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );

    if device.plugged() {
        draw_bolt(
            dc,
            RECT {
                left: width - px(104.0),
                top: y + px(9.0),
                right: width - px(104.0) + px(11.0),
                bottom: y + px(9.0) + px(15.0),
            },
            color,
        );
    }

    let label = if device.approx {
        format!("≈{}%", device.percent)
    } else {
        format!("{}%", device.percent)
    };
    let mut percent = RECT {
        left: width - px(84.0),
        top: y + px(6.0),
        right: width - px(18.0),
        bottom: y + px(6.0) + px(22.0),
    };
    text(
        dc,
        &label,
        &mut percent,
        fonts.percent,
        color,
        DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );

    let mut detail = RECT {
        left: px(54.0),
        top: y + px(28.0),
        right: width - px(18.0),
        bottom: y + px(28.0) + px(15.0),
    };
    let strings = i18n::text();
    let mut parts = vec![strings.kind(device.kind).to_string()];
    if device.wired {
        parts.push(format!("{} · {}", device.source, strings.charge_cable));
    } else {
        parts.push(device.source.clone());
    }
    if let Some(state) = device.charge {
        parts.push(strings.charge(state).to_string());
    } else if let Some(left) = device.time_left {
        let remaining = strings.remaining_text(left);
        parts.push(if device.approx {
            format!("{} {remaining}", strings.approx)
        } else {
            remaining
        });
    }
    let detail_text = parts.join(" · ");
    text(
        dc,
        &detail_text,
        &mut detail,
        fonts.small,
        colors.sub,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );

    let bar = RECT {
        left: px(54.0),
        top: y + px(47.0),
        right: width - px(18.0),
        bottom: y + px(47.0) + px(6.0),
    };
    round_fill(dc, bar, px(3.0), colors.bar);
    if device.percent > 0 {
        let total = bar.right - bar.left;
        let filled = ((total as f32) * device.percent as f32 / 100.0)
            .max(px(4.0) as f32)
            .min(total as f32) as i32;
        round_fill(
            dc,
            RECT {
                left: bar.left,
                top: bar.top,
                right: bar.left + filled,
                bottom: bar.bottom,
            },
            px(3.0),
            color,
        );
    }
}

unsafe fn draw_glyph(dc: HDC, kind: DeviceKind, rect: RECT, color: COLORREF, scale: f32) {
    let px = |value: f32| (value * scale).round() as i32;
    let pen = CreatePen(PS_SOLID, (1.6 * scale).max(1.0) as i32, color);
    let old_pen = SelectObject(dc, HGDIOBJ(pen.0));
    let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
    let (left, top) = (rect.left, rect.top);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;

    match kind {
        DeviceKind::Keyboard => {
            let _ = RoundRect(
                dc,
                left,
                top + height / 4,
                rect.right,
                rect.bottom - height / 5,
                px(3.0),
                px(3.0),
            );
            for index in 1..4 {
                let cx = left + width * index / 4;
                let _ = Rectangle(
                    dc,
                    cx,
                    top + height / 2 - px(1.0),
                    cx + px(2.0),
                    top + height / 2 + px(1.0),
                );
            }
        }
        DeviceKind::Mouse => {
            let body = width * 2 / 3;
            let cx = left + (width - body) / 2;
            let _ = RoundRect(dc, cx, top, cx + body, rect.bottom, px(6.0), px(6.0));
            let _ = MoveToEx(dc, cx + body / 2, top, None);
            let _ = LineTo(dc, cx + body / 2, top + height / 3);
        }
        DeviceKind::Gamepad => {
            let _ = RoundRect(
                dc,
                left,
                top + height / 4,
                rect.right,
                rect.bottom - height / 5,
                px(6.0),
                px(6.0),
            );
            let cx = left + width / 4;
            let cy = top + height / 2;
            let _ = Ellipse(dc, cx - px(2.0), cy - px(2.0), cx + px(2.0), cy + px(2.0));
        }
        DeviceKind::Headset => {
            let _ = RoundRect(
                dc,
                left + width / 6,
                top,
                rect.right - width / 6,
                top + height / 2,
                px(8.0),
                px(8.0),
            );
            let _ = RoundRect(
                dc,
                left,
                top + height / 3,
                left + width / 4,
                rect.bottom,
                px(2.0),
                px(2.0),
            );
            let _ = RoundRect(
                dc,
                rect.right - width / 4,
                top + height / 3,
                rect.right,
                rect.bottom,
                px(2.0),
                px(2.0),
            );
        }
        DeviceKind::Other => {
            let _ = Ellipse(
                dc,
                left + width / 6,
                top + height / 6,
                rect.right - width / 6,
                rect.bottom - height / 6,
            );
        }
    }

    SelectObject(dc, old_brush);
    SelectObject(dc, old_pen);
    let _ = DeleteObject(HGDIOBJ(pen.0));
}

unsafe fn draw_bolt(dc: HDC, rect: RECT, color: COLORREF) {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let points = [
        POINT {
            x: rect.left + width * 62 / 100,
            y: rect.top,
        },
        POINT {
            x: rect.left + width * 15 / 100,
            y: rect.top + height * 58 / 100,
        },
        POINT {
            x: rect.left + width * 45 / 100,
            y: rect.top + height * 58 / 100,
        },
        POINT {
            x: rect.left + width * 35 / 100,
            y: rect.bottom,
        },
        POINT {
            x: rect.left + width * 88 / 100,
            y: rect.top + height * 38 / 100,
        },
        POINT {
            x: rect.left + width * 58 / 100,
            y: rect.top + height * 38 / 100,
        },
    ];
    let brush = CreateSolidBrush(color);
    let pen = CreatePen(PS_SOLID, 1, color);
    let old_brush = SelectObject(dc, HGDIOBJ(brush.0));
    let old_pen = SelectObject(dc, HGDIOBJ(pen.0));
    let _ = Polygon(dc, &points);
    SelectObject(dc, old_pen);
    SelectObject(dc, old_brush);
    let _ = DeleteObject(HGDIOBJ(pen.0));
    let _ = DeleteObject(HGDIOBJ(brush.0));
}

unsafe fn fill_rect(dc: HDC, rect: RECT, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    FillRect(dc, &rect, brush);
    let _ = DeleteObject(HGDIOBJ(brush.0));
}

unsafe fn round_fill(dc: HDC, rect: RECT, radius: i32, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    let pen = CreatePen(PS_SOLID, 1, color);
    let old_brush = SelectObject(dc, HGDIOBJ(brush.0));
    let old_pen = SelectObject(dc, HGDIOBJ(pen.0));
    let _ = RoundRect(
        dc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius,
        radius,
    );
    SelectObject(dc, old_pen);
    SelectObject(dc, old_brush);
    let _ = DeleteObject(HGDIOBJ(pen.0));
    let _ = DeleteObject(HGDIOBJ(brush.0));
}

unsafe fn text(
    dc: HDC,
    value: &str,
    rect: &mut RECT,
    font: HFONT,
    color: COLORREF,
    flags: DRAW_TEXT_FORMAT,
) {
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    SelectObject(dc, HGDIOBJ(font.0));
    SetTextColor(dc, color);
    SetBkMode(dc, TRANSPARENT);
    DrawTextW(dc, &mut wide, rect, flags);
}

unsafe fn font(size: i32, weight: i32) -> HFONT {
    CreateFontW(
        -size,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        w!("Segoe UI"),
    )
}
