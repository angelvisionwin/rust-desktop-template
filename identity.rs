// Nombre visible del producto, compartido por `build.rs` y `src/meta.rs`.
//
// Este archivo no pertenece a ningún crate: se incluye con `include!` desde el
// build script y desde `src/meta.rs` para que el nombre esté definido una sola
// vez. El resto de la identidad se deriva de `Cargo.toml`.

/// Nombre visible de la aplicación.
pub const DISPLAY_NAME: &str = "Batería";
