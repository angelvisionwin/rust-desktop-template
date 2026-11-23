//! Bandeja del sistema: icono, tooltip, globo y menú contextual.
//!
//! No conoce la aplicación: recibe handles, textos y entradas de menú.

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_NONE, NIIF_WARNING, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NOTIFY_ICON_DATA_FLAGS, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetSystemMetrics, HICON, IMAGE_ICON,
    LR_DEFAULTCOLOR, LoadImageW, MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, PostMessageW,
    SM_CXSMICON, SM_CYSMICON, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
    WM_NULL,
};
use windows::core::{HSTRING, PCWSTR};

/// Identificador del icono dentro de la bandeja.
const TRAY_ID: u32 = 1;

/// Icono incrustado por `build.rs` con el identificador de recurso 1.
///
/// No es un puntero real: es el valor que Win32 interpreta como `MAKEINTRESOURCE(1)`.
#[allow(clippy::manual_dangling_ptr)]
const APP_ICON_RESOURCE: PCWSTR = PCWSTR(1 as *const u16);

/// Una entrada del menú contextual.
pub enum MenuItem<'a> {
    Item {
        id: usize,
        label: &'a str,
        checked: bool,
    },
    Separator,
}

fn base_nid(hwnd: HWND, flags: NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: flags,
        ..Default::default()
    }
}

fn set_tip(nid: &mut NOTIFYICONDATAW, tip: &str) {
    let wide: Vec<u16> = tip.encode_utf16().take(127).collect();
    nid.szTip[..wide.len()].copy_from_slice(&wide);
    for slot in nid.szTip[wide.len()..].iter_mut() {
        *slot = 0;
    }
}

/// Carga el icono incrustado en el ejecutable al tamaño de la bandeja.
pub fn load_app_icon() -> Option<HICON> {
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let width = GetSystemMetrics(SM_CXSMICON);
        let height = GetSystemMetrics(SM_CYSMICON);
        let handle = LoadImageW(
            HINSTANCE(instance.0),
            APP_ICON_RESOURCE,
            IMAGE_ICON,
            width,
            height,
            LR_DEFAULTCOLOR,
        )
        .ok()?;
        Some(HICON(handle.0))
    }
}

/// Da de alta el icono en la bandeja.
pub fn add(hwnd: HWND, callback: u32, icon: HICON, tip: &str) -> bool {
    let mut nid = base_nid(hwnd, NIF_MESSAGE | NIF_ICON | NIF_TIP);
    nid.uCallbackMessage = callback;
    nid.hIcon = icon;
    set_tip(&mut nid, tip);
    unsafe { Shell_NotifyIconW(NIM_ADD, &nid).as_bool() }
}

/// Actualiza icono y tooltip de un icono ya existente.
pub fn update(hwnd: HWND, icon: HICON, tip: &str) -> bool {
    let mut nid = base_nid(hwnd, NIF_ICON | NIF_TIP);
    nid.hIcon = icon;
    set_tip(&mut nid, tip);
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool() }
}

/// Quita el icono de la bandeja.
pub fn remove(hwnd: HWND) -> bool {
    let nid = base_nid(hwnd, NIF_MESSAGE);
    unsafe { Shell_NotifyIconW(NIM_DELETE, &nid).as_bool() }
}

/// Muestra un globo de notificación.
pub fn notify(hwnd: HWND, title: &str, body: &str, warning: bool) -> bool {
    let mut nid = base_nid(hwnd, NIF_INFO);
    let title: Vec<u16> = title.encode_utf16().take(63).collect();
    let body: Vec<u16> = body.encode_utf16().take(255).collect();
    nid.szInfoTitle[..title.len()].copy_from_slice(&title);
    nid.szInfo[..body.len()].copy_from_slice(&body);
    nid.dwInfoFlags = if warning { NIIF_WARNING } else { NIIF_NONE };
    nid.Anonymous.uTimeout = 6000;
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool() }
}

/// Muestra el menú contextual y devuelve el identificador elegido.
pub unsafe fn menu(hwnd: HWND, items: &[MenuItem<'_>]) -> Option<usize> {
    let menu = unsafe { CreatePopupMenu() }.ok()?;
    for item in items {
        match item {
            MenuItem::Item { id, label, checked } => {
                let flags = if *checked {
                    MF_STRING | MF_CHECKED
                } else {
                    MF_STRING | MF_UNCHECKED
                };
                let label = HSTRING::from(*label);
                let _ = unsafe { AppendMenuW(menu, flags, *id, &label) };
            }
            MenuItem::Separator => {
                let _ = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()) };
            }
        }
    }

    let mut point = POINT::default();
    let _ = unsafe { GetCursorPos(&mut point) };
    let _ = unsafe { SetForegroundWindow(hwnd) };
    let command = unsafe {
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            None,
        )
    };
    let _ = unsafe { DestroyMenu(menu) };
    let _ = unsafe { PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0)) };

    (command.0 != 0).then_some(command.0 as usize)
}
