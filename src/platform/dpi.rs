//! Conciencia de DPI del proceso.

use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

/// Declara el proceso como «per-monitor aware v2».
///
/// Debe llamarse antes de crear cualquier ventana.
pub fn enable_per_monitor_v2() -> bool {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_ok() }
}
