use std::sync::Mutex;

use windows::Win32::Globalization::GetUserDefaultUILanguage;

use crate::meta;
use crate::platform::paths;

pub fn log(msg: &str) {
    static LOG_LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOG_LOCK.lock();
    let Some(dir) = paths::local_app_data_dir(meta::DATA_DIR_NAME) else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(meta::log_file());
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
