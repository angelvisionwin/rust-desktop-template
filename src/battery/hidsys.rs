use std::ffi::c_void;
use std::mem::size_of;

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA,
    SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};
use windows::Win32::Devices::HumanInterfaceDevice::{
    HIDD_ATTRIBUTES, HIDP_CAPS, HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetPreparsedData,
    HidD_GetProductString, HidP_GetCaps, PHIDP_PREPARSED_DATA,
};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile,
    WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::{GUID, PCWSTR};

const GUID_DEVINTERFACE_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);

pub struct HidDevice {
    pub path: String,
    pub group: String,
    pub product: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub caps: HIDP_CAPS,
    pub preparsed: PHIDP_PREPARSED_DATA,
    pub handle: Option<HANDLE>,
}

impl Drop for HidDevice {
    fn drop(&mut self) {
        unsafe {
            if let Some(h) = self.handle.take() {
                let _ = CloseHandle(h);
            }
            if self.preparsed.0 != 0 {
                let _ = HidD_FreePreparsedData(self.preparsed);
            }
        }
    }
}

pub fn enumerate() -> Vec<HidDevice> {
    let mut out = Vec::new();
    unsafe {
        let Ok(set) = SetupDiGetClassDevsW(
            Some(&GUID_DEVINTERFACE_HID),
            None,
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        ) else {
            return out;
        };
        enumerate_inner(set, &mut out);
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    out
}

unsafe fn enumerate_inner(set: HDEVINFO, out: &mut Vec<HidDevice>) {
    let mut index = 0u32;
    loop {
        let mut iface = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        if SetupDiEnumDeviceInterfaces(set, None, &GUID_DEVINTERFACE_HID, index, &mut iface)
            .is_err()
        {
            break;
        }
        index += 1;

        let mut required = 0u32;
        let _ = SetupDiGetDeviceInterfaceDetailW(set, &iface, None, 0, Some(&mut required), None);
        if required == 0 {
            continue;
        }
        let mut storage = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
        let detail = storage.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        let mut info = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        if SetupDiGetDeviceInterfaceDetailW(
            set,
            &iface,
            Some(detail),
            required,
            Some(&mut required),
            Some(&mut info),
        )
        .is_err()
        {
            continue;
        }
        let ptr = (*detail).DevicePath.as_ptr();
        let len = (0..).take_while(|&i| *ptr.add(i) != 0).count();
        let mut path_wide = std::slice::from_raw_parts(ptr, len).to_vec();
        path_wide.push(0);
        let path = String::from_utf16_lossy(&path_wide[..len]);
        let group = match path.find("&col") {
            Some(pos) => path[..pos].to_string(),
            None => path.clone(),
        };

        let Ok(handle) = CreateFileW(
            PCWSTR(path_wide.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            None,
        ) else {
            continue;
        };

        let mut attrs = HIDD_ATTRIBUTES {
            Size: size_of::<HIDD_ATTRIBUTES>() as u32,
            ..Default::default()
        };
        let mut preparsed = PHIDP_PREPARSED_DATA(0);
        let mut caps = HIDP_CAPS::default();
        let _ = HidD_GetAttributes(handle, &mut attrs);
        if !HidD_GetPreparsedData(handle, &mut preparsed).as_bool() || preparsed.0 == 0 {
            let _ = CloseHandle(handle);
            continue;
        }
        if !HidP_GetCaps(preparsed, &mut caps).is_ok() {
            let _ = HidD_FreePreparsedData(preparsed);
            let _ = CloseHandle(handle);
            continue;
        }

        let mut name_buf = [0u16; 128];
        let mut product = String::new();
        if HidD_GetProductString(
            handle,
            name_buf.as_mut_ptr() as *mut c_void,
            (name_buf.len() * 2) as u32,
        )
        .as_bool()
        {
            product = crate::util::from_wide(&name_buf);
        }

        out.push(HidDevice {
            path,
            group,
            product,
            vendor_id: attrs.VendorID,
            product_id: attrs.ProductID,
            caps,
            preparsed,
            handle: Some(handle),
        });
    }
}

pub unsafe fn read_report(handle: HANDLE, timeout_ms: u32) -> Option<Vec<u8>> {
    let event = CreateEventW(None, true, false, None).ok()?;
    let mut buf = [0u8; 256];
    let mut overlapped = OVERLAPPED {
        hEvent: event,
        ..Default::default()
    };
    let mut read = 0u32;
    let _ = ReadFile(
        handle,
        Some(&mut buf[..]),
        Some(&mut read),
        Some(&mut overlapped),
    );
    let result = if WaitForSingleObject(event, timeout_ms) == WAIT_OBJECT_0 {
        let mut got = 0u32;
        let ok = GetOverlappedResult(handle, &overlapped, &mut got, false).is_ok();
        if ok {
            Some(buf[..got.min(buf.len() as u32) as usize].to_vec())
        } else {
            None
        }
    } else {
        let _ = CancelIoEx(handle, Some(&overlapped));
        let mut got = 0u32;
        let _ = GetOverlappedResult(handle, &overlapped, &mut got, true);
        None
    };
    let _ = CloseHandle(event);
    result
}

pub unsafe fn write_report(handle: HANDLE, data: &[u8]) -> bool {
    let Ok(event) = CreateEventW(None, true, false, None) else {
        return false;
    };
    let mut overlapped = OVERLAPPED {
        hEvent: event,
        ..Default::default()
    };
    let mut written = 0u32;
    let _ = WriteFile(
        handle,
        Some(data),
        Some(&mut written),
        Some(&mut overlapped),
    );
    let mut ok = false;
    if WaitForSingleObject(event, 1000) == WAIT_OBJECT_0 {
        let mut got = 0u32;
        ok = GetOverlappedResult(handle, &overlapped, &mut got, false).is_ok();
    } else {
        let _ = CancelIoEx(handle, Some(&overlapped));
        let mut got = 0u32;
        let _ = GetOverlappedResult(handle, &overlapped, &mut got, true);
    }
    let _ = CloseHandle(event);
    ok
}
