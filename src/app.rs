use std::collections::HashSet;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};
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

type Channel = (
    mpsc::Sender<Vec<DeviceBattery>>,
    Mutex<mpsc::Receiver<Vec<DeviceBattery>>>,
);

static DEVICES: OnceLock<Mutex<Vec<DeviceBattery>>> = OnceLock::new();
static UPDATED: OnceLock<Mutex<Option<(u16, u16)>>> = OnceLock::new();
static NOTIFIED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static CHANNEL: OnceLock<Channel> = OnceLock::new();
static CONFIG: OnceLock<Mutex<Config>> = OnceLock::new();
static HISTORY: OnceLock<Mutex<History>> = OnceLock::new();
static LAST_REFRESH: Mutex<Option<Instant>> = Mutex::new(None);
static BUSY: AtomicBool = AtomicBool::new(false);
static PANEL_HWND: OnceLock<isize> = OnceLock::new();
static TRAY_ICON: Mutex<isize> = Mutex::new(0);
static TRAY_RETIRED: Mutex<isize> = Mutex::new(0);
static TRAY_STATE: Mutex<Option<(Option<u8>, bool)>> = Mutex::new(None);
static HIDDEN_AT: Mutex<Option<Instant>> = Mutex::new(None);
static SHOW_ON_FIRST: AtomicBool = AtomicBool::new(false);
static PENDING_FROM_DEVICE: AtomicBool = AtomicBool::new(false);
static DEVICE_NOTIFICATIONS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

const GUID_DEVINTERFACE_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);
const GUID_DEVINTERFACE_BLUETOOTH: GUID = GUID::from_u128(0x0850302a_b344_4fda_9be9_90576b8d46f0);
const DBT_DEVTYP_DEVICEINTERFACE: u32 = 5;

fn devices() -> &'static Mutex<Vec<DeviceBattery>> {
    DEVICES.get_or_init(|| Mutex::new(Vec::new()))
}

fn updated() -> &'static Mutex<Option<(u16, u16)>> {
    UPDATED.get_or_init(|| Mutex::new(None))
}

fn notified() -> &'static Mutex<HashSet<String>> {
    NOTIFIED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn channel() -> &'static Channel {
    CHANNEL.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        (sender, Mutex::new(receiver))
    })
}

fn config_store() -> &'static Mutex<Config> {
    CONFIG.get_or_init(|| Mutex::new(Config::load()))
}

pub(crate) fn config() -> Config {
    config_store()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

fn history() -> &'static Mutex<History> {
    HISTORY.get_or_init(|| Mutex::new(History::load()))
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
        i18n::set_current(config().language());
        SHOW_ON_FIRST.store(args.show, Ordering::SeqCst);
        util::log(&format!(
            "inicio v{} — idioma {} — refresco {}s — umbral {}%",
            meta::VERSION,
            i18n::current(),
            config().refresh_interval_secs(),
            config().threshold()
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
            None,
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
            None,
        ) else {
            util::log("no se pudo crear el panel");
            return;
        };
        let _ = PANEL_HWND.set(panel.0 as isize);

        register_device_notifications(hwnd);

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
            *TRAY_ICON.lock().unwrap() = initial.0 as isize;
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
            (config().refresh_interval_secs() * 1000) as u32,
            None,
        );
        let _ = SetTimer(hwnd, TIMER_NOTIFICATIONS, 1000, None);

        request_refresh(hwnd, "inicio");

        let mut message = MSG::default();
        while GetMessageW(&mut message, HWND(std::ptr::null_mut()), 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        util::log("fin");
    }
}

/// Se suscribe a los cambios de interfaces HID y Bluetooth para refrescar
/// en cuanto se conecta o desconecta un dispositivo.
unsafe fn register_device_notifications(hwnd: HWND) {
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
                DEVICE_NOTIFICATIONS.lock().unwrap().push(handle.0 as isize);
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

fn request_refresh(hwnd: HWND, reason: &str) {
    if BUSY.swap(true, Ordering::SeqCst) {
        PENDING_FROM_DEVICE.store(true, Ordering::SeqCst);
        return;
    }
    *LAST_REFRESH.lock().unwrap() = Some(Instant::now());
    util::log(&format!("refresh ({reason})"));
    let sender = channel().0.clone();
    let address = hwnd.0 as isize;
    let config = config();
    std::thread::spawn(move || {
        let list = match std::panic::catch_unwind(|| refresh_all(&config)) {
            Ok(list) => list,
            Err(_) => {
                util::log("refresh: error inesperado");
                Vec::new()
            }
        };
        let _ = sender.send(list);
        BUSY.store(false, Ordering::SeqCst);
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

unsafe fn on_refreshed(hwnd: HWND) {
    let mut received = Vec::new();
    if let Ok(receiver) = channel().1.try_lock() {
        while let Ok(list) = receiver.try_recv() {
            received = list;
        }
    }

    {
        let mut history = history().lock().unwrap();
        let now = history::unix_now();
        for device in received.iter_mut() {
            history.record(&device.key, device.percent, now);
        }
        for device in received.iter_mut() {
            if !device.plugged() && config().show_time_remaining {
                device.time_left = history.estimate(&device.key, device.percent, now);
            }
        }
        history.save();
    }

    let changed = {
        let previous = devices().lock().unwrap();
        previous.len() != received.len()
            || previous.iter().zip(received.iter()).any(|(old, new)| {
                old.name != new.name
                    || old.percent != new.percent
                    || old.source != new.source
                    || old.charge != new.charge
            })
    };
    *devices().lock().unwrap() = received.clone();
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
    *updated().lock().unwrap() = Some((hours, minutes));

    update_tray(hwnd, &received);
    maybe_notify(hwnd, &received);

    if SHOW_ON_FIRST.swap(false, Ordering::SeqCst) {
        show_panel();
    } else if let Some(&address) = PANEL_HWND.get() {
        let panel = HWND(address as *mut c_void);
        if IsWindowVisible(panel).as_bool() {
            panel::resize(panel, received.len());
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(panel, None, true);
        }
    }
}

unsafe fn update_tray(hwnd: HWND, list: &[DeviceBattery]) {
    let minimum = list.iter().map(|device| device.percent).min();
    let charging = list.iter().any(DeviceBattery::plugged);

    let state = (minimum, charging);
    let changed = {
        let mut last = TRAY_STATE.lock().unwrap();
        if *last == Some(state) {
            false
        } else {
            *last = Some(state);
            true
        }
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
        let mut current = TRAY_ICON.lock().unwrap();
        let previous = std::mem::replace(&mut *current, new_icon.0 as isize);
        // El icono anterior se libera en la siguiente actualización: el shell
        // necesita tiempo para copiarlo y, si se destruye antes, sigue
        // mostrando el antiguo.
        let mut retired = TRAY_RETIRED.lock().unwrap();
        let older = std::mem::replace(&mut *retired, previous);
        if older != 0 {
            let _ = DestroyIcon(HICON(older as *mut c_void));
        }
    } else {
        let _ = DestroyIcon(new_icon);
    }
}

unsafe fn maybe_notify(hwnd: HWND, list: &[DeviceBattery]) {
    let threshold = config().threshold();
    let low = list
        .iter()
        .filter(|device| device.percent <= threshold && !device.plugged())
        .min_by_key(|device| device.percent);

    if let Some(device) = low {
        let mut alert = notified().lock().unwrap();
        if alert.insert(device.key.clone()) {
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

    let mut alert = notified().lock().unwrap();
    alert.retain(|key| {
        list.iter().any(|device| {
            &device.key == key && device.percent <= threshold.saturating_add(5) && !device.plugged()
        })
    });
}

fn show_panel() {
    let Some(&address) = PANEL_HWND.get() else {
        return;
    };
    let count = devices().lock().unwrap().len();
    unsafe {
        panel::show(HWND(address as *mut c_void), count);
    }
}

unsafe fn show_menu(hwnd: HWND) {
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
        ID_REFRESH => request_refresh(hwnd, "menú"),
        ID_PANEL => show_panel(),
        ID_AUTOSTART => {
            let enabled = platform::registry::autostart_enabled(meta::RUN_VALUE);
            let ok = platform::registry::set_autostart(meta::RUN_VALUE, !enabled);
            util::log(&format!("inicio con Windows -> {ok}"));
        }
        ID_CONFIG => {
            if let Some(updated) = settings::open(hwnd) {
                apply_config(hwnd, updated);
            }
        }
        ID_EXIT => {
            let _ = DestroyWindow(hwnd);
        }
        _ => {}
    }
}

/// Aplica en caliente la configuración guardada desde el diálogo.
unsafe fn apply_config(hwnd: HWND, updated: Config) {
    *config_store().lock().unwrap() = updated.clone();
    i18n::set_current(updated.language());

    let title = windows::core::HSTRING::from(i18n::text().app);
    let _ = SetWindowTextW(hwnd, &title);
    if let Some(&address) = PANEL_HWND.get() {
        let panel = HWND(address as *mut c_void);
        let _ = SetWindowTextW(panel, &title);
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(panel, None, true);
    }

    // El tooltip está en otro idioma y el umbral puede haber cambiado.
    *TRAY_STATE.lock().unwrap() = None;
    let list = devices().lock().unwrap().clone();
    update_tray(hwnd, &list);

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
    request_refresh(hwnd, "configuración");
}

extern "system" fn main_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_TRAY => {
                let event = lparam.0 as u32;
                if settings::is_open() {
                    settings::focus();
                    return LRESULT(0);
                }
                if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
                    let recently_hidden = HIDDEN_AT
                        .lock()
                        .unwrap()
                        .map(|when| when.elapsed().as_millis() < 350)
                        .unwrap_or(false);
                    if recently_hidden {
                        *HIDDEN_AT.lock().unwrap() = None;
                    } else {
                        show_panel();
                    }
                } else if event == WM_RBUTTONUP {
                    show_menu(hwnd);
                }
                LRESULT(0)
            }
            WM_REFRESHED => {
                on_refreshed(hwnd);
                if PENDING_FROM_DEVICE.swap(false, Ordering::SeqCst) {
                    request_refresh(hwnd, "pendiente");
                }
                LRESULT(0)
            }
            WM_TIMER => {
                match wparam.0 {
                    TIMER_REFRESH => request_refresh(hwnd, "temporizador"),
                    TIMER_NOTIFICATIONS => {
                        if battery::hidpp::take_notification() {
                            let recent = LAST_REFRESH
                                .lock()
                                .unwrap()
                                .map(|when| when.elapsed().as_millis() < MIN_REFRESH_GAP_MS)
                                .unwrap_or(false);
                            if recent {
                                PENDING_FROM_DEVICE.store(true, Ordering::SeqCst);
                            } else {
                                request_refresh(hwnd, "notificación");
                            }
                        }
                    }
                    TIMER_DEVICES => {
                        let _ = KillTimer(hwnd, TIMER_DEVICES);
                        // Puede haber cambiado el índice del dispositivo en el receptor.
                        battery::hidpp::force_full_scan();
                        request_refresh(hwnd, "cambio de dispositivos");
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
                for handle in DEVICE_NOTIFICATIONS.lock().unwrap().drain(..) {
                    let _ = UnregisterDeviceNotification(HDEVNOTIFY(handle as *mut c_void));
                }
                for slot in [&TRAY_ICON, &TRAY_RETIRED] {
                    let handle = std::mem::replace(&mut *slot.lock().unwrap(), 0);
                    if handle != 0 {
                        let _ = DestroyIcon(HICON(handle as *mut c_void));
                    }
                }
                let nid = base_nid(hwnd, NIF_MESSAGE);
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
                let _ = KillTimer(hwnd, TIMER_REFRESH);
                let _ = KillTimer(hwnd, TIMER_NOTIFICATIONS);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

extern "system" fn panel_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let list = devices().lock().unwrap().clone();
                let timestamp = *updated().lock().unwrap();
                panel::paint(hwnd, &list, timestamp);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_PRINTCLIENT => {
                let list = devices().lock().unwrap().clone();
                let timestamp = *updated().lock().unwrap();
                let dc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut c_void);
                panel::paint_to(hwnd, dc, &list, timestamp);
                LRESULT(0)
            }
            WM_ACTIVATE => {
                if (wparam.0 as u32 & 0xFFFF) == WA_INACTIVE {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    *HIDDEN_AT.lock().unwrap() = Some(Instant::now());
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if wparam.0 as u32 == VK_ESCAPE.0 as u32 {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    *HIDDEN_AT.lock().unwrap() = Some(Instant::now());
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

// --------------------------- modo consola ---------------------------

fn dump() {
    i18n::set_current(config().language());
    let list = refresh_all(&config());
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
