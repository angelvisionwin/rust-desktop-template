# Indicaciones para el orquestador (estado tras Fase 2)

Documento operativo para el agente/orquestador que continúe la transformación de
Batería en `rust-desktop-template`. Sustituye a cualquier instrucción previa sobre
el estado del repositorio.

## 1. Estado actual

| Elemento | Valor |
| --- | --- |
| Repositorio | `angelvisionwin/rust-desktop-template` |
| Rama | `main` |
| Commits | `b91403e` inicial → `7ab6b87` importación de Batería v0.2.0 → `74edf5a` Fase 1 → Fase 2 |
| Compilación | `cargo build` OK, 37 tests OK, `fmt` y `clippy -D warnings` limpios |
| Toolchain | Rust estable, edition 2024, MSVC x86_64 |
| Dependencias | `windows = "=0.58.0"`, `serde`, `toml`, `winresource` (build). **No añadir ni actualizar ninguna.** |

Fases completadas:

- **Fase 0** — auditoría (sin cambios de código).
- **Fase 1** — identidad centralizada en `src/meta.rs` + `identity.rs`.
- **Fase 2** — capa `src/platform/` extraída.

## 2. Invariantes que NO se pueden romper

Estos valores están verificados y cualquier cambio es una regresión:

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
| Claves de registro | `...\CurrentVersion\Run` y `...\Themes\Personalize\AppsUseLightTheme` |
| CLI | `--show`, `--dump`, `--version`/`-V`, `--help`/`-h`; salida de `--version` = `bateria <versión>` |
| Mensajes Win32 | `WM_APP+1`, `WM_APP+2`, `TIMER_*` y `ID_*` del menú no se renumeran |

## 3. Verificación obligatoria en cada fase

Ejecutar y adjuntar la salida real (no resumida) de:

```powershell
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
.\target\debug\bateria.exe --version   # debe imprimir: bateria 0.2.0
.\target\debug\bateria.exe --help      # texto idéntico al de Fase 1
```

Para comprobar rutas sin tocar datos reales, redirigir la variable antes de lanzar
el proceso:

```powershell
$tmp = Join-Path $env:TEMP ("check-" + (Get-Random)); New-Item -ItemType Directory $tmp | Out-Null
$env:LOCALAPPDATA = $tmp; .\target\debug\bateria.exe --dump; Get-ChildItem -Recurse $tmp
```

Debe aparecer `<tmp>\Bateria\config.toml` y `<tmp>\Bateria\bateria.log`, y la
primera línea del TOML debe ser `# Configuración de Batería`.

## 4. Mapa del código tras Fase 2

```
src/
  main.rs          arranque, bucle de mensajes, bandeja, menú, temporizadores, CLI, JSON
  meta.rs          identidad (clases, mutex, Run, rutas, versión)
  platform/        capa Win32 reutilizable (NO conoce la aplicación)
    paths.rs       %LOCALAPPDATA% y directorio de datos
    registry.rs    autostart (Run) y tema oscuro
    instance.rs    mutex de instancia única
    console.rs     print_line (AttachConsole + WriteFile)
    dpi.rs         per-monitor aware v2
    string.rs      wide / from_wide
  util.rs          log rotativo, local_time, user_ui_language
  config.rs        Config TOML de Batería
  panel.rs         panel GDI
  settings.rs      diálogo de configuración
  icon.rs          icono de batería generado con GDI
  i18n.rs          traducciones
  history.rs       histórico para el tiempo restante
  battery/         dominio de batería (se eliminará)
identity.rs        nombre visible compartido con build.rs
```

Reglas de dependencia vigentes:

- `platform/*` **no** puede importar `util`, `config`, `i18n`, `panel`, `settings` ni `battery`.
- `platform/*` recibe nombres y rutas como parámetros; nunca lee `meta`.
- `main.rs` sí usa `meta` + `platform` + el resto.

## 5. Reglas para el agente

1. Una fase = un commit independiente, con mensaje `Fase N: <título>` y trailer
   `Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>`.
2. No modificar comportamiento, UI, nombres, rutas, registro ni mensajes Win32.
3. No crear módulos vacíos ni traits/abstracciones sin un uso real demostrable.
4. No mover los `static` globales de `main.rs` hasta que la fase lo pida.
5. Antes de editar, revisar el diff; después, revisar el diff completo buscando
   cambios accidentales (especialmente en `main.rs`).
6. Si una comprobación no se puede ejecutar, decirlo: no darla por superada.
7. Dejar el árbol de trabajo limpio al terminar (`git status` vacío).

## 6. Trampas conocidas del entorno

- `w!` de `windows-core` **sí** funciona en contexto `const` (se usa en `meta.rs`
  para los `PCWSTR`). No hace falta construir `Vec<u16>` en los puntos de uso.
- `SetProcessDpiAwarenessContext` devuelve `Result<()>` en 0.58, no `BOOL`.
- `include!` no admite comentarios internos `//!` (rompen la compilación): usar `//`.
- Un bloque `unsafe { … }` al inicio de un brazo de `match` se parsea como sentencia;
  envolverlo en paréntesis o separarlo en un `let`.
- PowerShell: `Test-Path`, `-contains` y `Select-String` son insensibles a mayúsculas.
  Para comprobar nombres exactos usar `-ccontains` sobre `Get-ChildItem`.
- El repo `C:\si\trabajo\rust\bateria` contiene el código original (LF) por si hace
  falta comparar; la plantilla usa CRLF. Comparar normalizando EOL.
- Ejecutar `bateria.exe --dump` crea además `%LOCALAPPDATA%\Microsoft\Windows\Caches`
  (lo genera la enumeración BLE de WinRT, no nuestra configuración).

## 7. Fase 3 propuesta (no iniciada)

**Objetivo:** `lib.rs` + división de `main.rs` en `cli.rs` y `app.rs`, sin mover aún
los `static` ni cambiar mensajes.

Entregables y criterios de aceptación:

1. `src/lib.rs` que declare los módulos y permita tests de integración; `main.rs`
   queda como punto de entrada mínimo.
2. `src/cli.rs`: `Args { show, dump }`, `parse(&[String])`, `help()`, `version()`
   con tests unitarios. La salida de `--help` y `--version` debe ser **byte a byte**
   idéntica (comparar con el fichero de evidencias de Fase 1/2).
3. `src/app.rs`: agrupa la orquestación actual (refresco en hilo, `PostMessage`,
   temporizadores) **sin** cambiar su lógica; los `static` se quedan donde están en
   esta fase.
4. `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo test`,
   `cargo build --release` en verde; 37 tests siguen pasando más los nuevos de `cli`.
5. Prueba manual: bandeja, panel, menú y configuración siguen funcionando.

No adelantar Fase 4 (eliminación de `static`) ni Fase 5 (UI desacoplada).
