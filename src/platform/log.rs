//! Registro rotativo en un archivo.
//!
//! La capa no conoce la identidad de la aplicación: el llamante indica la ruta.

use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use super::time::local_time;

/// Tamaño a partir del cual se trunca el archivo.
const MAX_BYTES: u64 = 512 * 1024;

static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Añade una línea con la hora actual, truncando el archivo si supera 512 KB.
pub fn write(path: &Path, message: &str) {
    let _guard = WRITE_LOCK.lock();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0) > MAX_BYTES {
        let _ = std::fs::remove_file(path);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let (hours, minutes, seconds) = local_time();
        let _ = writeln!(file, "[{hours:02}:{minutes:02}:{seconds:02}] {message}");
    }
}
