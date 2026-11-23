use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HANDLE};

use super::hidsys;
use super::{ChargeState, DeviceBattery, DeviceKind};

const LOGITECH: u16 = 0x046D;
const VENDOR_PAGE: u16 = 0xFF00;
const SHORT_REPORT: u8 = 0x10;
const LONG_REPORT: u8 = 0x11;

static NEXT_SW: AtomicU8 = AtomicU8::new(1);
static SESSION: Mutex<Option<Session>> = Mutex::new(None);
static LISTENER: Once = Once::new();
static NOTIFICATION: AtomicBool = AtomicBool::new(false);
static FORCE_FULL: AtomicBool = AtomicBool::new(false);

fn next_sw() -> u8 {
    1 + (NEXT_SW.fetch_add(1, Ordering::Relaxed) % 15)
}

#[derive(Clone, Copy, PartialEq)]
enum Chan {
    Short,
    Long,
}

struct Receiver {
    group: String,
    short: Option<HANDLE>,
    long: Option<HANDLE>,
    product_id: u16,
}

impl Drop for Receiver {
    fn drop(&mut self) {
        unsafe {
            if let Some(h) = self.short.take() {
                let _ = CloseHandle(h);
            }
            if let Some(h) = self.long.take() {
                let _ = CloseHandle(h);
            }
        }
    }
}

struct Session {
    receivers: Vec<Receiver>,
    signature: Vec<String>,
    /// Índices del receptor donde alguna vez se encontró un dispositivo.
    known: HashMap<String, Vec<u8>>,
}

// Los handles de HID son del proceso y se pueden usar desde cualquier hilo.
unsafe impl Send for Receiver {}
unsafe impl Send for Session {}

enum Reply {
    Data(Vec<u8>),
    Error,
    Timeout,
}

/// Lectura de batería decodificada de un dispositivo.
#[derive(Debug, PartialEq, Eq)]
struct Reading {
    percent: u8,
    approx: bool,
    charge: Option<ChargeState>,
}

/// Fuerza un barrido completo de índices en la próxima lectura.
pub fn force_full_scan() {
    FORCE_FULL.store(true, Ordering::SeqCst);
}

pub fn scan() -> Vec<DeviceBattery> {
    let receivers = build_receivers();
    let signature: Vec<String> = receivers
        .iter()
        .map(|receiver| format!("{}:{:04x}", receiver.group, receiver.product_id))
        .collect();

    let mut guard = match SESSION.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    FORCE_FULL.store(false, Ordering::SeqCst);
    let known = match guard.as_ref() {
        Some(previous) if previous.signature == signature => previous.known.clone(),
        _ => HashMap::new(),
    };
    *guard = Some(Session {
        receivers,
        signature,
        known,
    });

    let mut found: Vec<(String, u8, DeviceBattery)> = Vec::new();
    if let Some(session) = guard.as_ref() {
        for receiver in &session.receivers {
            let key = receiver.group.clone();
            // Con índices ya conocidos la lectura es rápida; si no, se sondean todos.
            let candidates: Vec<u8> = match session.known.get(&key) {
                Some(indices) if !indices.is_empty() => indices.clone(),
                _ => (1..=8).collect(),
            };
            for idx in candidates {
                if let Some(device) = probe(receiver, idx) {
                    found.push((key.clone(), idx, device));
                }
            }
        }
    }

    if let Some(session) = guard.as_mut() {
        for (key, idx, _) in &found {
            let list = session.known.entry(key.clone()).or_default();
            if !list.contains(idx) {
                list.push(*idx);
            }
        }
    }
    drop(guard);

    ensure_listener();
    found.into_iter().map(|(_, _, device)| device).collect()
}

/// Devuelve `true` una sola vez por notificación recibida del dispositivo.
pub fn take_notification() -> bool {
    NOTIFICATION.swap(false, Ordering::SeqCst)
}

fn build_receivers() -> Vec<Receiver> {
    let mut receivers: HashMap<String, Receiver> = HashMap::new();
    for mut dev in hidsys::enumerate() {
        if dev.caps.UsagePage != VENDOR_PAGE || dev.vendor_id != LOGITECH {
            continue;
        }
        if dev.caps.Usage != 1 && dev.caps.Usage != 2 {
            continue;
        }
        let entry = receivers
            .entry(dev.group.clone())
            .or_insert_with(|| Receiver {
                group: dev.group.clone(),
                short: None,
                long: None,
                product_id: dev.product_id,
            });
        if dev.caps.Usage == 1 {
            entry.short = dev.handle.take();
        } else {
            entry.long = dev.handle.take();
        }
    }
    let mut list: Vec<Receiver> = receivers
        .into_values()
        .filter(|receiver| receiver.short.is_some() || receiver.long.is_some())
        .collect();
    list.sort_by(|a, b| a.group.cmp(&b.group));
    list
}

/// Hilo que escucha notificaciones HID++ para refrescar al instante.
fn ensure_listener() {
    LISTENER.call_once(|| {
        std::thread::Builder::new()
            .name("hidpp-listener".to_string())
            .spawn(|| {
                loop {
                    std::thread::sleep(Duration::from_millis(150));
                    let Ok(mut guard) = SESSION.lock() else {
                        continue;
                    };
                    let Some(session) = guard.as_mut() else {
                        continue;
                    };
                    for receiver in &session.receivers {
                        let Some(handle) = receiver.long else {
                            continue;
                        };
                        // El mutex evita robar respuestas de una petición en curso.
                        let Some(buf) = (unsafe { hidsys::read_report(handle, 60) }) else {
                            continue;
                        };
                        if is_device_notification(&buf) {
                            NOTIFICATION.store(true, Ordering::SeqCst);
                        }
                    }
                }
            })
            .ok();
    });
}

fn is_device_notification(buf: &[u8]) -> bool {
    buf.len() >= 4 && (1..=8).contains(&buf[1]) && buf[3] == 0x00
}

fn probe(receiver: &Receiver, idx: u8) -> Option<DeviceBattery> {
    // Los dispositivos inalámbricos pueden tardar ~1 s en despertar y responder.
    match request(receiver, idx, 0x00, 0x01, &[], 1000) {
        Reply::Data(_) => {}
        _ => return None,
    }

    let reading = read_reading(receiver, idx)?;
    let name_feature = feature(receiver, idx, 0x0005).unwrap_or(0);
    let name = if name_feature != 0 {
        read_name(receiver, idx, name_feature)
            .unwrap_or_else(|| format!("Dispositivo Logitech {idx}"))
    } else {
        format!("Dispositivo Logitech {idx}")
    };
    let kind = if name_feature != 0 {
        read_kind(receiver, idx, name_feature).unwrap_or(DeviceKind::Other)
    } else {
        DeviceKind::Other
    };

    let source = if receiver.long.is_some() {
        "Receptor Bolt"
    } else {
        "Receptor USB"
    };

    Some(DeviceBattery {
        key: format!("hidpp:{:04x}:{idx}", receiver.product_id),
        name,
        kind,
        percent: reading.percent,
        approx: reading.approx,
        charge: reading.charge,
        wired: false,
        source: source.to_string(),
        time_left: None,
    })
}

fn read_reading(receiver: &Receiver, idx: u8) -> Option<Reading> {
    if let Some(f) = feature(receiver, idx, 0x1004).filter(|&f| f != 0)
        && let Reply::Data(buf) = request(receiver, idx, f, 0x01, &[], 500)
        && let Some(reading) = parse_unified(payload(&buf))
    {
        return Some(reading);
    }

    if let Some(f) = feature(receiver, idx, 0x1001).filter(|&f| f != 0)
        && let Reply::Data(buf) = request(receiver, idx, f, 0x00, &[], 500)
        && let Some(reading) = parse_voltage(payload(&buf))
    {
        return Some(reading);
    }

    if let Some(f) = feature(receiver, idx, 0x1000).filter(|&f| f != 0)
        && let Reply::Data(buf) = request(receiver, idx, f, 0x00, &[], 500)
        && let Some(reading) = parse_status(payload(&buf))
    {
        return Some(reading);
    }
    None
}

/// Datos tras la cabecera de 4 bytes de la respuesta HID++.
fn payload(buf: &[u8]) -> &[u8] {
    buf.get(4..).unwrap_or(&[])
}

fn charge_from_status(status: u8) -> Option<ChargeState> {
    match status {
        1 | 4 => Some(ChargeState::Charging),
        2 => Some(ChargeState::Finishing),
        3 => Some(ChargeState::Full),
        _ => None,
    }
}

/// Feature 0x1004: nivel, nivel aproximado y estado de carga.
fn parse_unified(data: &[u8]) -> Option<Reading> {
    if data.len() < 3 {
        return None;
    }
    let discharge = data[0];
    let level = data[1];
    let charge = charge_from_status(data[2]);
    if (1..=100).contains(&discharge) {
        return Some(Reading {
            percent: discharge,
            approx: false,
            charge,
        });
    }
    let percent = level_to_percent(level)?;
    Some(Reading {
        percent,
        approx: true,
        charge,
    })
}

/// Feature 0x1001: voltaje en mV y bits de estado.
fn parse_voltage(data: &[u8]) -> Option<Reading> {
    if data.len() < 3 {
        return None;
    }
    let millivolts = ((data[0] as u32) << 8) | data[1] as u32;
    let flags = data[2];
    if millivolts <= 3000 {
        return None;
    }
    let charge = if flags & 0x80 != 0 {
        Some(if flags & 0x03 == 1 {
            ChargeState::Full
        } else {
            ChargeState::Charging
        })
    } else {
        None
    };
    Some(Reading {
        percent: voltage_to_percent(millivolts),
        approx: true,
        charge,
    })
}

/// Feature 0x1000: nivel de descarga, siguiente nivel y estado.
fn parse_status(data: &[u8]) -> Option<Reading> {
    if data.len() < 3 {
        return None;
    }
    let percent = data[0];
    if !(1..=100).contains(&percent) {
        return None;
    }
    Some(Reading {
        percent,
        approx: false,
        charge: charge_from_status(data[2]),
    })
}

fn level_to_percent(level: u8) -> Option<u8> {
    Some(match level {
        8 => 100,
        4 => 70,
        2 => 30,
        1 => 10,
        0 => 0,
        _ => return None,
    })
}

fn voltage_to_percent(mv: u32) -> u8 {
    const TABLE: [(u32, u8); 13] = [
        (4186, 100),
        (4067, 90),
        (3989, 80),
        (3922, 70),
        (3859, 60),
        (3811, 50),
        (3778, 40),
        (3751, 30),
        (3717, 20),
        (3671, 10),
        (3646, 5),
        (3579, 2),
        (3500, 0),
    ];
    if mv >= TABLE[0].0 {
        return TABLE[0].1;
    }
    for pair in TABLE.windows(2) {
        let (high_mv, high_pct) = pair[0];
        let (low_mv, low_pct) = pair[1];
        if mv <= high_mv && mv >= low_mv {
            let pct = low_pct as f32
                + (high_pct as f32 - low_pct as f32) * (mv as f32 - low_mv as f32)
                    / (high_mv as f32 - low_mv as f32);
            return pct.round() as u8;
        }
    }
    TABLE[TABLE.len() - 1].1
}

fn read_name(receiver: &Receiver, idx: u8, feature: u8) -> Option<String> {
    let Reply::Data(head) = request(receiver, idx, feature, 0x00, &[], 500) else {
        return None;
    };
    if head.len() < 5 {
        return None;
    }
    let total = head[4] as usize;
    if total == 0 || total > 128 {
        return None;
    }
    let mut bytes = Vec::with_capacity(total);
    while bytes.len() < total {
        let offset = bytes.len() as u8;
        let Reply::Data(chunk) = request(receiver, idx, feature, 0x01, &[offset], 500) else {
            break;
        };
        if chunk.len() <= 4 {
            break;
        }
        let body = &chunk[4..];
        let need = total - bytes.len();
        bytes.extend_from_slice(&body[..need.min(body.len())]);
    }
    let name = String::from_utf8_lossy(&bytes)
        .trim_matches('\0')
        .trim()
        .to_string();
    (!name.is_empty()).then_some(name)
}

fn read_kind(receiver: &Receiver, idx: u8, feature: u8) -> Option<DeviceKind> {
    let Reply::Data(buf) = request(receiver, idx, feature, 0x02, &[], 500) else {
        return None;
    };
    if buf.len() < 5 {
        return None;
    }
    Some(match buf[4] {
        0x00 | 0x02 => DeviceKind::Keyboard,
        0x03..=0x05 => DeviceKind::Mouse,
        _ => DeviceKind::Other,
    })
}

fn feature(receiver: &Receiver, idx: u8, feature: u16) -> Option<u8> {
    let params = [(feature >> 8) as u8, (feature & 0xFF) as u8];
    match request(receiver, idx, 0x00, 0x00, &params, 500) {
        Reply::Data(buf) if buf.len() >= 5 => Some(buf[4]),
        _ => None,
    }
}

fn build_frame(idx: u8, feature: u8, function: u8, sw: u8, params: &[u8], long: bool) -> [u8; 20] {
    let mut frame = [0u8; 20];
    frame[0] = if long { LONG_REPORT } else { SHORT_REPORT };
    frame[1] = idx;
    frame[2] = feature;
    frame[3] = (function << 4) | sw;
    let size = if long { 20 } else { 7 };
    let n = params.len().min(size - 4);
    frame[4..4 + n].copy_from_slice(&params[..n]);
    frame
}

fn request(
    receiver: &Receiver,
    idx: u8,
    feature: u8,
    function: u8,
    params: &[u8],
    timeout_ms: u32,
) -> Reply {
    let sw = next_sw();

    let mut channels = Vec::new();
    if receiver.long.is_some() {
        channels.push(Chan::Long);
    }
    if receiver.short.is_some() {
        channels.push(Chan::Short);
    }

    for chan in channels {
        let handle = match chan {
            Chan::Long => receiver.long,
            Chan::Short => receiver.short,
        };
        let Some(handle) = handle else { continue };

        let long = chan == Chan::Long;
        let frame = build_frame(idx, feature, function, sw, params, long);
        if !unsafe { hidsys::write_report(handle, &frame[..if long { 20 } else { 7 }]) } {
            continue;
        }

        let mut read_handles = vec![handle];
        if let Some(other) = [receiver.short, receiver.long]
            .into_iter()
            .flatten()
            .find(|&h| h != handle)
        {
            read_handles.push(other);
        }

        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        while Instant::now() < deadline {
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(20, 150) as u32;
            for &read_handle in &read_handles {
                let Some(buf) = (unsafe { hidsys::read_report(read_handle, remaining) }) else {
                    continue;
                };
                match matches_request(&buf, idx, feature, (function << 4) | sw) {
                    Some(Reply::Data(data)) => return Reply::Data(data),
                    Some(Reply::Error) => return Reply::Error,
                    Some(Reply::Timeout) | None => {}
                }
            }
        }
    }
    Reply::Timeout
}

/// Comprueba si el reporte responde a la petición indicada.
fn matches_request(buf: &[u8], idx: u8, feature: u8, func_sw: u8) -> Option<Reply> {
    if buf.len() < 5 || buf[1] != idx {
        return None;
    }
    if buf[2] == feature && buf[3] == func_sw {
        return Some(Reply::Data(buf.to_vec()));
    }
    if (buf[2] == 0x8F || buf[2] == 0xFF) && buf[3] == feature && buf[4] == func_sw {
        return Some(Reply::Error);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_payload_reports_percentage_and_charge() {
        // 0x5A = 90 %, nivel 8, estado 0 (descargando).
        let reading = parse_unified(&[90, 8, 0, 0]).unwrap();
        assert_eq!(reading.percent, 90);
        assert!(!reading.approx);
        assert_eq!(reading.charge, None);

        let charging = parse_unified(&[42, 8, 1, 0]).unwrap();
        assert_eq!(charging.charge, Some(ChargeState::Charging));

        let finishing = parse_unified(&[99, 8, 2, 0]).unwrap();
        assert_eq!(finishing.charge, Some(ChargeState::Finishing));

        let full = parse_unified(&[100, 8, 3, 0]).unwrap();
        assert_eq!(full.charge, Some(ChargeState::Full));

        let below_optimal = parse_unified(&[60, 8, 4, 0]).unwrap();
        assert_eq!(below_optimal.charge, Some(ChargeState::Charging));

        // Sin porcentaje utilizable se usa el nivel aproximado.
        let approx = parse_unified(&[0, 4, 0, 0]).unwrap();
        assert_eq!(approx.percent, 70);
        assert!(approx.approx);

        assert!(parse_unified(&[]).is_none());
        assert!(parse_unified(&[0, 9, 0, 0]).is_none());
    }

    #[test]
    fn voltage_payload_uses_high_bit_for_charging() {
        // 4067 mV => 90 %.
        let reading = parse_voltage(&[0x0F, 0xE3, 0x00]).unwrap();
        assert_eq!(reading.percent, 90);
        assert!(reading.approx);
        assert_eq!(reading.charge, None);

        let charging = parse_voltage(&[0x0F, 0xE3, 0x80]).unwrap();
        assert_eq!(charging.charge, Some(ChargeState::Charging));

        let full = parse_voltage(&[0x0F, 0xE3, 0x81]).unwrap();
        assert_eq!(full.charge, Some(ChargeState::Full));

        assert!(parse_voltage(&[0x00, 0x10, 0x00]).is_none());
    }

    #[test]
    fn status_payload_reads_first_byte() {
        let reading = parse_status(&[77, 76, 0]).unwrap();
        assert_eq!(reading.percent, 77);
        assert_eq!(reading.charge, None);
        assert!(parse_status(&[0, 0, 0]).is_none());
        assert!(parse_status(&[101, 0, 0]).is_none());
    }

    #[test]
    fn voltage_table_matches_reference_points() {
        assert_eq!(voltage_to_percent(4200), 100);
        assert_eq!(voltage_to_percent(4067), 90);
        assert_eq!(voltage_to_percent(3811), 50);
        assert_eq!(voltage_to_percent(3400), 0);
        // 4030 mV cae entre 3989 mV (80 %) y 4067 mV (90 %).
        assert_eq!(voltage_to_percent(4030), 85);
    }

    #[test]
    fn build_frame_places_function_and_software_id() {
        let frame = build_frame(4, 0x07, 1, 5, &[0xAA], true);
        assert_eq!(&frame[..5], &[0x11, 4, 0x07, 0x15, 0xAA]);
        let short = build_frame(2, 0x00, 0, 1, &[0x10, 0x00], false);
        assert_eq!(&short[..6], &[0x10, 2, 0x00, 0x01, 0x10, 0x00]);
    }

    #[test]
    fn matches_only_matching_reports() {
        let ok = [0x11, 2, 0x07, 0x15, 0x5A, 8, 0];
        assert!(matches!(
            matches_request(&ok, 2, 0x07, 0x15),
            Some(Reply::Data(_))
        ));
        assert!(matches_request(&ok, 3, 0x07, 0x15).is_none());
        assert!(matches_request(&ok, 2, 0x07, 0x16).is_none());

        let short_error = [0x10, 2, 0x8F, 0x07, 0x15, 0, 0];
        assert!(matches!(
            matches_request(&short_error, 2, 0x07, 0x15),
            Some(Reply::Error)
        ));
        let long_error = [0x11, 2, 0xFF, 0x07, 0x15, 0, 0];
        assert!(matches!(
            matches_request(&long_error, 2, 0x07, 0x15),
            Some(Reply::Error)
        ));
    }

    #[test]
    fn detects_device_notifications() {
        assert!(is_device_notification(&[0x11, 3, 0x07, 0x00, 0]));
        assert!(!is_device_notification(&[0x11, 0xFF, 0x07, 0x00, 0]));
        assert!(!is_device_notification(&[0x11, 3, 0x07, 0x15, 0]));
        assert!(!is_device_notification(&[0x11, 3]));
    }
}
