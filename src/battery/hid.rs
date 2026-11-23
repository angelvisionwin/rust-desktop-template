use std::ffi::c_void;

use windows::Win32::Devices::HumanInterfaceDevice::{
    HIDP_VALUE_CAPS, HidP_Feature, HidP_GetUsageValue, HidP_GetValueCaps, HidP_Input,
};

use super::hidsys::{self, HidDevice};
use super::{DeviceBattery, DeviceKind};

const VENDOR_PAGES: [u16; 2] = [0xFF00, 0xFFA0];
const GENERIC_DEVICE_CONTROLS: u16 = 0x06;
const BATTERY_STRENGTH: u16 = 0x20;

pub fn scan() -> Vec<DeviceBattery> {
    let mut out = Vec::new();
    for device in hidsys::enumerate() {
        if VENDOR_PAGES.contains(&device.caps.UsagePage) {
            continue;
        }
        if device.product.is_empty() || device.handle.is_none() {
            continue;
        }
        let Some(percent) = read_battery(&device) else {
            continue;
        };
        out.push(DeviceBattery {
            key: format!("hid:{}", device.path),
            name: device.product.clone(),
            kind: kind_of(&device),
            percent,
            approx: false,
            charge: None,
            wired: false,
            source: "USB HID".to_string(),
            time_left: None,
        });
    }
    out
}

fn read_battery(device: &HidDevice) -> Option<u8> {
    unsafe {
        let handle = device.handle?;
        if let Some(caps) = find_caps(device, HidP_Feature, device.caps.NumberFeatureValueCaps) {
            let length = device.caps.FeatureReportByteLength as usize;
            if length > 0 {
                let mut report = vec![0u8; length];
                report[0] = caps.ReportID;
                let read = windows::Win32::Devices::HumanInterfaceDevice::HidD_GetFeature(
                    handle,
                    report.as_mut_ptr() as *mut c_void,
                    length as u32,
                )
                .as_bool();
                if read && let Some(value) = usage_value(device, HidP_Feature, &report) {
                    return Some(value);
                }
            }
        }

        if find_caps(device, HidP_Input, device.caps.NumberInputValueCaps).is_some() {
            for _ in 0..4 {
                let Some(report) = hidsys::read_report(handle, 220) else {
                    continue;
                };
                if let Some(value) = usage_value(device, HidP_Input, &report) {
                    return Some(value);
                }
            }
        }
    }
    None
}

unsafe fn find_caps(
    device: &HidDevice,
    kind: windows::Win32::Devices::HumanInterfaceDevice::HIDP_REPORT_TYPE,
    count: u16,
) -> Option<HIDP_VALUE_CAPS> {
    if count == 0 {
        return None;
    }
    let mut caps = vec![HIDP_VALUE_CAPS::default(); count as usize];
    let mut length = count;
    let status = HidP_GetValueCaps(kind, caps.as_mut_ptr(), &mut length, device.preparsed);
    if !status.is_ok() {
        return None;
    }
    caps.truncate(length as usize);
    caps.into_iter().find(|caps| {
        caps.UsagePage == GENERIC_DEVICE_CONTROLS && usage_matches(caps, BATTERY_STRENGTH)
    })
}

fn usage_matches(caps: &HIDP_VALUE_CAPS, usage: u16) -> bool {
    // SAFETY: `IsRange` indica qué miembro de la unión es válido.
    unsafe {
        if !caps.IsRange.as_bool() {
            caps.Anonymous.NotRange.Usage == usage
        } else {
            (caps.Anonymous.Range.UsageMin..=caps.Anonymous.Range.UsageMax).contains(&usage)
        }
    }
}

unsafe fn usage_value(
    device: &HidDevice,
    kind: windows::Win32::Devices::HumanInterfaceDevice::HIDP_REPORT_TYPE,
    report: &[u8],
) -> Option<u8> {
    let mut value: u32 = 0;
    let status = HidP_GetUsageValue(
        kind,
        GENERIC_DEVICE_CONTROLS,
        0,
        BATTERY_STRENGTH,
        &mut value,
        device.preparsed,
        report,
    );
    if status.is_ok() && value <= 100 {
        Some(value as u8)
    } else {
        None
    }
}

fn kind_of(device: &HidDevice) -> DeviceKind {
    match (device.caps.UsagePage, device.caps.Usage) {
        (0x01, 0x06) => DeviceKind::Keyboard,
        (0x01, 0x02) => DeviceKind::Mouse,
        (0x01, 0x04) | (0x01, 0x05) | (0x01, 0x08) => DeviceKind::Gamepad,
        _ => DeviceKind::Other,
    }
}
