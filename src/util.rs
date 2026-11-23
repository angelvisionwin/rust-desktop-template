use std::path::PathBuf;
use std::sync::Mutex;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Globalization::GetUserDefaultUILanguage;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_DWORD, RegCloseKey,
    RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows::core::PCWSTR;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end]).trim().to_string()
}

pub fn log(msg: &str) {
    static LOG_LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOG_LOCK.lock();
    let Some(base) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let dir: PathBuf = PathBuf::from(base).join("Bateria");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("bateria.log");
    if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > 512 * 1024 {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write;
        let (h, m, s) = local_time();
        let _ = writeln!(f, "[{h:02}:{m:02}:{s:02}] {msg}");
    }
}

/// LANGID de la interfaz de usuario del sistema.
pub fn user_ui_language() -> u16 {
    unsafe { GetUserDefaultUILanguage() }
}

pub fn local_time() -> (u16, u16, u16) {
    let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    (st.wHour, st.wMinute, st.wSecond)
}

fn open_run_key(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
    let sub = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let mut key = HKEY::default();
    let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()), 0, access, &mut key) };
    (rc == ERROR_SUCCESS).then_some(key)
}

pub fn is_dark_mode() -> bool {
    let sub = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let value = wide("AppsUseLightTheme");
    let mut data: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut std::ffi::c_void),
            Some(&mut size),
        )
    };
    rc == ERROR_SUCCESS && data == 0
}

const RUN_VALUE: &str = "BateriaTray";

pub fn autostart_enabled() -> bool {
    let sub = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let value = wide(RUN_VALUE);
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(value.as_ptr()),
            windows::Win32::System::Registry::RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
    };
    rc == ERROR_SUCCESS
}

pub fn set_autostart(enabled: bool) -> bool {
    let Some(key) = open_run_key(KEY_READ | KEY_SET_VALUE) else {
        return false;
    };
    let value = wide(RUN_VALUE);
    let ok = if enabled {
        let Ok(exe) = std::env::current_exe() else {
            unsafe {
                let _ = RegCloseKey(key);
            }
            return false;
        };
        let command = format!("\"{}\"", exe.display());
        let ws = wide(&command);
        let bytes = unsafe {
            std::slice::from_raw_parts(
                ws.as_ptr() as *const u8,
                ws.len() * std::mem::size_of::<u16>(),
            )
        };
        let rc = unsafe { RegSetValueExW(key, PCWSTR(value.as_ptr()), 0, REG_SZ, Some(bytes)) };
        rc == ERROR_SUCCESS
    } else {
        unsafe { RegDeleteValueW(key, PCWSTR(value.as_ptr())) == ERROR_SUCCESS }
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    ok
}
