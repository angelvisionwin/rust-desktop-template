# Indicaciones para el orquestador (estado tras Fase 3)

Documento operativo para el agente/orquestador que continúe la transformación de
Batería en `rust-desktop-template`. Sustituye a `orquestador-fase-2.md`.

## 1. Estado actual

| Elemento | Valor |
| --- | --- |
| Repositorio | `angelvisionwin/rust-desktop-template` |
| Rama | `main` |
| Commits | `b91403e` inicial → `7ab6b87` importación v0.2.0 → `74edf5a` Fase 1 → `d8cf239` Fase 2 → Fase 3 (HEAD) |
| Estructura | `main.rs` (bin mínimo) + `lib.rs` (módulos) + `cli.rs` + `app.rs` |
| Tests | **45** en el target `lib`; el target `bin` tiene 0 |
| Verificación | `fmt`, `clippy -D warnings`, `test`, `build` y `build --release` en verde |
| Dependencias | `windows = "=0.58.0"`, `serde`, `toml`, `winresource` (build). **No añadir ni actualizar ninguna.** |

Fases completadas: Fase 0 (auditoría), Fase 1 (identidad en `meta.rs`), Fase 2
(capa `platform/`), Fase 3 (biblioteca, CLI y orquestación).

## 2. Invariantes que NO se pueden romper

| Elemento | Valor |
| --- | --- |
| Nombre visible / `ProductName` | `Batería` |
| `FileDescription` | `Batería: estado de batería en la bandeja` |
| `OriginalFilename` | `bateria.exe` |
| Clases de ventana | `BateriaTrayWnd`, `BateriaPanelWnd`, `BateriaSettingsWnd` |
| Mutex de instancia única | `Local\BateriaTrayMutex` |
| Valor en `HKCU\...\Run` | `BateriaTray` |
| Carpeta de datos | `%LOCALAPPDATA%\Bateria` |
| Archivos | `config.toml`, `bateria.log`, `history.tsv` |
| CLI | `--show`, `--dump`, `--version`/`-V`, `--help`/`-h` |
| Salida `--version` | `bateria 0.2.0` + CRLF (lo añade `print_line`) |
| Precedencia CLI | `--version` > `--help` > modo normal; desconocidos ignorados |
| Códigos de salida | Siempre 0; sin mensajes de error |
| Orden de arranque | DPI → hook de pánico → `--dump` → mutex → idioma → `--show` → resto |
| Mensajes Win32 | `WM_APP+1`, `WM_APP+2`, `TIMER_*`, `ID_*` no se renumeran |

## 3. Verificación obligatoria en cada fase

```powershell
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test                     # debe mostrar 45 en src\lib.rs y 0 en src\main.rs
cargo build
cargo build --release
.\target\debug\bateria.exe --version    # bateria 0.2.0
.\target\debug\bateria.exe --help       # 4 líneas, sin salto final
```

Rutas sin tocar datos reales:

```powershell
$tmp = Join-Path $env:TEMP ("check-" + (Get-Random)); New-Item -ItemType Directory $tmp | Out-Null
$env:LOCALAPPDATA = $tmp; .\target\debug\bateria.exe --dump; Get-ChildItem -Recurse $tmp -File
# Debe crear <tmp>\Bateria\config.toml y <tmp>\Bateria\bateria.log
```

## 4. Mapa del código

```
src/
  main.rs      #![windows_subsystem] + fn main() { bateria::run() }   ← 5 líneas, sin `mod`
  lib.rs       #![allow(unsafe_op_in_unsafe_fn)] + módulos + pub fn run()
  cli.rs       Args, Invocation, parse(), help(), version(), print_help(), print_version()
  app.rs       orquestación: 16 static, constantes Win32, wndprocs, bandeja, timers, dump
  meta.rs      identidad
  platform/    paths, registry, instance, console, dpi, string
  util.rs      log, local_time, user_ui_language
  config.rs    Config TOML
  panel.rs     panel GDI
  settings.rs  diálogo
  icon.rs      icono generado
  i18n.rs      traducciones
  history.rs   histórico
  battery/     dominio (se eliminará)
identity.rs    nombre visible compartido con build.rs
```

Reglas de dependencia:

- **Superficie pública de la lib: solo `pub fn run()`.** Todo lo demás es
  `pub(crate)`. No exponer `Args`, `Invocation` ni módulos.
- `main.rs` **no** declara `mod` (provocaría dos copias de los `static`).
- `platform/*` no importa módulos de aplicación.
- `cli` no conoce `app`; `app` solo usa `cli::Args`.
- Ciclo conocido y aceptado hasta Fase 6: `app` ⟷ `settings` (por `app::config()`).

## 5. Reglas para el agente

1. Una fase = un commit independiente (`Fase N: <título>`) con trailer
   `Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>`.
2. No cambiar comportamiento, UI, nombres, rutas, registro, mensajes ni códigos de salida.
3. Mover código antes que reescribirlo; el diff debe ser traslados y repuntes.
4. No crear abstracciones sin consumidor real.
5. Si una comprobación no puede ejecutarse, decirlo; no darla por superada.
6. Dejar `git status` limpio.

## 6. Trampas conocidas

- Añadir `src/lib.rs` hace que el bin y la lib sean **crates distintos**:
  `pub(crate)` no es visible desde `main.rs`, y volver a poner `mod` en `main.rs`
  duplica los `static` (bug silencioso y grave).
- `#![windows_subsystem = "windows"]` solo es válido en el binario;
  `#![allow(unsafe_op_in_unsafe_fn)]` debe estar en `lib.rs` (es por crate).
- `cargo test` imprime dos targets: si el recuento de `src\lib.rs` baja, se ha
  perdido un `mod` en `lib.rs`.
- `w!` funciona en contexto `const`; `SetProcessDpiAwarenessContext` devuelve
  `Result<()>` en 0.58.
- `include!` no admite comentarios `//!`.
- PowerShell: `Test-Path`/`-contains` son insensibles a mayúsculas (`-ccontains`).
- El repo original `C:\si\trabajo\rust\bateria` usa LF; la plantilla, CRLF.

## 7. Fase 4 propuesta (no iniciada)

**Objetivo:** sustituir los 16 `static` por estado propio de una estructura
`App` dentro del hilo de la interfaz, sin cambiar el comportamiento.

Pasos previstos:

1. `struct App` con los campos actuales (`config`, `history`, `view`, `tray`, …).
2. `Arc<App>` en el `lpParam` de `CreateWindowExW` y recuperación vía
   `GWLP_USERDATA` en los `wndproc` (hoy usa `OnceLock`/`Mutex` globales).
3. Mantener intactos: mensajes, temporizadores, orden de arranque, CLIs y textos.
4. Mejora asociada (opcional, decidir): activar la instancia existente en lugar de
   salir en silencio.
5. Aceptación: mismos 45 tests (o más), mismas salidas, prueba manual de bandeja,
   panel, menú, configuración y segunda instancia.

No adelantar Fase 5 (`ui/` desacoplada) ni Fase 6 (diálogo declarativo).
