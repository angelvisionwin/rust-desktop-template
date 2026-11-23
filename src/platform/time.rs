//! Hora local del sistema.

/// Hora, minuto y segundo actuales.
pub fn local_time() -> (u16, u16, u16) {
    let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    (now.wHour, now.wMinute, now.wSecond)
}
