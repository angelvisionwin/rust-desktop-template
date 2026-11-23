//! Conversión entre cadenas de Rust y buffers UTF-16 de Win32.

/// Cadena UTF-16 terminada en nulo, lista para las API `...W`.
pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
