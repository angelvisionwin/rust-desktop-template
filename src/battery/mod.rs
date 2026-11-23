pub mod ble;
pub mod hid;
pub mod hidpp;
pub mod hidsys;
pub mod xinput;

use std::collections::HashMap;
use std::time::Duration;

use crate::config::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Keyboard,
    Mouse,
    Gamepad,
    Headset,
    Other,
}

impl DeviceKind {
    fn order(self) -> u8 {
        match self {
            DeviceKind::Keyboard => 0,
            DeviceKind::Mouse => 1,
            DeviceKind::Gamepad => 2,
            DeviceKind::Headset => 3,
            DeviceKind::Other => 4,
        }
    }

    /// Identificador estable para `--dump`.
    pub fn slug(self) -> &'static str {
        match self {
            DeviceKind::Keyboard => "keyboard",
            DeviceKind::Mouse => "mouse",
            DeviceKind::Gamepad => "gamepad",
            DeviceKind::Headset => "headset",
            DeviceKind::Other => "other",
        }
    }
}

/// Estado de carga comunicado por el dispositivo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChargeState {
    Charging,
    Finishing,
    Full,
}

#[derive(Clone, Debug)]
pub struct DeviceBattery {
    pub name: String,
    pub kind: DeviceKind,
    pub percent: u8,
    pub approx: bool,
    pub charge: Option<ChargeState>,
    /// Conexión por cable según el propio controlador (XInput).
    pub wired: bool,
    pub source: String,
    pub key: String,
    pub time_left: Option<Duration>,
}

impl DeviceBattery {
    /// El dispositivo informa de que está cargando.
    pub fn plugged(&self) -> bool {
        self.charge.is_some()
    }
}

pub fn refresh_all(config: &Config) -> Vec<DeviceBattery> {
    let mut items = Vec::new();
    collect("HID++", hidpp::scan, &mut items);
    collect("BLE", ble::scan, &mut items);
    collect("HID", hid::scan, &mut items);
    collect("XInput", xinput::scan, &mut items);

    items.retain(|device| !config.is_ignored(&device.name, &device.key));
    for device in items.iter_mut() {
        device.percent = device.percent.min(100);
    }

    // Dos dispositivos del mismo modelo comparten nombre: se desambigua la clave.
    let mut seen: HashMap<String, u8> = HashMap::new();
    for device in items.iter_mut() {
        let count = seen.entry(device.key.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            device.key = format!("{}#{}", device.key, count);
        }
    }

    items.sort_by(|a, b| {
        (a.kind.order(), a.name.to_lowercase()).cmp(&(b.kind.order(), b.name.to_lowercase()))
    });
    items
}

fn collect(source: &str, f: fn() -> Vec<DeviceBattery>, out: &mut Vec<DeviceBattery>) {
    match std::panic::catch_unwind(f) {
        Ok(found) => {
            crate::util::log(&format!("{source}: {} dispositivo(s)", found.len()));
            out.extend(found);
        }
        Err(_) => crate::util::log(&format!("{source}: error inesperado")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(key: &str, name: &str) -> DeviceBattery {
        DeviceBattery {
            name: name.to_string(),
            kind: DeviceKind::Mouse,
            percent: 50,
            approx: false,
            charge: None,
            wired: false,
            source: "prueba".to_string(),
            key: key.to_string(),
            time_left: None,
        }
    }

    #[test]
    fn duplicate_keys_get_a_suffix() {
        let mut items = [device("ble:abc", "Raton"), device("ble:abc", "Raton")];
        let mut seen: HashMap<String, u8> = HashMap::new();
        for item in items.iter_mut() {
            let count = seen.entry(item.key.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                item.key = format!("{}#{}", item.key, count);
            }
        }
        assert_eq!(items[0].key, "ble:abc");
        assert_eq!(items[1].key, "ble:abc#2");
    }

    #[test]
    fn plugged_reflects_charge_state() {
        let mut mouse = device("k", "n");
        assert!(!mouse.plugged());
        mouse.charge = Some(ChargeState::Charging);
        assert!(mouse.plugged());
    }
}
