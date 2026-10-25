use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationTextPattern, IUIAutomationValuePattern,
};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VK_CONTROL, VK_INSERT, VK_SHIFT, VK_V,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};

const CF_UNICODETEXT: u32 = 13;

fn chrono_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("ts={}", secs)
}

/// Window classes whose paste shortcut differs from the Ctrl+V default.
const TERMINAL_CLASSES: &[(&str, PasteMode)] = &[
    // Windows Terminal
    ("CASCADIA_HOSTING_WINDOW_CLASS", PasteMode::CtrlShiftV),
    // Classic conhost (Windows 10 1809+)
    ("ConsoleWindowClass", PasteMode::CtrlV),
    // mintty (Git Bash)
    ("mintty", PasteMode::ShiftInsert),
    // ConEmu
    ("VirtualConsoleClass", PasteMode::ShiftInsert),
    // Alacritty
    ("Alacritty", PasteMode::CtrlShiftV),
];

pub struct PasteEngine;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PasteMode {
    CtrlV,
    CtrlShiftV,
    ShiftInsert,
}

/// Whether a paste attempt verifiably produced a change.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Landed {
    Yes,
    No,
    Unknown,
}

thread_local! {
    static UIA_CLIENT: Option<IUIAutomation> = {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()
        }
    };
}

impl PasteEngine {
    /// Early-connects UI Automation so Chromium/Electron apps (VS Code, browsers)
    /// have accessibility enabled by the time the first real dictation arrives.
    pub fn warmup() {
        let _ = Self::focus_text_snapshot();
    }

    /// Copies text to clipboard, pastes into the focused app, then restores the
    /// original clipboard contents.
    ///
    /// Each attempt is verified with UI Automation: if the focused element did
    /// not change, the next shortcut in the chain is tried, and as a last resort
    /// the text is typed via synthesized unicode keystrokes, which works in
    /// every app including terminals that ignore clipboard paste.
    pub fn paste(text: &str) -> Result<(), String> {
        if text.trim().is_empty() {
            return Ok(());
        }

        let original_text = Self::get_clipboard_text().ok();

        if let Err(e) = Self::set_clipboard_text(text) {
            // Fall back to direct simulated typing if clipboard access fails
            Self::type_text_fallback(text);
            return Err(e);
        }

        // Give the clipboard a moment to settle before injecting the shortcut.
        sleep(Duration::from_millis(80));

        let class = Self::foreground_class();
        let chain = Self::attempt_chain(&class);

        let mut verified = false;
        let mut assumed = false;
        for mode in &chain {
            let pre = Self::focus_text_snapshot();
            let sent = Self::send_paste(*mode);
            sleep(Duration::from_millis(300));
            let post = Self::focus_text_snapshot();
            let mut verdict = Self::verdict(pre.as_deref(), post.as_deref(), text);
            if verdict == Landed::No {
                // Chromium-based targets update their accessibility text
                // asynchronously; give it one more beat before concluding the
                // paste failed, otherwise a first attempt that actually worked
                // would be retried and pasted twice.
                sleep(Duration::from_millis(280));
                let post2 = Self::focus_text_snapshot();
                verdict = Self::verdict(pre.as_deref(), post2.as_deref(), text);
            }
            Self::log(&format!(
                "class='{}' mode={:?} sent={} landed={:?}",
                class, mode, sent, verdict
            ));
            match verdict {
                Landed::Yes => {
                    verified = true;
                    break;
                }
                Landed::Unknown => {
                    // Verification is unavailable (app not exposing UIA):
                    // stop injecting to avoid double pastes.
                    assumed = true;
                    break;
                }
                Landed::No => continue,
            }
        }

        if !verified && !assumed {
            Self::log("all shortcuts failed, falling back to typing");
            Self::type_text_fallback(text);
        }

        // Allow heavier targets (VSCode, terminals, Slack) time to read the clipboard
        // before the original contents are restored.
        sleep(Duration::from_millis(1200));

        // Restore original clipboard
        if let Some(orig) = original_text {
            let _ = Self::set_clipboard_text(&orig);
        }

        Ok(())
    }

    /// Shortcut chain for the given window class: class-specific combo first,
    /// then the universally supported Shift+Insert and Ctrl+Shift+V.
    fn attempt_chain(class: &str) -> Vec<PasteMode> {        let mut chain = Vec::new();
        let first = TERMINAL_CLASSES
            .iter()
            .find(|(c, _)| *c == class)
            .map(|(_, m)| *m)
            .unwrap_or(PasteMode::CtrlV);
        chain.push(first);
        if first != PasteMode::ShiftInsert {
            chain.push(PasteMode::ShiftInsert);
        }
        if first != PasteMode::CtrlShiftV {
            chain.push(PasteMode::CtrlShiftV);
        }
        chain
    }

    /// Decides whether a paste attempt succeeded, based on UIA readouts taken
    /// before and after injecting the shortcut.
    ///
    /// Deliberately conservative: retrying a shortcut that in fact landed is
    /// worse than skipping the retry, so a retry only happens when there is
    /// strong evidence the first attempt did nothing.
    fn verdict(pre: Option<&str>, post: Option<&str>, text: &str) -> Landed {
        let (Some(p), Some(q)) = (pre, post) else {
            return Landed::Unknown; // UIA readout unavailable: assume success
        };
        if p != q {
            return Landed::Yes; // focus text changed: paste landed
        }
        // Same content, but maybe the paste landed before 'pre' was taken
        // (or replaced identical-looking text): the pasted tail guarantees it.
        let tail = Self::text_tail(text);
        if let Some(tail) = tail {
            if q.contains(&tail) {
                return Landed::Yes;
            }
        }
        // Identical and tail absent. A tiny readout (element name/label, e.g.
        // "Terminal") is not meaningful evidence of content, so treat weak
        // readouts as inconclusive rather than failed.
        if p.chars().count() < 120 {
            return Landed::Unknown;
        }
        Landed::No
    }

    /// Last few meaningful characters of the dictated text, for containment
    /// checks. None when the text is too short to check reliably.
    fn text_tail(text: &str) -> Option<String> {
        let trimmed: Vec<char> = text.chars().collect();
        if trimmed.len() < 8 {
            return None;
        }
        let start = trimmed.len().saturating_sub(24);
        let tail: String = trimmed[start..].iter().collect();
        let tail = tail.trim_start().to_string();
        if tail.chars().count() < 4 {
            None
        } else {
            Some(tail)
        }
    }

    /// Writes paste decisions to `data/paste.log` next to the installed exe
    /// (best effort; dev builds only log to stderr).
    fn log(message: &str) {
        eprintln!("[Paste] {}", message);
        if cfg!(debug_assertions) {
            return;
        }
        let path = std::env::current_exe().ok().and_then(|p| {
            p.parent()
                .map(|d| d.join("data").join("paste.log"))
        });
        if let Some(path) = path {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let line = format!("{} [Paste] {}\n", chrono_now(), message);
                let _ = f.write_all(line.as_bytes());
            }
        }
    }

    /// Reads the accessible text of the focused element via UI Automation.
    /// Returns None when no meaningful readout is possible.
    fn focus_text_snapshot() -> Option<String> {
        UIA_CLIENT.with(|cell| {
            let uia = cell.as_ref()?;
            unsafe {
                let element = uia.GetFocusedElement().ok()?;
                if let Ok(value) = element.cast::<IUIAutomationValuePattern>() {
                    if let Ok(text) = value.CurrentValue() {
                        let s = text.to_string();
                        if !s.is_empty() {
                            return Some(s);
                        }
                    }
                }
                if let Ok(pattern) = element.cast::<IUIAutomationTextPattern>() {
                    if let Ok(range) = pattern.DocumentRange() {
                        if let Ok(text) = range.GetText(-1) {
                            let s = text.to_string();
                            if !s.is_empty() {
                                return Some(s);
                            }
                        }
                    }
                }
                if let Ok(name) = element.CurrentName() {
                    let s = name.to_string();
                    if !s.is_empty() {
                        return Some(s);
                    }
                }
                None
            }
        })
    }

    fn foreground_class() -> String {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return String::new();
            }
            let mut buf = [0u16; 256];
            let len = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            if len <= 0 {
                return String::new();
            }
            String::from_utf16_lossy(&buf[..len as usize])
        }
    }

    fn key_input(vk: u16, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Returns the number of injected events.
    pub fn send_paste(mode: PasteMode) -> u32 {
        let keys: &[(u16, bool)] = match mode {
            PasteMode::CtrlV => &[
                (VK_CONTROL, false),
                (VK_V, false),
                (VK_V, true),
                (VK_CONTROL, true),
            ],
            PasteMode::CtrlShiftV => &[
                (VK_CONTROL, false),
                (VK_SHIFT, false),
                (VK_V, false),
                (VK_V, true),
                (VK_SHIFT, true),
                (VK_CONTROL, true),
            ],
            PasteMode::ShiftInsert => &[
                (VK_SHIFT, false),
                (VK_INSERT, false),
                (VK_INSERT, true),
                (VK_SHIFT, true),
            ],
        };
        let mut inputs: Vec<INPUT> =
            keys.iter().map(|&(vk, up)| Self::key_input(vk, up)).collect();

        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        }
    }

    pub fn type_text_fallback(text: &str) {
        // A bare LF is unreliable as a typed unicode char; use CR (Enter).
        let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
        let chars: Vec<u16> = normalized.encode_utf16().collect();
        let mut batch: Vec<INPUT> = Vec::with_capacity(64 * 2);
        for &c in &chars {
            batch.push(Self::unicode_input(c, false));
            batch.push(Self::unicode_input(c, true));
            if batch.len() >= 64 * 2 {
                Self::send_inputs(batch.drain(..));
            }
        }
        Self::send_inputs(batch.drain(..));
    }

    fn send_inputs(inputs: impl ExactSizeIterator<Item = INPUT>) {
        if inputs.len() == 0 {
            return;
        }
        let mut inputs: Vec<INPUT> = inputs.collect();
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            );
        }
    }

    fn unicode_input(c: u16, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: c,
                    dwFlags: if up {
                        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                    } else {
                        KEYEVENTF_UNICODE
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
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
