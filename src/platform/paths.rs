//! Rutas del sistema y directorios de datos.

use std::path::PathBuf;

/// `%LOCALAPPDATA%`, si está definido.
pub fn local_app_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

/// `%LOCALAPPDATA%\<name>`, si `%LOCALAPPDATA%` está definido.
pub fn local_app_data_dir(name: &str) -> Option<PathBuf> {
    local_app_data().map(|base| base.join(name))
}

/// Directorio de datos de la aplicación.
///
/// Si `%LOCALAPPDATA%` no está definido cae a `.\<name>`.
pub fn data_dir(name: &str) -> PathBuf {
    local_app_data()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(name)
}
