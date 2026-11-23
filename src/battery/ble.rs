use windows::Devices::Bluetooth::GenericAttributeProfile::GattCommunicationStatus;
use windows::Devices::Bluetooth::{
    BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice,
};
use windows::Devices::Enumeration::{DeviceInformation, DeviceInformationCollection};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::Result as WinResult;

use super::{ChargeState, DeviceBattery, DeviceKind};

mod guids {
    // Agrupación canónica de UUID (8-4-4-4-12), no la de bytes.
    #![allow(clippy::unusual_byte_groupings)]

    use windows::core::GUID;

    pub const BATTERY_SERVICE: GUID = GUID::from_u128(0x0000_180f_0000_1000_8000_00805f9b34fb);
    pub const BATTERY_LEVEL: GUID = GUID::from_u128(0x0000_2a19_0000_1000_8000_00805f9b34fb);
    pub const BATTERY_LEVEL_STATUS: GUID = GUID::from_u128(0x0000_2bed_0000_1000_8000_00805f9b34fb);
    pub const HID_SERVICE: GUID = GUID::from_u128(0x0000_1812_0000_1000_8000_00805f9b34fb);
    pub const AUDIO_SINK: GUID = GUID::from_u128(0x0000_110b_0000_1000_8000_00805f9b34fb);
    pub const HANDSFREE: GUID = GUID::from_u128(0x0000_111e_0000_1000_8000_00805f9b34fb);
    pub const HEADSET: GUID = GUID::from_u128(0x0000_1112_0000_1000_8000_00805f9b34fb);
    pub const AUDIO_STREAM_CONTROL: GUID = GUID::from_u128(0x0000_184e_0000_1000_8000_00805f9b34fb);
}

use guids::{
    AUDIO_SINK, AUDIO_STREAM_CONTROL, BATTERY_LEVEL, BATTERY_LEVEL_STATUS, BATTERY_SERVICE,
    HANDSFREE, HEADSET, HID_SERVICE,
};

pub fn scan() -> Vec<DeviceBattery> {
    let mut out = Vec::new();
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
        if let Err(e) = scan_all(&mut out) {
            crate::util::log(&format!("BLE: {e}"));
        }
    }
    out
}

unsafe fn scan_all(out: &mut Vec<DeviceBattery>) -> WinResult<()> {
    let selector = BluetoothLEDevice::GetDeviceSelector()?;
    let infos = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.get()?;
    let count = infos.Size().unwrap_or(0);
    for i in 0..count {
        if let Err(e) = scan_device(&infos, i, out) {
            crate::util::log(&format!("BLE dispositivo {i}: {e}"));
        }
    }
    Ok(())
}

unsafe fn scan_device(
    infos: &DeviceInformationCollection,
    index: u32,
    out: &mut Vec<DeviceBattery>,
) -> WinResult<()> {
    let info = infos.GetAt(index)?;
    let id = info.Id()?;
    let Ok(device) = BluetoothLEDevice::FromIdAsync(&id).and_then(|op| op.get()) else {
        return Ok(());
    };
    if device.ConnectionStatus() != Ok(BluetoothConnectionStatus::Connected) {
        let _ = device.Close();
        return Ok(());
    }

    let name = device.Name().map(|n| n.to_string()).unwrap_or_default();
    let services = device
        .GetGattServicesWithCacheModeAsync(BluetoothCacheMode::Uncached)
        .and_then(|op| op.get());
    let Ok(result) = services else {
        let _ = device.Close();
        return Ok(());
    };
    if result.Status()? != GattCommunicationStatus::Success {
        let _ = device.Close();
        return Ok(());
    }

    let services = result.Services()?;
    let mut has_hid = false;
    let mut has_audio = false;
    let mut percent = None;
    let mut charge = None;

    for j in 0..services.Size()? {
        let service = services.GetAt(j)?;
        let uuid = service.Uuid()?;
        if uuid == HID_SERVICE {
            has_hid = true;
        }
        if uuid == AUDIO_SINK
            || uuid == HANDSFREE
            || uuid == HEADSET
            || uuid == AUDIO_STREAM_CONTROL
        {
            has_audio = true;
        }
        if uuid != BATTERY_SERVICE {
            continue;
        }
        let Ok(chars) = service.GetCharacteristicsAsync().and_then(|op| op.get()) else {
            continue;
        };
        if chars.Status() != Ok(GattCommunicationStatus::Success) {
            continue;
        }
        let chars = chars.Characteristics()?;
        for k in 0..chars.Size()? {
            let characteristic = chars.GetAt(k)?;
            let uuid = characteristic.Uuid()?;
            if uuid != BATTERY_LEVEL && uuid != BATTERY_LEVEL_STATUS {
                continue;
            }
            let Some(data) = read_value(&characteristic) else {
                continue;
            };
            if uuid == BATTERY_LEVEL {
                if let Some(&value) = data.first()
                    && value <= 100
                {
                    percent = Some(value);
                }
            } else {
                let (status_percent, status_charge) = parse_level_status(&data);
                if percent.is_none() {
                    percent = status_percent;
                }
                if status_charge.is_some() {
                    charge = status_charge;
                }
            }
        }
    }

    let _ = device.Close();

    if let Some(percent) = percent {
        if name.is_empty() {
            return Ok(());
        }
        let kind = if has_audio {
            DeviceKind::Headset
        } else if has_hid {
            if name.to_lowercase().contains("mouse") || name.to_lowercase().contains("ratón") {
                DeviceKind::Mouse
            } else {
                DeviceKind::Keyboard
            }
        } else {
            DeviceKind::Other
        };
        out.push(DeviceBattery {
            key: format!("ble:{id}"),
            name,
            kind,
            percent,
            approx: false,
            charge,
            wired: false,
            source: "Bluetooth LE".to_string(),
            time_left: None,
        });
    }
    Ok(())
}

unsafe fn read_value(
    characteristic: &windows::Devices::Bluetooth::GenericAttributeProfile::GattCharacteristic,
) -> Option<Vec<u8>> {
    for mode in [BluetoothCacheMode::Uncached, BluetoothCacheMode::Cached] {
        let read = characteristic
            .ReadValueWithCacheModeAsync(mode)
            .and_then(|op| op.get());
        let Ok(read) = read else {
            continue;
        };
        if read.Status() != Ok(GattCommunicationStatus::Success) {
            continue;
        }
        let Ok(buffer) = read.Value() else {
            continue;
        };
        let Ok(reader) = DataReader::FromBuffer(&buffer) else {
            continue;
        };
        let length = reader.UnconsumedBufferLength().unwrap_or(0) as usize;
        let mut data = vec![0u8; length];
        if reader.ReadBytes(&mut data).is_err() {
            continue;
        }
        if !data.is_empty() {
            return Some(data);
        }
    }
    None
}

/// Decodifica *Battery Level Status* (0x2BED) según Bluetooth SIG BAS 1.1.
fn parse_level_status(data: &[u8]) -> (Option<u8>, Option<ChargeState>) {
    if data.is_empty() {
        return (None, None);
    }
    let flags = data[0];
    let identifier_present = flags & 0x01 != 0;
    let level_present = flags & 0x02 != 0;

    let power_state = if data.len() >= 3 {
        Some(u16::from_le_bytes([data[1], data[2]]))
    } else {
        None
    };
    let mut offset = 3;
    if identifier_present {
        offset += 2;
    }
    let level = if level_present && data.len() > offset {
        Some(data[offset])
    } else {
        None
    };

    let charge = power_state.and_then(|state| {
        let external_power = (state >> 2) & 0x03 == 1 || (state >> 4) & 0x03 == 1;
        match (state >> 6) & 0x03 {
            1 => Some(ChargeState::Charging),
            _ if external_power && level == Some(100) => Some(ChargeState::Full),
            _ if external_power => Some(ChargeState::Charging),
            _ => None,
        }
    });

    (level.filter(|&value| value <= 100), charge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_only_payload() {
        // flags: nivel presente; power state 0 (sin batería ni datos).
        let (percent, charge) = parse_level_status(&[0x02, 0x00, 0x00, 0x55]);
        assert_eq!(percent, Some(0x55));
        assert_eq!(charge, None);
    }

    #[test]
    fn charging_state_is_detected() {
        // flags: nivel; charge state = 1 (cargando) en los bits 6-7.
        let (percent, charge) = parse_level_status(&[0x02, 0x40, 0x00, 42]);
        assert_eq!(percent, Some(42));
        assert_eq!(charge, Some(ChargeState::Charging));
    }

    #[test]
    fn external_power_with_full_level_is_full() {
        // wired external power = 1 (bits 2-3), nivel 100.
        let (percent, charge) = parse_level_status(&[0x02, 0x04, 0x00, 100]);
        assert_eq!(percent, Some(100));
        assert_eq!(charge, Some(ChargeState::Full));
    }

    #[test]
    fn identifier_shifts_the_level_byte() {
        // flags: identificador + nivel presentes.
        let (percent, charge) = parse_level_status(&[0x03, 0x00, 0x00, 0xAA, 0xBB, 77]);
        assert_eq!(percent, Some(77));
        assert_eq!(charge, None);
    }

    #[test]
    fn missing_level_and_short_payloads_are_safe() {
        assert_eq!(parse_level_status(&[]), (None, None));
        assert_eq!(parse_level_status(&[0x02]), (None, None));
        // Nivel fuera de rango se descarta.
        let (percent, _) = parse_level_status(&[0x02, 0x00, 0x00, 200]);
        assert_eq!(percent, None);
    }

    #[test]
    fn discharging_without_external_power_has_no_charge_state() {
        // charge state = 2 (discharging active), sin alimentación externa.
        let (_, charge) = parse_level_status(&[0x02, 0x80, 0x00, 60]);
        assert_eq!(charge, None);
    }
}
