# Indicaciones para el orquestador (estado tras Fase 4)

Documento operativo para el agente/orquestador que continúe la transformación de
Batería en `rust-desktop-template`. Sustituye a `orquestador-fase-3.md`.

## 1. Estado actual

| Elemento | Valor |
| --- | --- |
| Repositorio | `angelvisionwin/rust-desktop-template` |
| Rama | `main` |
| Commits | `b91403e` → `7ab6b87` importación → `74edf5a` Fase 1 → `d8cf239` Fase 2 → `2d3e1da` Fase 3 → Fase 4 (HEAD) |
| Estructura | `main.rs` (bin mínimo) + `lib.rs` + `cli.rs` + `app.rs` |
| Estado de la app | `struct App` en `app.rs`; **0 `static`** |
| Tests | **45** en el target `lib`; 0 en el bin |
| Dependencias | `windows = "=0.58.0"`, `serde`, `toml`, `winresource` (build). **No añadir ni actualizar.** |

Fases completadas: 0 (auditoría), 1 (identidad), 2 (capa `platform`), 3 (lib/cli/app),
4 (estado en `App`).

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
| CLI | `--show`, `--dump`, `--version`/`-V`, `--help`/`-h`; versión = `bateria 0.2.0` |
| Precedencia CLI | `--version` > `--help` > modo normal; desconocidos ignorados |
| Códigos de salida | Siempre 0; sin mensajes de error |
| Orden de arranque | DPI → hook de pánico → `--dump` → mutex → `App::new` → idioma → `--show` → ventanas |
| Mensajes Win32 | `WM_APP+1`, `WM_APP+2`, `TIMER_*`, `ID_*` no se renumeran |

## 3. Verificación obligatoria

```powershell
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test                     # 45 en src\lib.rs, 0 en src\main.rs
cargo build
cargo build --release
.\target\debug\bateria.exe --version   # bateria 0.2.0
.\target\debug\bateria.exe --help      # 4 líneas, LF interno + CRLF final
```

Rutas sin tocar datos reales (capturar con `cmd /c`, no con `>` de PowerShell):

```powershell
$tmp = Join-Path $env:TEMP ("check-" + (Get-Random)); New-Item -ItemType Directory $tmp | Out-Null
$env:LOCALAPPDATA = $tmp
cmd /c ".\target\debug\bateria.exe --dump > `"$tmp\out.json`" 2>&1"
Get-ChildItem -Recurse -File $tmp | Where-Object { $_.FullName -notmatch '\\Microsoft\\' }
# Debe crear <tmp>\Bateria\config.toml y <tmp>\Bateria\bateria.log
```

## 4. Modelo de estado (crítico)

```
run()  ──Box<App>──►  dueño único durante toda la vida del proceso
   │
   ├─ CreateWindowExW(main, lpParam = &*app)
   │     └─ WM_NCCREATE: GWLP_USERDATA = puntero   (si falla → LRESULT(0): se rechaza la ventana)
   ├─ CreateWindowExW(panel, parent=main, lpParam = &*app)
   ├─ bucle de mensajes
   └─ teardown: DestroyWindow(panel) → DestroyWindow(main) → drop(app)
```

Reglas que no se pueden romper:

1. `run()` es el **único propietario**. No hay `Box::into_raw`/`from_raw`.
2. Las ventanas guardan un **puntero prestado**; `WM_NCDESTROY` solo limpia
   `GWLP_USERDATA`.
3. Antes de `drop(app)`, `destroy_window()` confirma con `IsWindow` que la ventana
   ya no existe. Si alguna sobrevive, se registra y **se filtra `App`**
   (`std::mem::forget`) para no dejar punteros colgantes.
4. `App` es `!Sync` a propósito (`Cell`/`RefCell`, `HWND`). El trabajador de
   refresco **solo** recibe copias (`Config`, `Sender`) y el `Arc<AtomicBool>` de
   `busy`. **Prohibido `Arc<App>`.**
5. Los `wndproc` se **reentran** (`TrackPopupMenu`, `settings::open`, `MessageBoxW`,
   `SetWindowTextW`, `ShowWindow`, `DestroyWindow`, `Shell_NotifyIconW`). Ningún
   `RefCell` prestado puede sobrevivir a esas llamadas.
6. En rutas alcanzables desde un `wndproc` se usa `try_borrow`/`try_borrow_mut` y se
   degrada (no notificar / no pintar / saltar) en lugar de provocar un *panic*:
   un panic que salga de un `extern "system"` **aborta el proceso**.

## 5. Mapa del código

```
src/
  main.rs      #![windows_subsystem] + bateria::run()
  lib.rs       #![allow(unsafe_op_in_unsafe_fn)] + módulos + pub fn run()
  cli.rs       Args, Invocation, parse(), help(), version(), print_*
  app.rs       struct App, helpers de GWLP_USERDATA, wndprocs, bandeja, panel, timers, dump
  meta.rs      identidad
  platform/    paths, registry, instance, console, dpi, string
  util.rs      log, local_time, user_ui_language
  config.rs    Config TOML
  panel.rs     panel GDI
  settings.rs  diálogo (recibe `Config` por valor)
  icon.rs      icono generado
  i18n.rs      traducciones
  history.rs   histórico
  battery/     dominio (se eliminará)
identity.rs    nombre visible compartido con build.rs
```

Dependencias permitidas: `app → {battery, config, history, i18n, icon, meta, panel,
platform, settings, util, cli::Args}`. `platform/*` no importa módulos de aplicación.
`cli` no conoce `app`.

## 6. Trampas conocidas

- **Doble compilación**: `main.rs` no declara `mod`. Si se añadiera, habría dos
  copias del estado.
- `#![windows_subsystem]` solo en el binario; `#![allow(unsafe_op_in_unsafe_fn)]`
  en `lib.rs`.
- `WM_NCCREATE` debe devolver `TRUE` (`DefWindowProcW`). Si `attach_app` falla se
  devuelve `LRESULT(0)` y la ventana no se crea.
- `SetWindowTextW` envía `WM_SETTEXT` **sincrónico**: no llamarlo con préstamos vivos.
- `GetWindowLongPtrW`/`SetWindowLongPtrW` son `unsafe`; el puntero no promete
  duración de vida: la `&App` se crea en el ámbito de uso.
- `cargo test` muestra dos targets: si el recuento de `src\lib.rs` baja de 45, se
  ha perdido un `mod`.
- Captura de CLI: `>` de PowerShell no llega a un binario gráfico; usar `cmd /c`.
- `w!` funciona en `const`; `SetProcessDpiAwarenessContext` devuelve `Result<()>`.
- El repo original `C:\si\trabajo\rust\bateria` usa LF; la plantilla, CRLF.

## 7. Fase 5 propuesta (no iniciada)

**Objetivo:** desacoplar la interfaz del dominio de batería, sin cambiar su aspecto.

1. `ui/theme.rs` (paleta clara/oscura) y `ui/panel.rs` con `PanelModel`/`RowView`
   en lugar de `&[DeviceBattery]`.
2. `panel.rs` deja de depender de `battery` e `i18n`: recibe textos y valores ya
   resueltos.
3. Añadir `WM_DPICHANGED` al panel (hoy solo se recoloca en `resize`).
4. Mantener `icon.rs` fuera del alcance salvo que se mueva a `ui/`.
5. Aceptación: mismo aspecto píxel a píxel, 45 tests, CLI y rutas idénticas.

No adelantar Fase 6 (diálogo declarativo) ni Fase 7 (i18n por capas).
