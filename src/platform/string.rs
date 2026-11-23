//! Conversión entre cadenas de Rust y buffers UTF-16 de Win32.

/// Cadena UTF-16 terminada en nulo, lista para las API `...W`.
pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Texto de un buffer UTF-16 terminado en nulo, sin espacios sobrantes.
pub fn from_wide(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end]).trim().to_string()
}
