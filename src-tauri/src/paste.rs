use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::thread::sleep;
use std::time::Duration;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VK_CONTROL,
};

const CF_UNICODETEXT: u32 = 13;

pub struct PasteEngine;

impl PasteEngine {
    /// Copies text to clipboard, simulates Ctrl+V into the active focused app,
    /// waits 150ms, then restores original clipboard contents.
    pub fn paste(text: &str) -> Result<(), String> {
        let original_text = Self::get_clipboard_text().ok();

        if let Err(e) = Self::set_clipboard_text(text) {
            // Fall back to direct simulated typing if clipboard access fails
            Self::type_text_fallback(text);
            return Err(e);
        }

        // Simulate Ctrl + V
        Self::send_ctrl_v();

        // 150 ms delay to allow active target window (e.g. Word, Slack, VSCode) to read clipboard
        sleep(Duration::from_millis(150));

        // Restore original clipboard
        if let Some(orig) = original_text {
            let _ = Self::set_clipboard_text(&orig);
        }

        Ok(())
    }

    pub fn send_ctrl_v() {
        let v_vk: u16 = 0x56; // VK_V

        let mut inputs = [
            // Ctrl Down
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: 0,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // V Down
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: v_vk,
                        wScan: 0,
                        dwFlags: 0,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // V Up
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: v_vk,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Ctrl Up
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            );
        }
    }

    pub fn type_text_fallback(text: &str) {
        for c in text.encode_utf16() {
            let mut inputs = [
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: 0,
                            wScan: c,
                            dwFlags: KEYEVENTF_UNICODE,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: 0,
                            wScan: c,
                            dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                },
            ];
            unsafe {
                SendInput(2, inputs.as_mut_ptr(), std::mem::size_of::<INPUT>() as i32);
            }
        }
    }

    pub fn get_clipboard_text() -> Result<String, String> {
        unsafe {
            if OpenClipboard(0 as HWND) == 0 {
                return Err("Failed to open clipboard".to_string());
            }
            if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
                CloseClipboard();
                return Err("Clipboard does not contain unicode text".to_string());
            }
            let h_mem = GetClipboardData(CF_UNICODETEXT);
            if h_mem.is_null() {
                CloseClipboard();
                return Err("Failed to retrieve clipboard data".to_string());
            }
            let ptr = GlobalLock(h_mem) as *const u16;
            if ptr.is_null() {
                CloseClipboard();
                return Err("Failed to lock clipboard data".to_string());
            }

            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let slice = std::slice::from_raw_parts(ptr, len);
            let s = String::from_utf16_lossy(slice);

            GlobalUnlock(h_mem);
            CloseClipboard();
            Ok(s)
        }
    }

    pub fn set_clipboard_text(text: &str) -> Result<(), String> {
        let wide: Vec<u16> = OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let bytes = wide.len() * std::mem::size_of::<u16>();

        unsafe {
            // Retry loop for opening clipboard in case another process briefly holds it
            let mut opened = false;
            for _ in 0..5 {
                if OpenClipboard(0 as HWND) != 0 {
                    opened = true;
                    break;
                }
                sleep(Duration::from_millis(10));
            }
            if !opened {
                return Err("Unable to open clipboard for writing".to_string());
            }

            EmptyClipboard();

            let h_mem = GlobalAlloc(GMEM_MOVEABLE, bytes);
            if h_mem.is_null() {
                CloseClipboard();
                return Err("Failed to allocate global memory for clipboard".to_string());
            }

            let ptr = GlobalLock(h_mem) as *mut u16;
            if ptr.is_null() {
                CloseClipboard();
                return Err("Failed to lock global memory".to_string());
            }

            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
            GlobalUnlock(h_mem);

            if SetClipboardData(CF_UNICODETEXT, h_mem).is_null() {
                CloseClipboard();
                return Err("Failed to set clipboard data".to_string());
            }

            CloseClipboard();
            Ok(())
        }
    }
}
