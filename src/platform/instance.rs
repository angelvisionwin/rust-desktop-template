//! Control de instancia única mediante un mutex con nombre.

use windows::Win32::Foundation::{BOOL, ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::PCWSTR;

/// Crea el mutex `name` y devuelve `true` si esta es la primera instancia.
///
/// El handle permanece abierto hasta que termina el proceso.
pub fn acquire(name: PCWSTR) -> bool {
    let Ok(_handle) = (unsafe { CreateMutexW(None, BOOL(1), name) }) else {
        return false;
    };
    let last_error = unsafe { GetLastError() };
    last_error != ERROR_ALREADY_EXISTS
}
