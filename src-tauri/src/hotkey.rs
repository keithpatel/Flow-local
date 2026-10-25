use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

// Virtual key codes
const VK_RCONTROL: u32 = 0xA3;
const VK_SPACE: u32 = 0x20;
const VK_ESCAPE: u32 = 0x1B;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    StartPushToTalk,
    StopPushToTalk,
    ToggleHandsFree,
    Cancel,
}

static mut HOOK_HANDLE: HHOOK = std::ptr::null_mut();
static mut EVENT_SENDER: Option<Sender<HotkeyEvent>> = None;
static mut IS_RCTRL_DOWN: bool = false;
static mut LAST_RCTRL_RELEASE: Option<Instant> = None;
static mut IS_HANDS_FREE_ACTIVE: bool = false;

pub struct HotkeyManager {
    is_running: Arc<AtomicBool>,
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&self, tx: Sender<HotkeyEvent>) -> Result<(), String> {
        self.is_running.store(true, Ordering::SeqCst);
        let running_clone = Arc::clone(&self.is_running);

        thread::spawn(move || {
            unsafe {
                EVENT_SENDER = Some(tx);
                let hook = SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(Self::low_level_keyboard_proc),
                    0 as HINSTANCE,
                    0,
                );

                if hook.is_null() {
                    eprintln!("Failed to install low-level keyboard hook");
                    return;
                }
                HOOK_HANDLE = hook;

                let mut msg: MSG = std::mem::zeroed();
                while running_clone.load(Ordering::SeqCst) && GetMessageW(&mut msg, 0 as _, 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                if !HOOK_HANDLE.is_null() {
                    UnhookWindowsHookEx(HOOK_HANDLE);
                    HOOK_HANDLE = std::ptr::null_mut();
                }
            }
        });

        Ok(())
    }

    unsafe extern "system" fn low_level_keyboard_proc(
        n_code: i32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        if n_code >= 0 {
            let kbd = *(l_param as *const KBDLLHOOKSTRUCT);
            let vk = kbd.vkCode;

            let is_key_down = w_param == WM_KEYDOWN as WPARAM || w_param == WM_SYSKEYDOWN as WPARAM;
            let is_key_up = w_param == WM_KEYUP as WPARAM || w_param == WM_SYSKEYUP as WPARAM;

            if vk == VK_ESCAPE && is_key_down {
                if let Some(ref sender) = EVENT_SENDER {
                    IS_HANDS_FREE_ACTIVE = false;
                    let _ = sender.send(HotkeyEvent::Cancel);
                }
            } else if vk == VK_RCONTROL {
                if is_key_down {
                    if !IS_RCTRL_DOWN {
                        IS_RCTRL_DOWN = true;

                        // Check for double-tap within 350ms
                        let is_double_tap = if let Some(last_time) = LAST_RCTRL_RELEASE {
                            last_time.elapsed() < Duration::from_millis(350)
                        } else {
                            false
                        };

                        if let Some(ref sender) = EVENT_SENDER {
                            if is_double_tap {
                                IS_HANDS_FREE_ACTIVE = !IS_HANDS_FREE_ACTIVE;
                                let _ = sender.send(HotkeyEvent::ToggleHandsFree);
                            } else if !IS_HANDS_FREE_ACTIVE {
                                let _ = sender.send(HotkeyEvent::StartPushToTalk);
                            } else {
                                // In hands-free mode, pressing RCtrl stops it
                                IS_HANDS_FREE_ACTIVE = false;
                                let _ = sender.send(HotkeyEvent::StopPushToTalk);
                            }
                        }
                    }
                } else if is_key_up {
                    if IS_RCTRL_DOWN {
                        IS_RCTRL_DOWN = false;
                        LAST_RCTRL_RELEASE = Some(Instant::now());

                        if !IS_HANDS_FREE_ACTIVE {
                            if let Some(ref sender) = EVENT_SENDER {
                                let _ = sender.send(HotkeyEvent::StopPushToTalk);
                            }
                        }
                    }
                }
            } else if vk == VK_SPACE && is_key_down && IS_RCTRL_DOWN {
                // Hotkey combo: Right Ctrl + Space triggers Hands-Free
                if let Some(ref sender) = EVENT_SENDER {
                    IS_HANDS_FREE_ACTIVE = !IS_HANDS_FREE_ACTIVE;
                    let _ = sender.send(HotkeyEvent::ToggleHandsFree);
                }
            }
        }

        CallNextHookEx(HOOK_HANDLE, n_code, w_param, l_param)
    }
}
