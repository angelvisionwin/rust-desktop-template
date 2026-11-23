use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;

use windows::Win32::Foundation::{BOOL, COLORREF, HANDLE, POINT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateCompatibleDC, CreateDIBSection,
    CreatePen, CreateSolidBrush, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetStockObject, HGDIOBJ,
    NULL_BRUSH, PS_SOLID, Polygon, RoundRect, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

const CANVAS: i32 = 160;
const OUTPUT: i32 = 32;

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

fn level_color(percent: Option<u8>) -> COLORREF {
    match percent {
        None => rgb(0x8A, 0x8A, 0x92),
        Some(p) if p >= 50 => rgb(0x3F, 0xB9, 0x50),
        Some(p) if p >= 20 => rgb(0xD2, 0x9E, 0x22),
        Some(_) => rgb(0xF8, 0x51, 0x49),
    }
}

pub fn create(percent: Option<u8>, charging: bool) -> Option<HICON> {
    unsafe { build(percent, charging) }
}

unsafe fn build(percent: Option<u8>, charging: bool) -> Option<HICON> {
    let color = level_color(percent);

    let mut info = BITMAPINFO::default();
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = CANVAS;
    info.bmiHeader.biHeight = -CANVAS;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB.0;

    let canvas_dc = CreateCompatibleDC(None);
    let mut canvas_bits: *mut c_void = null_mut();
    let canvas = CreateDIBSection(
        canvas_dc,
        &info,
        DIB_RGB_COLORS,
        &mut canvas_bits,
        HANDLE(null_mut()),
        0,
    )
    .ok()?;
    let old_canvas = SelectObject(canvas_dc, HGDIOBJ(canvas.0));
    std::ptr::write_bytes(canvas_bits as *mut u8, 0, (CANVAS * CANVAS * 4) as usize);

    let outline = CreatePen(PS_SOLID, 12, color);
    let old_pen = SelectObject(canvas_dc, HGDIOBJ(outline.0));
    let old_brush = SelectObject(canvas_dc, GetStockObject(NULL_BRUSH));
    let _ = RoundRect(canvas_dc, 22, 40, 128, 128, 26, 26);
    SelectObject(canvas_dc, old_brush);
    SelectObject(canvas_dc, old_pen);
    let _ = DeleteObject(HGDIOBJ(outline.0));

    if charging {
        // Un rayo sustituye a la barra: se distingue incluso a 16 px.
        fill_bolt(canvas_dc, rgb(0xF2, 0xB1, 0x05));
    } else if let Some(pct) = percent
        && pct > 0
    {
        let bottom = 116;
        let top = bottom - ((bottom - 52) * pct as i32 / 100).max(4);
        fill_round(canvas_dc, 34, top, 116, bottom, 14, color);
    }
    fill_round(canvas_dc, 128, 66, 148, 104, 8, color);

    // GDI no escribe el canal alfa: marcamos opaco todo lo pintado.
    let bytes = canvas_bits as *mut u8;
    for i in 0..(CANVAS * CANVAS) as usize {
        let b = *bytes.add(i * 4);
        let g = *bytes.add(i * 4 + 1);
        let r = *bytes.add(i * 4 + 2);
        if (r | g | b) != 0 {
            *bytes.add(i * 4 + 3) = 255;
        }
    }

    let factor = (CANVAS / OUTPUT) as usize;
    let mut small = vec![0u8; (OUTPUT * OUTPUT * 4) as usize];
    for oy in 0..OUTPUT as usize {
        for ox in 0..OUTPUT as usize {
            let mut sum = [0u32; 3];
            let mut count = 0u32;
            for dy in 0..factor {
                for dx in 0..factor {
                    let sx = ox * factor + dx;
                    let sy = oy * factor + dy;
                    let i = (sy * CANVAS as usize + sx) * 4;
                    if *bytes.add(i + 3) != 0 {
                        sum[0] += *bytes.add(i + 2) as u32;
                        sum[1] += *bytes.add(i + 1) as u32;
                        sum[2] += *bytes.add(i) as u32;
                        count += 1;
                    }
                }
            }
            let o = (oy * OUTPUT as usize + ox) * 4;
            let divisor = count.max(1);
            small[o] = (sum[2] / divisor) as u8;
            small[o + 1] = (sum[1] / divisor) as u8;
            small[o + 2] = (sum[0] / divisor) as u8;
            if count > 0 {
                let coverage = count as f32 / (factor * factor) as f32;
                small[o + 3] = (coverage * 255.0).round() as u8;
            }
        }
    }

    let mut small_info = info;
    small_info.bmiHeader.biWidth = OUTPUT;
    small_info.bmiHeader.biHeight = -OUTPUT;
    let icon_dc = CreateCompatibleDC(None);
    let mut icon_bits: *mut c_void = null_mut();
    let icon_bitmap = CreateDIBSection(
        icon_dc,
        &small_info,
        DIB_RGB_COLORS,
        &mut icon_bits,
        HANDLE(null_mut()),
        0,
    )
    .ok()?;
    std::ptr::copy_nonoverlapping(small.as_ptr(), icon_bits as *mut u8, small.len());

    let mask_bytes = vec![0u8; (OUTPUT * OUTPUT / 8) as usize];
    let mask = CreateBitmap(
        OUTPUT,
        OUTPUT,
        1,
        1,
        Some(mask_bytes.as_ptr() as *const c_void),
    );
    let icon_info = ICONINFO {
        fIcon: BOOL(1),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: icon_bitmap,
    };
    let icon = CreateIconIndirect(&icon_info).ok();

    let _ = DeleteObject(HGDIOBJ(icon_bitmap.0));
    let _ = DeleteObject(HGDIOBJ(mask.0));
    let _ = DeleteDC(icon_dc);
    SelectObject(canvas_dc, old_canvas);
    let _ = DeleteObject(HGDIOBJ(canvas.0));
    let _ = DeleteDC(canvas_dc);

    icon
}

unsafe fn fill_bolt(dc: windows::Win32::Graphics::Gdi::HDC, color: COLORREF) {
    let points = [
        POINT { x: 86, y: 46 },
        POINT { x: 48, y: 94 },
        POINT { x: 74, y: 94 },
        POINT { x: 62, y: 124 },
        POINT { x: 98, y: 76 },
        POINT { x: 72, y: 76 },
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

unsafe fn fill_round(
    dc: windows::Win32::Graphics::Gdi::HDC,
    l: i32,
    t: i32,
    r: i32,
    b: i32,
    radius: i32,
    color: COLORREF,
) {
    let brush = CreateSolidBrush(color);
    let pen = CreatePen(PS_SOLID, 1, color);
    let old_brush = SelectObject(dc, HGDIOBJ(brush.0));
    let old_pen = SelectObject(dc, HGDIOBJ(pen.0));
    let _ = RoundRect(dc, l, t, r, b, radius, radius);
    SelectObject(dc, old_pen);
    SelectObject(dc, old_brush);
    let _ = DeleteObject(HGDIOBJ(pen.0));
    let _ = DeleteObject(HGDIOBJ(brush.0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;

    #[test]
    fn creates_icons_for_all_states() {
        for (percent, charging) in [
            (None, false),
            (Some(0), false),
            (Some(55), false),
            (Some(12), false),
            (Some(100), false),
            (Some(80), true),
            (None, true),
        ] {
            let icon = create(percent, charging);
            assert!(
                icon.is_some(),
                "no se pudo crear el icono {percent:?}/{charging}"
            );
            if let Some(icon) = icon {
                unsafe {
                    let _ = DestroyIcon(icon);
                }
            }
        }
    }
}
