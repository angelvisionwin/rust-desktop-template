//! Identidad de la aplicación: única fuente de nombres, rutas y versión.
//!
//! Los literales que definen el producto viven aquí. Para renombrar la
//! aplicación hay que ajustar [`APP_NAME`] (definido en `identity.rs`),
//! [`APP_ID`] y los nombres derivados (clases, mutex y valor de `Run`), además
//! de los campos `name` y `description` de `Cargo.toml`.

use windows::core::{PCWSTR, w};

include!("../identity.rs");

/// Nombre visible de la aplicación.
pub const APP_NAME: &str = DISPLAY_NAME;

/// Identificador interno sin acentos: base de las clases y del mutex.
pub const APP_ID: &str = "Bateria";

/// Nombre del paquete y del binario, definido en `Cargo.toml`.
pub const BIN_NAME: &str = env!("CARGO_PKG_NAME");

/// Versión del paquete, definida en `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Carpeta de datos dentro de `%LOCALAPPDATA%`.
pub const DATA_DIR_NAME: &str = APP_ID;

/// Archivo de configuración dentro de la carpeta de datos.
pub const CONFIG_FILE: &str = "config.toml";

/// Archivo de histórico dentro de la carpeta de datos.
pub const HISTORY_FILE: &str = "history.tsv";

/// Valor de `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
pub const RUN_VALUE: &str = "BateriaTray";

/// Ventana oculta que posee el icono de la bandeja.
pub const WINDOW_CLASS_MAIN: PCWSTR = w!("BateriaTrayWnd");

/// Ventana del panel.
pub const WINDOW_CLASS_PANEL: PCWSTR = w!("BateriaPanelWnd");

/// Ventana del diálogo de configuración.
pub const WINDOW_CLASS_SETTINGS: PCWSTR = w!("BateriaSettingsWnd");

/// Mutex de instancia única, en el espacio de nombres `Local`.
pub const MUTEX_NAME: PCWSTR = w!("Local\\BateriaTrayMutex");

/// Nombre del archivo de log, derivado del nombre del paquete.
pub fn log_file() -> String {
    format!("{BIN_NAME}.log")
}
