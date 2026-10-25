use std::os::windows::ffi::OsStrExt;
use std::ffi::OsStr;

const RUN_KEY_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_VALUE_NAME: &str = "FlowLocal";

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, REG_SZ,
};

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Registers or removes the HKCU Run entry so the packaged FlowLocal exe
/// starts automatically when the user logs in.
pub fn set_launch_at_login(enable: bool) -> Result<(), String> {
    // Only the packaged release build registers itself for autostart. A debug
    // cargo run build registered here would launch the dev binary on login.
    if cfg!(debug_assertions) {
        return Ok(());
    }

    let hkey = open_run_key()?;
    if !enable {
        let status = unsafe { RegDeleteValueW(hkey, to_wide(APP_VALUE_NAME).as_ptr()) };
        unsafe {
            RegCloseKey(hkey);
        }
        // 2 = ERROR_FILE_NOT_FOUND: the entry is already absent, which is fine.
        return if status == 0 || status == 2 {
            Ok(())
        } else {
            Err(format!("Failed to remove autostart entry (status {})", status))
        };
    }

    let exe = std::env::current_exe().map_err(|e| format!("Failed to determine exe path: {}", e))?;
    let command = format!("\"{}\"", exe.display());
    let command_wide = to_wide(&command);
    let bytes_len = command_wide.len() * std::mem::size_of::<u16>();

    let status = unsafe {
        RegSetValueExW(
            hkey,
            to_wide(APP_VALUE_NAME).as_ptr(),
            0,
            REG_SZ,
            command_wide.as_ptr() as *const u8,
            bytes_len as u32,
        )
    };
    unsafe {
        RegCloseKey(hkey);
    }
    if status != ERROR_SUCCESS {
        return Err(format!("Failed to write autostart entry (status {})", status));
    }
    Ok(())
}

/// True when the autostart entry currently exists.
pub fn is_launch_at_login_enabled() -> bool {
    match open_run_key() {
        Ok(hkey) => {
            let mut len: u32 = 0;
            let status = unsafe {
                RegQueryValueExW(
                    hkey,
                    to_wide(APP_VALUE_NAME).as_ptr(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut len,
                )
            };
            unsafe {
                RegCloseKey(hkey);
            }
            status == ERROR_SUCCESS
        }
        Err(_) => false,
    }
}

fn open_run_key() -> Result<HKEY, String> {
    let mut hkey: HKEY = std::ptr::null_mut();
    let status = unsafe {
        RegCreateKeyW(
            HKEY_CURRENT_USER,
            to_wide(RUN_KEY_PATH).as_ptr(),
            &mut hkey,
        )
    };
    if status != ERROR_SUCCESS || hkey.is_null() {
        return Err(format!("Failed to open registry run key (status {})", status));
    }
    Ok(hkey)
}
