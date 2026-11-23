//! Ayudantes para diálogos con controles Win32 estándar.

use std::ffi::c_void;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH,
    FF_DONTCARE, HFONT, OUT_DEFAULT_PRECIS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, GetWindowTextLengthW, GetWindowTextW, HMENU, MB_ICONWARNING, MB_OK,
    MB_SETFOREGROUND, MESSAGEBOX_STYLE, MessageBoxW, SendMessageW, SetWindowTextW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_SETFONT,
};
use windows::core::{HSTRING, PCWSTR, w};

/// Estilo `SS_CENTERIMAGE`, no exportado por el crate `windows`.
pub const SS_CENTERIMAGE: WINDOW_STYLE = WINDOW_STYLE(0x0200);

pub fn rect(x: i32, y: i32, width: i32, height: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}

/// Fuente `Segoe UI` del tamaño indicado (en píxeles negativos, como `CreateFontW`).
pub fn font(size: i32, weight: i32) -> HFONT {
    unsafe {
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
}

/// Tamaño de ventana (incluye el marco) para un área cliente dada.
pub fn window_size(
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    width: i32,
    height: i32,
    dpi: u32,
) -> (i32, i32) {
    let mut frame = rect(0, 0, width, height);
    let _ = unsafe { AdjustWindowRectExForDpi(&mut frame, style, false, ex_style, dpi) };
    (frame.right - frame.left, frame.bottom - frame.top)
}

/// Envoltura directa de `CreateWindowExW`: una firma con muchos parámetros es lo natural.
#[allow(clippy::too_many_arguments)]
pub unsafe fn child(
    parent: HWND,
    class: PCWSTR,
    text: &str,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    area: RECT,
    id: i32,
    font: HFONT,
) -> Option<HWND> {
    let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }.ok()?.0);
    let text = HSTRING::from(text);
    let handle = unsafe {
        CreateWindowExW(
            ex_style,
            class,
            &text,
            style,
            area.left,
            area.top,
            area.right - area.left,
            area.bottom - area.top,
            parent,
            HMENU(id as isize as *mut c_void),
            instance,
            None,
        )
    }
    .ok()?;
    unsafe { SendMessageW(handle, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1)) };
    Some(handle)
}

pub unsafe fn set_text(hwnd: HWND, text: &str) {
    let text = HSTRING::from(text);
    let _ = unsafe { SetWindowTextW(hwnd, &text) };
}

pub unsafe fn get_text(hwnd: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(hwnd) }.max(0) as usize;
    let mut buffer = vec![0u16; length + 1];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..copied])
        .trim()
        .to_string()
}

pub unsafe fn warn(owner: HWND, title: &str, body: &str) {
    let title = HSTRING::from(title);
    let body = HSTRING::from(body);
    unsafe {
        MessageBoxW(
            owner,
            &body,
            &title,
            MESSAGEBOX_STYLE(MB_OK.0 | MB_ICONWARNING.0 | MB_SETFOREGROUND.0),
        );
    }
}
