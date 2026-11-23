use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use crate::battery::{self, ChargeState, DeviceBattery, refresh_all};
use crate::config::Config;
use crate::history::{self, History};
use crate::{cli, i18n, icon, meta, panel, platform, settings, util};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_WARNING, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFY_ICON_DATA_FLAGS, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::WindowsAndMessaging::{
    DEV_BROADCAST_DEVICEINTERFACE_W, DEVICE_NOTIFY_WINDOW_HANDLE, HDEVNOTIFY,
    RegisterDeviceNotificationW, UnregisterDeviceNotification,
};
use windows::core::GUID;
use windows::core::PCWSTR;

const WM_TRAY: u32 = WM_APP + 1;
const WM_REFRESHED: u32 = WM_APP + 2;
const TRAY_ID: u32 = 1;
const TIMER_REFRESH: usize = 1;
const TIMER_NOTIFICATIONS: usize = 2;
const TIMER_DEVICES: usize = 3;
const ID_PANEL: usize = 1;
const ID_REFRESH: usize = 2;
const ID_AUTOSTART: usize = 3;
const ID_CONFIG: usize = 4;
const ID_EXIT: usize = 5;

const DBT_DEVNODES_CHANGED: u32 = 0x0007;
const DBT_DEVICEARRIVAL: u32 = 0x8000;
const DBT_DEVICEREMOVECOMPLETE: u32 = 0x8004;

const MIN_REFRESH_GAP_MS: u128 = 3000;

const GUID_DEVINTERFACE_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);
const GUID_DEVINTERFACE_BLUETOOTH: GUID = GUID::from_u128(0x0850302a_b344_4fda_9be9_90576b8d46f0);
const DBT_DEVTYP_DEVICEINTERFACE: u32 = 5;

/// Estado de la aplicación.
///
/// Vive en el hilo de la interfaz y su propietario es [`run`]: las ventanas solo
/// guardan un puntero prestado en `GWLP_USERDATA`. La mutabilidad interior es
/// necesaria porque los procedimientos de ventana se reentran (menús modales y
/// el diálogo de configuración), de modo que no puede existir más de una
/// referencia `&mut App` viva.
struct App {
    config: RefCell<Config>,
    history: RefCell<History>,

    devices: RefCell<Vec<DeviceBattery>>,
    updated: Cell<Option<(u16, u16)>>,

    tray_icon: Cell<isize>,
    tray_retired: Cell<isize>,
    tray_state: Cell<Option<(Option<u8>, bool)>>,
    notified: RefCell<HashSet<String>>,

    panel: Cell<Option<HWND>>,
    hidden_at: Cell<Option<Instant>>,

    refresh_tx: mpsc::Sender<Vec<DeviceBattery>>,
    refresh_rx: mpsc::Receiver<Vec<DeviceBattery>>,
    busy: Arc<AtomicBool>,
    pending_from_device: Cell<bool>,
    last_refresh: Cell<Option<Instant>>,
    show_on_first: Cell<bool>,

    device_notifications: RefCell<Vec<isize>>,
}

impl App {
    fn new() -> App {
        let (refresh_tx, refresh_rx) = mpsc::channel();
        App {
            config: RefCell::new(Config::load()),
            history: RefCell::new(History::load()),
            devices: RefCell::new(Vec::new()),
            updated: Cell::new(None),
            tray_icon: Cell::new(0),
            tray_retired: Cell::new(0),
            tray_state: Cell::new(None),
            notified: RefCell::new(HashSet::new()),
            panel: Cell::new(None),
            hidden_at: Cell::new(None),
            refresh_tx,
            refresh_rx,
            busy: Arc::new(AtomicBool::new(false)),
            pending_from_device: Cell::new(false),
            last_refresh: Cell::new(None),
            show_on_first: Cell::new(false),
            device_notifications: RefCell::new(Vec::new()),
        }
    }

    fn config_snapshot(&self) -> Config {
        self.config.borrow().clone()
    }

    fn devices_snapshot(&self) -> Vec<DeviceBattery> {
        self.devices.borrow().clone()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // Red de seguridad: `WM_DESTROY` ya deja ambos contadores a cero, así que
        // esto solo actúa si la ventana no llegó a destruirse.
        for slot in [&self.tray_icon, &self.tray_retired] {
            let handle = slot.replace(0);
            if handle != 0 {
                unsafe {
                    let _ = DestroyIcon(HICON(handle as *mut c_void));
                }
            }
        }
    }
}

/// Puntero al estado asociado a una ventana.
///
/// No promete ninguna duración de vida: el llamante debe garantizar que `App`
/// sigue vivo mientras use la referencia obtenida.
fn app_from(hwnd: HWND) -> Option<NonNull<App>> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut App;
    NonNull::new(raw)
}

/// Guarda el puntero recibido en `lpCreateParams` (se usa en `WM_NCCREATE`).
///
/// Devuelve `false` si la ventana no trae estado asociado.
unsafe fn attach_app(hwnd: HWND, lparam: LPARAM) -> bool {
    let create = lparam.0 as *const CREATESTRUCTW;
    if create.is_null() {
        return false;
    }
    let params = unsafe { (*create).lpCreateParams };
    if params.is_null() {
        return false;
    }
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, params as isize) };
    true
}

/// Retira el puntero de la ventana (se usa en `WM_NCDESTROY`). No libera nada.
unsafe fn detach_app(hwnd: HWND) -> Option<NonNull<App>> {
    let raw = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) } as *mut App;
    NonNull::new(raw)
}

/// Destruye la ventana si sigue existiendo y confirma que ya no queda ninguna.
fn destroy_window(hwnd: HWND) -> bool {
    unsafe {
        if !IsWindow(hwnd).as_bool() {
            return true;
        }
        if DestroyWindow(hwnd).is_err() {
            util::log("no se pudo destruir una ventana");
        }
        !IsWindow(hwnd).as_bool()
    }
}

pub(crate) fn run(args: cli::Args) {
    unsafe {
        let _ = platform::dpi::enable_per_monitor_v2();
        std::panic::set_hook(Box::new(|info| util::log(&format!("panic: {info}"))));

        if args.dump {
            dump();
            return;
        }

        if !platform::instance::acquire(meta::MUTEX_NAME) {
            return;
        }

        // Propietario único del estado: se libera al final del bucle de mensajes.
        let app = Box::new(App::new());
        let app_ptr = NonNull::from(&*app);
        let config = app.config_snapshot();
        i18n::set_current(config.language());
        app.show_on_first.set(args.show);
        util::log(&format!(
            "inicio v{} — idioma {} — refresco {}s — umbral {}%",
            meta::VERSION,
            i18n::current(),
            config.refresh_interval_secs(),
            config.threshold()
        ));

        let Some(instance) = GetModuleHandleW(None)
            .ok()
            .map(|module| HINSTANCE(module.0))
        else {
            util::log("no se pudo obtener el módulo");
            return;
        };

        let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();

        let main_class = meta::WINDOW_CLASS_MAIN;
        let main_wc = WNDCLASSW {
            lpfnWndProc: Some(main_wndproc),
            hInstance: instance,
            hCursor: cursor,
            lpszClassName: main_class,
            ..Default::default()
        };
        if RegisterClassW(&main_wc) == 0 {
            util::log("no se pudo registrar la ventana principal");
            return;
        }

        let panel_class = meta::WINDOW_CLASS_PANEL;
        let panel_wc = WNDCLASSW {
            style: CS_DROPSHADOW,
            lpfnWndProc: Some(panel_wndproc),
            hInstance: instance,
            hCursor: cursor,
            lpszClassName: panel_class,
            ..Default::default()
        };
        if RegisterClassW(&panel_wc) == 0 {
            util::log("no se pudo registrar el panel");
            return;
        }

        let window_title = windows::core::HSTRING::from(i18n::text().app);
        let params = Some(app_ptr.as_ptr() as *const c_void);
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            main_class,
            &window_title,
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            instance,
            params,
        ) else {
            util::log("no se pudo crear la ventana principal");
            return;
        };

        let Ok(panel) = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            panel_class,
            &window_title,
            WS_POPUP,
            0,
            0,
            0,
            0,
            hwnd,
            None,
            instance,
            params,
        ) else {
            util::log("no se pudo crear el panel");
            if !destroy_window(hwnd) {
                util::log("la ventana principal sigue viva; se conserva el estado");
                std::mem::forget(app);
            }
            return;
        };
        app.panel.set(Some(panel));

        register_device_notifications(&app, hwnd);

        let corner = 2u32; // DWMWCP_ROUND
        let _ = DwmSetWindowAttribute(
            panel,
            DWMWINDOWATTRIBUTE(33),
            &corner as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );

        let mut nid = base_nid(hwnd, NIF_MESSAGE | NIF_ICON | NIF_TIP);
        if let Some(initial) = icon::create(None, false) {
            nid.hIcon = initial;
            app.tray_icon.set(initial.0 as isize);
        }
        set_tip(
            &mut nid,
            &format!("{}: {}", i18n::text().app, i18n::text().empty_title),
        );
        if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            util::log("no se pudo crear el icono de la bandeja");
        }
        let _ = SetTimer(
            hwnd,
            TIMER_REFRESH,
            (config.refresh_interval_secs() * 1000) as u32,
            None,
        );
        let _ = SetTimer(hwnd, TIMER_NOTIFICATIONS, 1000, None);

        request_refresh(&app, hwnd, "inicio");

        let mut message = MSG::default();
        while GetMessageW(&mut message, HWND(std::ptr::null_mut()), 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        util::log("fin");

        // Ninguna ventana puede sobrevivir al estado: se destruyen antes de liberarlo.
        let panel_gone = destroy_window(panel);
        let main_gone = destroy_window(hwnd);
        if panel_gone && main_gone {
            drop(app);
        } else {
            util::log(
                "queda una ventana sin destruir; se conserva el estado hasta el fin del proceso",
            );
            std::mem::forget(app);
        }
    }
}

/// Se suscribe a los cambios de interfaces HID y Bluetooth para refrescar
/// en cuanto se conecta o desconecta un dispositivo.
unsafe fn register_device_notifications(app: &App, hwnd: HWND) {
    for (name, guid) in [
        ("HID", GUID_DEVINTERFACE_HID),
        ("Bluetooth", GUID_DEVINTERFACE_BLUETOOTH),
    ] {
        let filter = DEV_BROADCAST_DEVICEINTERFACE_W {
            dbcc_size: std::mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
            dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE,
            dbcc_reserved: 0,
            dbcc_classguid: guid,
            dbcc_name: [0],
        };
        match RegisterDeviceNotificationW(
            hwnd,
            &filter as *const _ as *const c_void,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        ) {
            Ok(handle) => {
                if let Ok(mut list) = app.device_notifications.try_borrow_mut() {
                    list.push(handle.0 as isize);
                }
                util::log(&format!("suscripción a cambios de {name} activa"));
            }
            Err(error) => util::log(&format!("no se pudo suscribir a {name}: {error}")),
        }
    }
}

fn base_nid(hwnd: HWND, flags: NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: flags,
        uCallbackMessage: WM_TRAY,
        ..Default::default()
    }
}

fn set_tip(nid: &mut NOTIFYICONDATAW, tip: &str) {
    let wide: Vec<u16> = tip.encode_utf16().take(127).collect();
    nid.szTip[..wide.len()].copy_from_slice(&wide);
    for slot in nid.szTip[wide.len()..].iter_mut() {
        *slot = 0;
    }
}

fn build_tip(list: &[DeviceBattery]) -> String {
    let text = i18n::text();
    if list.is_empty() {
        return format!("{} — {}", text.app, text.tip_no_devices);
    }
    let mut tip = format!("{} — ", text.app);
    for (index, device) in list.iter().enumerate() {
        if index > 0 {
            tip.push_str(" · ");
        }
        let name: String = device.name.chars().take(16).collect();
        tip.push_str(&format!("{} {}%", name, device.percent));
        if device.plugged() {
            tip.push_str(&format!(" ({})", text.tip_charging));
        }
    }
    if tip.chars().count() > 120 {
        tip = tip.chars().take(117).collect::<String>() + "…";
    }
    tip
}

fn request_refresh(app: &App, hwnd: HWND, reason: &str) {
    if app.busy.swap(true, Ordering::SeqCst) {
        app.pending_from_device.set(true);
        return;
    }
    app.last_refresh.set(Some(Instant::now()));
    util::log(&format!("refresh ({reason})"));
    let sender = app.refresh_tx.clone();
    let busy = Arc::clone(&app.busy);
    let address = hwnd.0 as isize;
    let config = app.config_snapshot();
    std::thread::spawn(move || {
        let list = match std::panic::catch_unwind(|| refresh_all(&config)) {
            Ok(list) => list,
            Err(_) => {
                util::log("refresh: error inesperado");
                Vec::new()
            }
        };
        let _ = sender.send(list);
        busy.store(false, Ordering::SeqCst);
        unsafe {
            let _ = PostMessageW(
                HWND(address as *mut c_void),
                WM_REFRESHED,
                WPARAM(0),
                LPARAM(0),
            );
        }
    });
}

unsafe fn on_refreshed(app: &App, hwnd: HWND) {
    let mut received = Vec::new();
    while let Ok(list) = app.refresh_rx.try_recv() {
        received = list;
    }

    let show_time_remaining = app.config_snapshot().show_time_remaining;
    if let Ok(mut history) = app.history.try_borrow_mut() {
        let now = history::unix_now();
        for device in received.iter_mut() {
            history.record(&device.key, device.percent, now);
        }
        for device in received.iter_mut() {
            if !device.plugged() && show_time_remaining {
                device.time_left = history.estimate(&device.key, device.percent, now);
            }
        }
        history.save();
    }

    let changed = match app.devices.try_borrow() {
        Ok(previous) => {
            previous.len() != received.len()
                || previous.iter().zip(received.iter()).any(|(old, new)| {
                    old.name != new.name
                        || old.percent != new.percent
                        || old.source != new.source
                        || old.charge != new.charge
                })
        }
        Err(_) => true,
    };
    if let Ok(mut devices) = app.devices.try_borrow_mut() {
        *devices = received.clone();
    }
    if changed {
        for device in &received {
            let charge = match device.charge {
                Some(state) => format!(" · {}", i18n::text().charge(state)),
                None => String::new(),
            };
            util::log(&format!(
                "{} = {}% ({} · {}{charge})",
                device.name,
                device.percent,
                i18n::text().kind(device.kind),
                device.source
            ));
        }
        util::log(&format!("refresh: {} dispositivo(s)", received.len()));
    }
    let (hours, minutes, _) = util::local_time();
    app.updated.set(Some((hours, minutes)));

    update_tray(app, hwnd, &received);
    maybe_notify(app, hwnd, &received);

    if app.show_on_first.replace(false) {
        show_panel(app);
    } else if let Some(panel) = app.panel.get()
        && IsWindowVisible(panel).as_bool()
    {
        panel::resize(panel, received.len());
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(panel, None, true);
    }
}

unsafe fn update_tray(app: &App, hwnd: HWND, list: &[DeviceBattery]) {
    let minimum = list.iter().map(|device| device.percent).min();
    let charging = list.iter().any(DeviceBattery::plugged);

    let state = (minimum, charging);
    let changed = if app.tray_state.get() == Some(state) {
        false
    } else {
        app.tray_state.set(Some(state));
        true
    };
    if changed {
        util::log(&format!("bandeja: nivel={:?} cargando={charging}", minimum));
    }

    let Some(new_icon) = icon::create(minimum, charging) else {
        return;
    };
    let mut nid = base_nid(hwnd, NIF_MESSAGE | NIF_ICON | NIF_TIP);
    nid.hIcon = new_icon;
    set_tip(&mut nid, &build_tip(list));
    if Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool() {
        let previous = app.tray_icon.replace(new_icon.0 as isize);
        // El icono anterior se libera en la siguiente actualización: el shell
        // necesita tiempo para copiarlo y, si se destruye antes, sigue
        // mostrando el antiguo.
        let older = app.tray_retired.replace(previous);
        if older != 0 {
            let _ = DestroyIcon(HICON(older as *mut c_void));
        }
    } else {
        let _ = DestroyIcon(new_icon);
    }
}

unsafe fn maybe_notify(app: &App, hwnd: HWND, list: &[DeviceBattery]) {
    let threshold = app.config_snapshot().threshold();
    let low = list
        .iter()
        .filter(|device| device.percent <= threshold && !device.plugged())
        .min_by_key(|device| device.percent);

    if let Some(device) = low {
        // El préstamo se cierra antes de hablar con el shell: es una llamada
        // externa y no debe coincidir con un `borrow` activo.
        let announce = match app.notified.try_borrow_mut() {
            Ok(mut alert) => alert.insert(device.key.clone()),
            Err(_) => false,
        };
        if announce {
            let text = i18n::text();
            let mut nid = base_nid(hwnd, NIF_INFO);
            let title: Vec<u16> = text.low_title.encode_utf16().collect();
            let body: Vec<u16> = text
                .low_body(&device.name, device.percent)
                .encode_utf16()
                .collect();
            nid.szInfoTitle[..title.len()].copy_from_slice(&title);
            nid.szInfo[..body.len()].copy_from_slice(&body);
            nid.dwInfoFlags = NIIF_WARNING;
            nid.Anonymous.uTimeout = 6000;
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    if let Ok(mut alert) = app.notified.try_borrow_mut() {
        alert.retain(|key| {
            list.iter().any(|device| {
                &device.key == key
                    && device.percent <= threshold.saturating_add(5)
                    && !device.plugged()
            })
        });
    }
}

fn show_panel(app: &App) {
    let Some(panel) = app.panel.get() else {
        return;
    };
    let count = app.devices.borrow().len();
    unsafe {
        panel::show(panel, count);
    }
}

unsafe fn show_menu(app: &App, hwnd: HWND) {
    let text = i18n::text();
    let Ok(menu) = CreatePopupMenu() else {
        return;
    };
    let _ = AppendMenuW(
        menu,
        MF_STRING,
        ID_PANEL,
        &windows::core::HSTRING::from(text.menu_show),
    );
    let _ = AppendMenuW(
        menu,
        MF_STRING,
        ID_REFRESH,
        &windows::core::HSTRING::from(text.menu_refresh),
    );
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let flags = if platform::registry::autostart_enabled(meta::RUN_VALUE) {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING | MF_UNCHECKED
    };
    let _ = AppendMenuW(
        menu,
        flags,
        ID_AUTOSTART,
        &windows::core::HSTRING::from(text.menu_autostart),
    );
    let _ = AppendMenuW(
        menu,
        MF_STRING,
        ID_CONFIG,
        &windows::core::HSTRING::from(text.menu_config),
    );
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(
        menu,
        MF_STRING,
        ID_EXIT,
        &windows::core::HSTRING::from(text.menu_exit),
    );

    let mut point = POINT::default();
    let _ = GetCursorPos(&mut point);
    let _ = SetForegroundWindow(hwnd);
    let command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        point.x,
        point.y,
        0,
        hwnd,
        None,
    );
    let _ = DestroyMenu(menu);
    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));

    match command.0 as usize {
        ID_REFRESH => request_refresh(app, hwnd, "menú"),
        ID_PANEL => show_panel(app),
        ID_AUTOSTART => {
            let enabled = platform::registry::autostart_enabled(meta::RUN_VALUE);
            let ok = platform::registry::set_autostart(meta::RUN_VALUE, !enabled);
            util::log(&format!("inicio con Windows -> {ok}"));
        }
        ID_CONFIG => {
            if let Some(updated) = settings::open(hwnd, app.config_snapshot()) {
                apply_config(app, hwnd, updated);
            }
        }
        ID_EXIT => {
            let closed = DestroyWindow(hwnd);
            if closed.is_err() {
                util::log("no se pudo cerrar la ventana principal");
            }
        }
        _ => {}
    }
}

/// Aplica en caliente la configuración guardada desde el diálogo.
unsafe fn apply_config(app: &App, hwnd: HWND, updated: Config) {
    if let Ok(mut config) = app.config.try_borrow_mut() {
        *config = updated.clone();
    }
    i18n::set_current(updated.language());

    let title = windows::core::HSTRING::from(i18n::text().app);
    let _ = SetWindowTextW(hwnd, &title);
    if let Some(panel) = app.panel.get() {
        let _ = SetWindowTextW(panel, &title);
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(panel, None, true);
    }

    // El tooltip está en otro idioma y el umbral puede haber cambiado.
    app.tray_state.set(None);
    let list = app.devices_snapshot();
    update_tray(app, hwnd, &list);

    let _ = SetTimer(
        hwnd,
        TIMER_REFRESH,
        (updated.refresh_interval_secs() * 1000) as u32,
        None,
    );
    util::log(&format!(
        "configuración aplicada — idioma {} — refresco {}s — umbral {}%",
        i18n::current(),
        updated.refresh_interval_secs(),
        updated.threshold()
    ));
    request_refresh(app, hwnd, "configuración");
}

extern "system" fn main_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            // Sin estado asociado no se admite la ventana.
            if !attach_app(hwnd, lparam) {
                util::log("no se pudo asociar el estado a la ventana principal");
                return LRESULT(0);
            }
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        if msg == WM_NCDESTROY {
            let _ = detach_app(hwnd);
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let Some(app_ptr) = app_from(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        // SAFETY: `run` mantiene el Box mientras esta ventana exista.
        let app = app_ptr.as_ref();

        match msg {
            WM_TRAY => {
                let event = lparam.0 as u32;
                if settings::is_open() {
                    settings::focus();
                    return LRESULT(0);
                }
                if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
                    let recently_hidden = app
                        .hidden_at
                        .get()
                        .map(|when| when.elapsed().as_millis() < 350)
                        .unwrap_or(false);
                    if recently_hidden {
                        app.hidden_at.set(None);
                    } else {
                        show_panel(app);
                    }
                } else if event == WM_RBUTTONUP {
                    show_menu(app, hwnd);
                }
                LRESULT(0)
            }
            WM_REFRESHED => {
                on_refreshed(app, hwnd);
                if app.pending_from_device.replace(false) {
                    request_refresh(app, hwnd, "pendiente");
                }
                LRESULT(0)
            }
            WM_TIMER => {
                match wparam.0 {
                    TIMER_REFRESH => request_refresh(app, hwnd, "temporizador"),
                    TIMER_NOTIFICATIONS => {
                        if battery::hidpp::take_notification() {
                            let recent = app
                                .last_refresh
                                .get()
                                .map(|when| when.elapsed().as_millis() < MIN_REFRESH_GAP_MS)
                                .unwrap_or(false);
                            if recent {
                                app.pending_from_device.set(true);
                            } else {
                                request_refresh(app, hwnd, "notificación");
                            }
                        }
                    }
                    TIMER_DEVICES => {
                        let _ = KillTimer(hwnd, TIMER_DEVICES);
                        // Puede haber cambiado el índice del dispositivo en el receptor.
                        battery::hidpp::force_full_scan();
                        request_refresh(app, hwnd, "cambio de dispositivos");
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DEVICECHANGE => {
                let event = wparam.0 as u32;
                let device_interface = lparam.0 == 0 || {
                    // DEV_BROADCAST_HDR: tamaño (u32) y tipo (u32).
                    let header = lparam.0 as *const u32;
                    header.add(1).read_unaligned() == DBT_DEVTYP_DEVICEINTERFACE
                };
                if device_interface
                    && (event == DBT_DEVNODES_CHANGED
                        || event == DBT_DEVICEARRIVAL
                        || event == DBT_DEVICEREMOVECOMPLETE)
                {
                    // Se agrupan ráfagas de eventos en un único refresco diferido.
                    let _ = SetTimer(hwnd, TIMER_DEVICES, 1200, None);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let handles: Vec<isize> = match app.device_notifications.try_borrow_mut() {
                    Ok(mut list) => list.drain(..).collect(),
                    Err(_) => Vec::new(),
                };
                for handle in handles {
                    let _ = UnregisterDeviceNotification(HDEVNOTIFY(handle as *mut c_void));
                }
                for slot in [&app.tray_icon, &app.tray_retired] {
                    let handle = slot.replace(0);
                    if handle != 0 {
                        let _ = DestroyIcon(HICON(handle as *mut c_void));
                    }
                }
                let nid = base_nid(hwnd, NIF_MESSAGE);
                if !Shell_NotifyIconW(NIM_DELETE, &nid).as_bool() {
                    util::log("no se pudo quitar el icono de la bandeja");
                }
                let _ = KillTimer(hwnd, TIMER_REFRESH);
                let _ = KillTimer(hwnd, TIMER_NOTIFICATIONS);
                if let Some(panel) = app.panel.get() {
                    let _ = destroy_window(panel);
                }
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

extern "system" fn panel_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            if !attach_app(hwnd, lparam) {
                util::log("no se pudo asociar el estado al panel");
                return LRESULT(0);
            }
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        if msg == WM_NCDESTROY {
            let _ = detach_app(hwnd);
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let Some(app_ptr) = app_from(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        // SAFETY: `run` mantiene el Box mientras esta ventana exista.
        let app = app_ptr.as_ref();

        match msg {
            WM_PAINT => {
                let list = app.devices_snapshot();
                let timestamp = app.updated.get();
                panel::paint(hwnd, &list, timestamp);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_PRINTCLIENT => {
                let list = app.devices_snapshot();
                let timestamp = app.updated.get();
                let dc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut c_void);
                panel::paint_to(hwnd, dc, &list, timestamp);
                LRESULT(0)
            }
            WM_ACTIVATE => {
                if (wparam.0 as u32 & 0xFFFF) == WA_INACTIVE {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    app.hidden_at.set(Some(Instant::now()));
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if wparam.0 as u32 == VK_ESCAPE.0 as u32 {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    app.hidden_at.set(Some(Instant::now()));
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

// --------------------------- modo consola ---------------------------

fn dump() {
    let config = Config::load();
    i18n::set_current(config.language());
    let list = refresh_all(&config);
    platform::console::print_line(&to_json(&list));
}

fn to_json(list: &[DeviceBattery]) -> String {
    if list.is_empty() {
        return "[\n]".to_string();
    }
    let mut out = String::from("[\n");
    for (index, device) in list.iter().enumerate() {
        if index > 0 {
            out.push_str(",\n");
        }
        out.push_str("  {");
        out.push_str(&format!("\"name\":{},", quote(&device.name)));
        out.push_str(&format!("\"kind\":{},", quote(device.kind.slug())));
        out.push_str(&format!("\"percent\":{},", device.percent));
        out.push_str(&format!("\"approx\":{},", device.approx));
        out.push_str(&format!("\"charging\":{},", device.plugged()));
        out.push_str(&format!("\"wired\":{},", device.wired));
        let charge = match device.charge {
            Some(ChargeState::Charging) => Some("charging"),
            Some(ChargeState::Finishing) => Some("finishing"),
            Some(ChargeState::Full) => Some("full"),
            None => None,
        };
        out.push_str(&format!(
            "\"charge\":{},",
            charge.map(quote).unwrap_or_else(|| "null".to_string())
        ));
        out.push_str(&format!("\"source\":{},", quote(&device.source)));
        out.push_str(&format!("\"key\":{},", quote(&device.key)));
        let seconds = device
            .time_left
            .map(|left| left.as_secs().to_string())
            .unwrap_or_else(|| "null".to_string());
        out.push_str(&format!("\"time_left_seconds\":{seconds}"));
        out.push('}');
    }
    out.push_str("\n]");
    out
}

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battery::DeviceKind;

    fn sample() -> DeviceBattery {
        DeviceBattery {
            name: "MX \"Keys\"\tS".to_string(),
            kind: DeviceKind::Keyboard,
            percent: 90,
            approx: false,
            charge: Some(ChargeState::Charging),
            wired: false,
            source: "Receptor Bolt".to_string(),
            key: "hidpp:046d:2".to_string(),
            time_left: None,
        }
    }

    #[test]
    fn json_escapes_special_characters() {
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(quote("a\tb"), "\"a\\tb\"");
        assert_eq!(quote("ñ"), "\"ñ\"");
    }

    #[test]
    fn json_contains_expected_fields() {
        let json = to_json(&[sample()]);
        assert!(json.starts_with("[\n  {"));
        assert!(json.contains("\"kind\":\"keyboard\""));
        assert!(json.contains("\"percent\":90"));
        assert!(json.contains("\"charging\":true"));
        assert!(json.contains("\"wired\":false"));
        assert!(json.contains("\"charge\":\"charging\""));
        assert!(json.contains("\"time_left_seconds\":null"));
        assert!(json.ends_with("\n]"));
    }

    #[test]
    fn empty_json_is_an_empty_array() {
        assert_eq!(to_json(&[]), "[\n]");
    }
}
