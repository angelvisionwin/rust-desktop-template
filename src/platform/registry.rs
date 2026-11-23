//! Operaciones del Registro de Windows.

use std::ffi::c_void;

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SAM_FLAGS, REG_SZ, RRF_RT_REG_DWORD,
    RRF_RT_REG_SZ, RegCloseKey, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows::core::PCWSTR;

use super::string::wide;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const THEMES_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";

fn open_run_key(access: REG_SAM_FLAGS) -> Option<HKEY> {
    let sub = wide(RUN_KEY);
    let mut key = HKEY::default();
    let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()), 0, access, &mut key) };
    (rc == ERROR_SUCCESS).then_some(key)
}

/// `true` si `value` ya está registrado en `HKCU\...\Run`.
pub fn autostart_enabled(value: &str) -> bool {
    let sub = wide(RUN_KEY);
    let name = wide(value);
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
    };
    rc == ERROR_SUCCESS
}

/// Activa o desactiva el inicio automático de `value`, apuntando al ejecutable actual.
pub fn set_autostart(value: &str, enabled: bool) -> bool {
    let Some(key) = open_run_key(KEY_READ | KEY_SET_VALUE) else {
        return false;
    };
    let name = wide(value);
    let ok = if enabled {
        let Ok(exe) = std::env::current_exe() else {
            unsafe {
                let _ = RegCloseKey(key);
            }
            return false;
        };
        let command = format!("\"{}\"", exe.display());
        let command = wide(&command);
        let bytes = unsafe {
            std::slice::from_raw_parts(
                command.as_ptr() as *const u8,
                command.len() * std::mem::size_of::<u16>(),
            )
        };
        let rc = unsafe { RegSetValueExW(key, PCWSTR(name.as_ptr()), 0, REG_SZ, Some(bytes)) };
        rc == ERROR_SUCCESS
    } else {
        unsafe { RegDeleteValueW(key, PCWSTR(name.as_ptr())) == ERROR_SUCCESS }
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    ok
}

/// `true` si Windows está configurado con tema oscuro.
pub fn is_dark_mode() -> bool {
    let sub = wide(THEMES_KEY);
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
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    rc == ERROR_SUCCESS && data == 0
}
