use windows::Win32::UI::Input::XboxController::{
    BATTERY_DEVTYPE_GAMEPAD, BATTERY_LEVEL_EMPTY, BATTERY_LEVEL_FULL, BATTERY_LEVEL_LOW,
    BATTERY_LEVEL_MEDIUM, BATTERY_TYPE_DISCONNECTED, BATTERY_TYPE_WIRED,
    XINPUT_BATTERY_INFORMATION, XInputGetBatteryInformation,
};

use super::{DeviceBattery, DeviceKind};

pub fn scan() -> Vec<DeviceBattery> {
    let mut out = Vec::new();
    for slot in 0..4u32 {
        let mut info = XINPUT_BATTERY_INFORMATION::default();
        let rc = unsafe { XInputGetBatteryInformation(slot, BATTERY_DEVTYPE_GAMEPAD, &mut info) };
        if rc != 0 || info.BatteryType == BATTERY_TYPE_DISCONNECTED {
            continue;
        }
        let wired = info.BatteryType == BATTERY_TYPE_WIRED;
        let percent = if info.BatteryLevel == BATTERY_LEVEL_FULL {
            100
        } else if info.BatteryLevel == BATTERY_LEVEL_MEDIUM {
            60
        } else if info.BatteryLevel == BATTERY_LEVEL_LOW {
            25
        } else if info.BatteryLevel == BATTERY_LEVEL_EMPTY {
            5
        } else if wired {
            100
        } else {
            continue;
        };
        out.push(DeviceBattery {
            key: format!("xinput:{slot}"),
            name: format!("Mando Xbox {}", slot + 1),
            kind: DeviceKind::Gamepad,
            percent,
            approx: true,
            // XInput solo distingue cable/inalámbrico; no informa de la carga.
            charge: None,
            wired,
            source: "XInput".to_string(),
            time_left: None,
        });
    }
    out
}
