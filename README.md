# rust-desktop-template

Plantilla para crear pequeñas utilidades de escritorio **nativas de Windows** en Rust:
un único ejecutable Win32, sin frameworks gráficos ni runtime adicional.

Incluye la infraestructura que suele repetirse en este tipo de aplicaciones:

- Icono y menú contextual en la bandeja del sistema.
- Panel emergente dibujado con GDI (doble búfer, DPI, tema claro/oscuro).
- Instancia única mediante mutex con nombre.
- Configuración en TOML dentro de `%LOCALAPPDATA%`.
- Registro rotativo en archivo.
- Diálogo de configuración modal y notificaciones del sistema.
- Inicio automático con Windows (`HKCU\...\Run`) conmutables desde el menú.
- CLI (`--help`, `--version`, `--show`, `--dump`) con salida JSON.
- CI con formato, clippy, pruebas, release etiquetado y SHA256.

## Requisitos

- Rust estable con toolchain MSVC (`x86_64-pc-windows-msvc`), edition 2024.
- Windows 10 o superior.

## Primeros pasos

```powershell
cargo run                 # icono en la bandeja, sin ventana
cargo run -- --show       # abre el panel al arrancar
cargo run -- --dump       # imprime el estado en JSON y termina
cargo test
cargo build --release     # target\release\tray-app.exe
```

## Crear una aplicación a partir de la plantilla

1. Copia el repositorio con otro nombre y crea un repositorio nuevo.
2. Ajusta la identidad en **`identity.rs`** (nombre visible y nombre del ejecutable) y en
   `src/meta.rs` (identificador interno, clases de ventana, mutex, valor de `Run`).
3. Cambia `name` y `description` en `Cargo.toml` y el `name` de `[[bin]]`.
   El nombre del binario debe coincidir con `BIN_NAME` de `identity.rs`; el build script
   no puede leer la sección `[[bin]]`.
4. Sustituye `assets/app.ico` por el icono de la aplicación (mismo nombre y ruta).
5. Escribe tu lógica en `snapshot()` de `src/app.rs`: decide qué datos muestra el panel.
   Según lo que necesite la aplicación, tendrás que ajustar además `config.rs` (ajustes),
   `settings.rs` (diálogo de configuración), `strings.rs` (textos) e `identity.rs`/`meta.rs`
   (identidad), y las entradas del menú en `show_menu()`.
6. Revisa `README.md`, `.github/workflows/ci.yml` y `LICENSE`.

## Estructura

```
src/
├─ main.rs              Punto de entrada del ejecutable
├─ lib.rs               Módulos internos y `run()`
├─ cli.rs               Análisis de argumentos y salidas de consola
├─ app.rs               Estado, orquestación y procedimientos de ventana
├─ config.rs            Configuración TOML
├─ meta.rs              Identidad: nombres, rutas, versión y log
├─ strings.rs           Textos de la interfaz
├─ tray.rs              Icono de bandeja y menú contextual
├─ panel.rs             Panel emergente (modelo de vista + pintado GDI)
├─ settings.rs          Diálogo modal de configuración
└─ platform/            Capa Win32 reutilizable, sin lógica de la aplicación
   ├─ console.rs        Salida por consola desde un ejecutable gráfico
   ├─ dialog.rs         Controles, fuentes y avisos de Win32
   ├─ dpi.rs            Conciencia de DPI por monitor
   ├─ instance.rs       Instancia única
   ├─ log.rs            Registro en archivo con rotación
   ├─ paths.rs          Rutas de `%LOCALAPPDATA%`
   ├─ registry.rs       Inicio automático con Windows
   ├─ string.rs         Conversión a UTF-16
   └─ time.rs           Hora local
```

`platform` no importa módulos de la aplicación: solo depende del crate `windows`.

## Modelo de vista del panel

El panel no conoce el dominio de la aplicación; recibe datos ya resueltos:

```rust
RowView {
    title: String,        // nombre de la fila
    detail: String,       // texto secundario
    value: String,        // valor a la derecha
    percent: Option<u8>,  // dibuja una barra si es Some
    accent: Accent,       // Neutral, Ok, Warn o Bad
}
```

`snapshot()` devuelve `Vec<RowView>` y `App` los guarda en un `PanelView` junto al
título, el subtítulo, el pie y el mensaje de lista vacía. El mismo vector alimenta el
panel, el tooltip de la bandeja y `--dump`.

El refresco lo ejecuta un hilo independiente que recibe una copia de la configuración y
devuelve las filas por un canal; `App` sigue siendo el único propietario del estado y
publica `WM_REFRESHED` cuando el trabajador termina.

## Configuración, datos y log

Todo se guarda en `%LOCALAPPDATA%\TrayApp`:

| Archivo | Contenido |
| --- | --- |
| `config.toml` | Intervalo de actualización en segundos (10–3600). |
| `tray-app.log` | Registro rotativo (512 KB) con el ciclo de vida y los errores. |

La configuración se puede editar a mano o desde el diálogo del menú de la bandeja.

## CLI

| Argumento | Efecto |
| --- | --- |
| `--show` | Abre el panel al arrancar. |
| `--dump` | Imprime el estado en JSON y termina. |
| `--version` | Muestra la versión y termina. |
| `--help` | Muestra la ayuda y termina. |

Sin argumentos, la aplicación solo deja el icono en la bandeja.

## Integración continua y releases

`.github/workflows/ci.yml` comprueba formato, clippy y pruebas, y en cada etiqueta `v*`
compila `tray-app.exe`, genera `SHA256SUMS.txt` y publica el release con ambos archivos.
El trabajo de compilación verifica además el tamaño del binario, los metadatos de versión
y que `--version` responda con el nombre esperado.

## Documentación histórica

Los documentos de las fases con las que se construyó esta plantilla están en
[`docs/historia/`](docs/historia). No forman parte de la documentación de uso.

## Licencia

MPL-2.0. Consulta [LICENSE](LICENSE).