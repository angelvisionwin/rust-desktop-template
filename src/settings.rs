//! Diálogo modal para editar la configuración.

use std::ffi::c_void;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, COLOR_BTNFACE, CreateFontW, DEFAULT_CHARSET,
    DEFAULT_PITCH, DeleteObject, FF_DONTCARE, GetMonitorInfoW, HBRUSH, HFONT, HGDIOBJ,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, OUT_DEFAULT_PRECIS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CB_ADDSTRING,
    CB_GETCURSEL, CB_SETCURSEL, CBS_DROPDOWNLIST, CBS_HASSTRINGS, CreateWindowExW, DefWindowProcW,
    DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_NUMBER,
    ES_RIGHT, ES_WANTRETURN, GWLP_USERDATA, GetCursorPos, GetMessageW, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowTextW, HMENU, IDC_ARROW, IsDialogMessageW, IsWindow,
    LoadCursorW, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MSG, MessageBoxW, PostQuitMessage,
    RegisterClassW, SW_SHOW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW,
    ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY,
    WM_SETFONT, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_EX_CLIENTEDGE,
    WS_EX_TOOLWINDOW, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::config::Config;
use crate::i18n::{self, Lang};
use crate::meta;
use crate::platform::string;
use crate::util;

const ID_LANGUAGE: i32 = 101;
const ID_THRESHOLD: i32 = 102;
const ID_REFRESH: i32 = 103;
const ID_TIME_LEFT: i32 = 104;
const ID_IGNORED: i32 = 105;
const ID_FOLDER: i32 = 106;
// IDOK e IDCANCEL: `IsDialogMessageW` envía estos identificadores con Intro y Escape.
const ID_SAVE: i32 = 1;
const ID_CANCEL: i32 = 2;

// Estilos no exportados por el crate de `windows` que se usan aquí.
const SS_CENTERIMAGE: WINDOW_STYLE = WINDOW_STYLE(0x0200);
const BST_CHECKED: isize = 1;

const WIDTH: f32 = 440.0;
const MARGIN: f32 = 16.0;
const SECTION: f32 = 14.0;
const GAP: f32 = 10.0;
const ROW: f32 = 24.0;
const FIELD: f32 = 180.0;
const NUMBER: f32 = 80.0;
const BUTTON: f32 = 28.0;

/// Códigos de idioma en el mismo orden que el desplegable.
const LANGUAGES: [&str; 6] = ["auto", "es", "ca", "en", "fr", "de"];
const LANGUAGE_NAMES: [&str; 5] = ["Español", "Català", "English", "Français", "Deutsch"];

#[derive(Clone, Copy)]
struct Controls {
    language: HWND,
    threshold: HWND,
    refresh: HWND,
    time_left: HWND,
    ignored: HWND,
    font: HFONT,
}

struct State {
    parent: HWND,
    controls: Controls,
    current: Config,
    saved: Option<Config>,
    out: *mut Option<Config>,
}

static OPENED: std::sync::Mutex<isize> = std::sync::Mutex::new(0);
static REGISTERED: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

pub fn is_open() -> bool {
    *OPENED.lock().unwrap() != 0
}

pub fn focus() {
    let address = *OPENED.lock().unwrap();
    if address != 0 {
        unsafe {
            let _ = SetForegroundWindow(HWND(address as *mut c_void));
        }
    }
}

/// Abre el diálogo y devuelve la configuración guardada, si la hay.
pub unsafe fn open(parent: HWND) -> Option<Config> {
    if is_open() {
        focus();
        return None;
    }

    let instance = HINSTANCE(GetModuleHandleW(None).ok()?.0);
    let class = meta::WINDOW_CLASS_SETTINGS;
    let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();

    let mut registered = REGISTERED.lock().unwrap();
    if !*registered {
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            hCursor: cursor,
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 as usize + 1) as *mut c_void),
            lpszClassName: class,
            ..Default::default()
        };
        if RegisterClassW(&window_class) == 0 {
            util::log("settings: no se pudo registrar la clase");
            return None;
        }
        *registered = true;
    }
    drop(registered);

    let mut point = POINT::default();
    let _ = GetCursorPos(&mut point);
    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
    let mut monitor_info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let _ = GetMonitorInfoW(monitor, &mut monitor_info);
    let work = monitor_info.rcWork;

    // El diálogo aparece en el monitor del cursor, así que su escala puede no
    // coincidir con la de la ventana principal.
    let dpi = {
        let mut dpi_x = 0u32;
        let mut dpi_y = 0u32;
        match GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) {
            Ok(()) if dpi_x > 0 => dpi_x,
            _ => {
                let dpi = GetDpiForWindow(parent);
                if dpi == 0 { 96 } else { dpi }
            }
        }
    };
    let scale = dpi as f32 / 96.0;
    let px = |value: f32| (value * scale).round() as i32;

    let m = px(MARGIN);
    let row = px(ROW);
    let section = px(SECTION);
    let gap = px(GAP);
    let client_width = px(WIDTH);
    let right = client_width - m;
    let label_width = right - m - px(FIELD) - px(12.0);

    let y_language = m;
    let y_threshold = y_language + row + section;
    let y_refresh = y_threshold + row + gap;
    let y_check = y_refresh + row + section;
    let y_ignored_label = y_check + px(22.0) + gap;
    let y_ignored = y_ignored_label + px(18.0) + px(4.0);
    let y_hint = y_ignored + px(90.0) + px(4.0);
    let y_buttons = y_hint + px(16.0) + section;
    let client_height = y_buttons + px(BUTTON) + m;

    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
    let ex_style = WS_EX_TOOLWINDOW;
    let mut frame = RECT {
        left: 0,
        top: 0,
        right: client_width,
        bottom: client_height,
    };
    let _ = AdjustWindowRectExForDpi(&mut frame, style, false, ex_style, dpi);
    let width = frame.right - frame.left;
    let height = frame.bottom - frame.top;

    let x = work.left + ((work.right - work.left) - width).max(0) / 2;
    let y = work.top + ((work.bottom - work.top) - height).max(0) / 2;

    let title = HSTRING::from(i18n::text().settings_title);
    let Ok(dialog) = CreateWindowExW(
        ex_style,
        class,
        &title,
        style,
        x,
        y,
        width,
        height,
        HWND(std::ptr::null_mut()),
        None,
        instance,
        None,
    ) else {
        util::log("settings: no se pudo crear la ventana");
        return None;
    };

    let font = CreateFontW(
        -px(12.0),
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        w!("Segoe UI"),
    );

    let controls = (|| -> Option<Controls> {
        let text = i18n::text();
        let labels: [(i32, &str, RECT); 4] = [
            (
                ID_LANGUAGE,
                text.settings_language,
                rect(m, y_language, label_width, row),
            ),
            (
                ID_THRESHOLD,
                text.settings_threshold,
                rect(m, y_threshold, label_width, row),
            ),
            (
                ID_REFRESH,
                text.settings_refresh,
                rect(m, y_refresh, label_width, row),
            ),
            (
                ID_IGNORED,
                text.settings_ignored,
                rect(m, y_ignored_label, right - m, px(18.0)),
            ),
        ];
        for (_, label, area) in labels {
            child(
                dialog,
                w!("STATIC"),
                label,
                WS_CHILD | WS_VISIBLE | SS_CENTERIMAGE,
                WINDOW_EX_STYLE(0),
                area,
                0,
                font,
            )?;
        }

        let language = child(
            dialog,
            w!("COMBOBOX"),
            "",
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | WINDOW_STYLE((CBS_DROPDOWNLIST | CBS_HASSTRINGS) as u32),
            WINDOW_EX_STYLE(0),
            rect(
                right - px(FIELD),
                y_language - px(3.0),
                px(FIELD),
                px(200.0),
            ),
            ID_LANGUAGE,
            font,
        )?;

        let number_style = WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | WINDOW_STYLE((ES_AUTOHSCROLL | ES_NUMBER | ES_RIGHT) as u32);
        let threshold = child(
            dialog,
            w!("EDIT"),
            "",
            number_style,
            WS_EX_CLIENTEDGE,
            rect(right - px(NUMBER), y_threshold, px(NUMBER), row),
            ID_THRESHOLD,
            font,
        )?;
        let refresh = child(
            dialog,
            w!("EDIT"),
            "",
            number_style,
            WS_EX_CLIENTEDGE,
            rect(right - px(NUMBER), y_refresh, px(NUMBER), row),
            ID_REFRESH,
            font,
        )?;

        let time_left = child(
            dialog,
            w!("BUTTON"),
            text.settings_show_remaining,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            WINDOW_EX_STYLE(0),
            rect(m, y_check, right - m, px(22.0)),
            ID_TIME_LEFT,
            font,
        )?;

        let ignored = child(
            dialog,
            w!("EDIT"),
            "",
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32),
            WS_EX_CLIENTEDGE,
            rect(m, y_ignored, right - m, px(90.0)),
            ID_IGNORED,
            font,
        )?;

        child(
            dialog,
            w!("STATIC"),
            text.settings_ignored_hint,
            WS_CHILD | WS_VISIBLE,
            WINDOW_EX_STYLE(0),
            rect(m, y_hint, right - m, px(16.0)),
            0,
            font,
        )?;

        let button = WS_CHILD | WS_VISIBLE | WS_TABSTOP;
        child(
            dialog,
            w!("BUTTON"),
            text.settings_open_folder,
            button | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            WINDOW_EX_STYLE(0),
            rect(
                right - px(96.0) - px(8.0) - px(96.0) - px(8.0) - px(130.0),
                y_buttons,
                px(130.0),
                px(BUTTON),
            ),
            ID_FOLDER,
            font,
        )?;
        child(
            dialog,
            w!("BUTTON"),
            text.settings_cancel,
            button | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            WINDOW_EX_STYLE(0),
            rect(
                right - px(96.0) - px(8.0) - px(96.0),
                y_buttons,
                px(96.0),
                px(BUTTON),
            ),
            ID_CANCEL,
            font,
        )?;
        child(
            dialog,
            w!("BUTTON"),
            text.settings_save,
            button | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
            WINDOW_EX_STYLE(0),
            rect(right - px(96.0), y_buttons, px(96.0), px(BUTTON)),
            ID_SAVE,
            font,
        )?;

        Some(Controls {
            language,
            threshold,
            refresh,
            time_left,
            ignored,
            font,
        })
    })();

    let Some(controls) = controls else {
        util::log("settings: no se pudieron crear los controles");
        let _ = DestroyWindow(dialog);
        let _ = DeleteObject(HGDIOBJ(font.0));
        return None;
    };

    let current = crate::app::config();
    let mut result: Option<Config> = None;
    let state = Box::into_raw(Box::new(State {
        parent,
        controls,
        current: current.clone(),
        saved: None,
        out: &mut result,
    }));
    SetWindowLongPtrW(dialog, GWLP_USERDATA, state as isize);
    *OPENED.lock().unwrap() = dialog.0 as isize;

    add_combo_item(controls.language, i18n::text().settings_auto);
    for name in LANGUAGE_NAMES {
        add_combo_item(controls.language, name);
    }
    let index = language_index(&current.language);
    SendMessageW(controls.language, CB_SETCURSEL, WPARAM(index), LPARAM(0));
    set_text(controls.threshold, &current.threshold().to_string());
    set_text(
        controls.refresh,
        &current.refresh_interval_secs().to_string(),
    );
    SendMessageW(
        controls.time_left,
        BM_SETCHECK,
        WPARAM(if current.show_time_remaining {
            BST_CHECKED as usize
        } else {
            0
        }),
        LPARAM(0),
    );
    set_text(controls.ignored, &current.ignored_devices.join("\r\n"));

    let _ = EnableWindow(parent, false);
    let _ = ShowWindow(dialog, SW_SHOW);
    let _ = SetForegroundWindow(dialog);

    let mut message = MSG::default();
    while IsWindow(dialog).as_bool() {
        let got = GetMessageW(&mut message, HWND(std::ptr::null_mut()), 0, 0);
        if got.0 <= 0 {
            if got.0 == 0 {
                let _ = DestroyWindow(dialog);
                PostQuitMessage(message.wParam.0 as i32);
            }
            break;
        }
        if !IsDialogMessageW(dialog, &message).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    result
}

extern "system" fn wndproc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_COMMAND => {
                let id = (wparam.0 & 0xFFFF) as i32;
                // BN_CLICKED
                if ((wparam.0 >> 16) & 0xFFFF) == 0 {
                    match id {
                        ID_SAVE => save(hwnd),
                        ID_CANCEL => {
                            let _ = DestroyWindow(hwnd);
                        }
                        ID_FOLDER => open_folder(),
                        _ => {}
                    }
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
                if !pointer.is_null() {
                    let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    let state = Box::from_raw(pointer);
                    if let Some(config) = state.saved.clone() {
                        *state.out = Some(config);
                    }
                    let _ = DeleteObject(HGDIOBJ(state.controls.font.0));
                    let _ = EnableWindow(state.parent, true);
                }
                *OPENED.lock().unwrap() = 0;
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

unsafe fn save(hwnd: HWND) {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    if pointer.is_null() {
        return;
    }
    let state = &mut *pointer;
    let strings = i18n::text();

    let threshold = match parse_number(
        &get_text(state.controls.threshold),
        state.current.threshold() as u64,
    ) {
        Some(value) if (1..=100).contains(&value) => value as u8,
        _ => {
            warn(
                hwnd,
                strings.settings_error_title,
                strings.settings_invalid_threshold,
            );
            let _ = SetFocus(state.controls.threshold);
            return;
        }
    };
    let refresh = match parse_number(
        &get_text(state.controls.refresh),
        state.current.refresh_interval_secs(),
    ) {
        Some(value) if (10..=3600).contains(&value) => value,
        _ => {
            warn(
                hwnd,
                strings.settings_error_title,
                strings.settings_invalid_refresh,
            );
            let _ = SetFocus(state.controls.refresh);
            return;
        }
    };

    let selection = SendMessageW(state.controls.language, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
    let language = LANGUAGES
        .get(selection.max(0) as usize)
        .copied()
        .unwrap_or("auto")
        .to_string();

    let show = SendMessageW(state.controls.time_left, BM_GETCHECK, WPARAM(0), LPARAM(0)).0;
    let config = Config {
        language,
        low_battery_threshold: threshold,
        refresh_seconds: refresh,
        show_time_remaining: show == BST_CHECKED,
        ignored_devices: get_text(state.controls.ignored)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
    };

    if let Err(error) = config.save() {
        warn(
            hwnd,
            strings.settings_error_title,
            &strings.save_failed(&error.to_string()),
        );
        return;
    }
    state.saved = Some(config);
    let _ = DestroyWindow(hwnd);
}

unsafe fn open_folder() {
    let dir = crate::config::dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Err(error) = std::process::Command::new("explorer.exe").arg(&dir).spawn() {
        util::log(&format!("settings: no se pudo abrir la carpeta: {error}"));
    }
}

fn language_index(code: &str) -> usize {
    match Lang::parse(code) {
        None => 0,
        Some(Lang::Es) => 1,
        Some(Lang::Ca) => 2,
        Some(Lang::En) => 3,
        Some(Lang::Fr) => 4,
        Some(Lang::De) => 5,
    }
}

fn parse_number(text: &str, fallback: u64) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() {
        return Some(fallback);
    }
    text.parse().ok()
}

fn rect(x: i32, y: i32, width: i32, height: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}

unsafe fn add_combo_item(combo: HWND, text: &str) {
    let wide = string::wide(text);
    SendMessageW(
        combo,
        CB_ADDSTRING,
        WPARAM(0),
        LPARAM(wide.as_ptr() as isize),
    );
}

unsafe fn set_text(hwnd: HWND, text: &str) {
    let text = HSTRING::from(text);
    let _ = SetWindowTextW(hwnd, &text);
}

unsafe fn get_text(hwnd: HWND) -> String {
    let length = GetWindowTextLengthW(hwnd).max(0) as usize;
    let mut buffer = vec![0u16; length + 1];
    let copied = GetWindowTextW(hwnd, &mut buffer).max(0) as usize;
    String::from_utf16_lossy(&buffer[..copied])
        .trim()
        .to_string()
}

unsafe fn warn(owner: HWND, title: &str, body: &str) {
    let title = HSTRING::from(title);
    let body = HSTRING::from(body);
    MessageBoxW(
        owner,
        &body,
        &title,
        windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE(
            MB_OK.0 | MB_ICONWARNING.0 | MB_SETFOREGROUND.0,
        ),
    );
}

// Envoltura directa de `CreateWindowExW`: una firma con muchos parámetros es lo natural.
#[allow(clippy::too_many_arguments)]
unsafe fn child(
    parent: HWND,
    class: PCWSTR,
    text: &str,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    area: RECT,
    id: i32,
    font: HFONT,
) -> Option<HWND> {
    let instance = HINSTANCE(GetModuleHandleW(None).ok()?.0);
    let text = HSTRING::from(text);
    let handle = CreateWindowExW(
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
    .ok()?;
    SendMessageW(handle, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
    Some(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_selection_maps_both_ways() {
        assert_eq!(language_index("auto"), 0);
        assert_eq!(language_index("es"), 1);
        assert_eq!(language_index("ca-ES"), 2);
        assert_eq!(language_index("EN"), 3);
        assert_eq!(language_index("fr"), 4);
        assert_eq!(language_index("de"), 5);
        assert_eq!(language_index("pt"), 0);
        for (index, code) in LANGUAGES.iter().enumerate() {
            assert_eq!(language_index(code), index);
        }
        assert_eq!(LANGUAGES.len(), LANGUAGE_NAMES.len() + 1);
    }

    #[test]
    fn numbers_fall_back_when_empty() {
        assert_eq!(parse_number("", 20), Some(20));
        assert_eq!(parse_number("  45 ", 20), Some(45));
        assert_eq!(parse_number("0", 20), Some(0));
        assert_eq!(parse_number("abc", 20), None);
        assert_eq!(parse_number("99999999999999999999999", 20), None);
    }
}
