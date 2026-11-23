//! Salida por consola desde una aplicación con subsistema gráfico.

use windows::Win32::Storage::FileSystem::WriteFile;
use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE,
};

/// Escribe `text` y un salto de línea en la salida estándar.
///
/// Si la salida está redirigida se usa el handle heredado; si no, se engancha a
/// la consola del proceso padre (cmd/PowerShell).
pub fn print_line(text: &str) {
    let mut payload = text.to_string();
    payload.push_str("\r\n");
    unsafe {
        let mut handle = GetStdHandle(STD_OUTPUT_HANDLE).ok();
        if handle.map(|handle| handle.is_invalid()).unwrap_or(true) {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
            handle = GetStdHandle(STD_OUTPUT_HANDLE).ok();
        }
        let Some(handle) = handle else {
            return;
        };
        if handle.is_invalid() {
            return;
        }
        let mut written = 0u32;
        let _ = WriteFile(handle, Some(payload.as_bytes()), Some(&mut written), None);
    }
}
