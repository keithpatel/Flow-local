pub mod audio;
pub mod autostart;
pub mod cleanup;
pub mod hotkey;
pub mod llm;
pub mod paste;
pub mod storage;
pub mod stt;
pub mod vad;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};

use audio::{AudioHandle, AudioRecorder};
use cleanup::CleanupEngine;
use hotkey::{HotkeyEvent, HotkeyManager};
use llm::LlmPolishEngine;
use paste::PasteEngine;
use storage::{AppSettings, HistoryEntry, StorageManager};
use stt::WhisperEngine;
use vad::VadGate;

#[derive(Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub state: String,
    pub transcript: String,
    pub latency_ms: u64,
    pub stt_ms: u64,
    pub cleanup_ms: u64,
    pub rms: f32,
    pub paused: bool,
}

impl Default for StatusSnapshot {
    fn default() -> Self {
        Self {
            state: "idle".into(),
            transcript: String::new(),
            latency_ms: 0,
            stt_ms: 0,
            cleanup_ms: 0,
            rms: 0.0,
            paused: false,
        }
    }
}

#[derive(Clone, Serialize)]
pub struct AppInfo {
    pub model: String,
    pub hotkey: String,
    pub models: Vec<String>,
    pub engine_ready: bool,
    pub cpu: String,
    pub gpu: String,
    pub version: String,
    pub microphones: Vec<String>,
}

#[derive(Clone)]
pub struct AppState {
    tx: Sender<HotkeyEvent>,
    status: Arc<Mutex<StatusSnapshot>>,
    level: Arc<Mutex<f32>>,
    waveform: Arc<Mutex<std::collections::VecDeque<f32>>>,
    storage: Arc<Mutex<StorageManager>>,
    paused: Arc<AtomicBool>,
    app_root: PathBuf,
}

fn set_status(app: &AppHandle, state: &AppState, snapshot: StatusSnapshot) {
    {
        let mut current = state.status.lock().unwrap();
        *current = snapshot.clone();
    }
    let _ = app.emit("dictation-state", snapshot);
}

fn reset_to_idle_after(app: AppHandle, state: AppState) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1800));
        let snapshot = {
            let mut current = state.status.lock().unwrap();
            if current.state != "done" {
                return;
            }
            current.state = "idle".into();
            current.clone()
        };
        let _ = app.emit("dictation-state", snapshot);
    });
}

fn spawn_engine(
    app: AppHandle,
    state: AppState,
    audio: AudioHandle,
    rx: Receiver<HotkeyEvent>,
    initial_settings: AppSettings,
) {
    thread::spawn(move || {
        // Wake up UI Automation early so Chromium/Electron apps (VS Code,
        // browsers) have accessibility ready by the first dictation.
        thread::spawn(PasteEngine::warmup);
        let vad = VadGate::new(16000);

        let mut settings = initial_settings;
        let mut stt = WhisperEngine::new(&state.app_root, &settings.active_model);
        let dict = state.storage.lock().unwrap().get_dictionary();
        let prompt_bias = dict.keys().cloned().collect::<Vec<_>>().join(", ");
        stt.set_prompt_bias(&prompt_bias);

        let mut cleanup = CleanupEngine::new();
        cleanup.set_dictionary(dict);
        cleanup.set_snippets(state.storage.lock().unwrap().get_snippets());

        let mut llm = LlmPolishEngine::new(settings.llm.clone());
        let mut is_recording = false;

        while let Ok(event) = rx.recv() {
            match event {
                HotkeyEvent::StartPushToTalk => {
                    if state.paused.load(Ordering::SeqCst) || is_recording {
                        continue;
                    }
                    is_recording = true;
                    audio.start_recording();
                    set_status(
                        &app,
                        &state,
                        StatusSnapshot {
                            state: "listening".into(),
                            transcript: pushed_transcript(&state),
                            latency_ms: 0,
                            stt_ms: 0,
                            cleanup_ms: 0,
                            rms: 0.0,
                            paused: false,
                        },
                    );
                }

                HotkeyEvent::StopPushToTalk => {
                    if !is_recording {
                        continue;
                    }
                    is_recording = false;
                    set_status(
                        &app,
                        &state,
                        StatusSnapshot {
                            state: "processing".into(),
                            transcript: pushed_transcript(&state),
                            latency_ms: 0,
                            stt_ms: 0,
                            cleanup_ms: 0,
                            rms: 0.0,
                            paused: false,
                        },
                    );

                    let samples = audio.stop_recording();
                    let (has_speech, speech_samples) = vad.process(&samples);
                    if !has_speech {
                        set_status(
                            &app,
                            &state,
                            StatusSnapshot {
                                state: "idle".into(),
                                transcript: pushed_transcript(&state),
                                latency_ms: 0,
                                stt_ms: 0,
                                cleanup_ms: 0,
                                rms: 0.0,
                                paused: false,
                            },
                        );
                        continue;
                    }

                    let wav_bytes = match AudioRecorder::samples_to_wav(&speech_samples) {
                        Ok(b) => b,
                        Err(e) => {
                            set_status(
                                &app,
                                &state,
                                StatusSnapshot {
                                    state: "error".into(),
                                    transcript: format!("Audio error: {}", e),
                                    latency_ms: 0,
                                    stt_ms: 0,
                                    cleanup_ms: 0,
                                    rms: 0.0,
                                    paused: false,
                                },
                            );
                            continue;
                        }
                    };

                    let start_total = std::time::Instant::now();
                    match stt.transcribe_wav_bytes(&wav_bytes) {
                        Ok((raw_text, stt_ms)) => {
                            if raw_text.trim().is_empty() {
                                set_status(
                                    &app,
                                    &state,
                                    StatusSnapshot {
                                        state: "idle".into(),
                                        transcript: pushed_transcript(&state),
                                        latency_ms: 0,
                                        stt_ms: 0,
                                        cleanup_ms: 0,
                                        rms: 0.0,
                                        paused: false,
                                    },
                                );
                                continue;
                            }

                            let cleanup_start = std::time::Instant::now();
                            let clean_l1 = cleanup.clean(&raw_text);
                            let cleanup_ms = cleanup_start.elapsed().as_millis() as u64;

                            let polished = tauri::async_runtime::block_on(llm.polish(&clean_l1));
                            let total_ms = start_total.elapsed().as_millis() as u64;

                            let paste_result = PasteEngine::paste(&polished);
                            let (final_state, message) = match paste_result {
                                Ok(()) => ("done".to_string(), polished.clone()),
                                Err(e) => ("error".to_string(), format!("Paste failed: {}", e)),
                            };

                            set_status(
                                &app,
                                &state,
                                StatusSnapshot {
                                    state: final_state.clone(),
                                    transcript: message,
                                    latency_ms: total_ms,
                                    stt_ms: stt_ms as u64,
                                    cleanup_ms,
                                    rms: 0.0,
                                    paused: false,
                                },
                            );

                            let duration_ms = (samples.len() as f64 / 16.0) as u64;
                            let mode = format!("{:?}", settings.llm.active_mode);
                            state
                                .storage
                                .lock()
                                .unwrap()
                                .add_history_entry(&raw_text, &polished, duration_ms, total_ms as u128, &mode);

                            if final_state == "done" {
                                reset_to_idle_after(app.clone(), state.clone());
                            }
                        }
                        Err(e) => {
                            set_status(
                                &app,
                                &state,
                                StatusSnapshot {
                                    state: "error".into(),
                                    transcript: format!("Transcription failed: {}", e),
                                    latency_ms: start_total.elapsed().as_millis() as u64,
                                    stt_ms: 0,
                                    cleanup_ms: 0,
                                    rms: 0.0,
                                    paused: false,
                                },
                            );
                        }
                    }
                }

                HotkeyEvent::ToggleHandsFree => {
                    if state.paused.load(Ordering::SeqCst) {
                        continue;
                    }
                    if is_recording {
                        is_recording = false;
                        let samples = audio.stop_recording();
                        let (has_speech, speech_samples) = vad.process(&samples);
                        if has_speech {
                            if let Ok(wav) = AudioRecorder::samples_to_wav(&speech_samples) {
                                if let Ok((raw, _)) = stt.transcribe_wav_bytes(&wav) {
                                    let clean = cleanup.clean(&raw);
                                    let _ = PasteEngine::paste(&clean);
                                    let _ = state.storage.lock().unwrap().add_history_entry(
                                        &raw,
                                        &clean,
                                        (samples.len() as f64 / 16.0) as u64,
                                        0,
                                        &format!("{:?}", settings.llm.active_mode),
                                    );
                                }
                            }
                        }
                        set_status(
                            &app,
                            &state,
                            StatusSnapshot {
                                state: "idle".into(),
                                transcript: pushed_transcript(&state),
                                latency_ms: 0,
                                stt_ms: 0,
                                cleanup_ms: 0,
                                rms: 0.0,
                                paused: false,
                            },
                        );
                    } else {
                        is_recording = true;
                        audio.start_recording();
                        set_status(
                            &app,
                            &state,
                            StatusSnapshot {
                                state: "listening".into(),
                                transcript: pushed_transcript(&state),
                                latency_ms: 0,
                                stt_ms: 0,
                                cleanup_ms: 0,
                                rms: 0.0,
                                paused: false,
                            },
                        );
                    }
                }

                HotkeyEvent::Cancel => {
                    if is_recording {
                        is_recording = false;
                        audio.cancel_recording();
                    }
                    set_status(
                        &app,
                        &state,
                        StatusSnapshot {
                            state: "idle".into(),
                            transcript: pushed_transcript(&state),
                            latency_ms: 0,
                            stt_ms: 0,
                            cleanup_ms: 0,
                            rms: 0.0,
                            paused: false,
                        },
                    );
                }

                HotkeyEvent::ReloadSettings => {
                    settings = state.storage.lock().unwrap().get_settings();
                    stt.set_active_model(&settings.active_model);
                    let dict = state.storage.lock().unwrap().get_dictionary();
                    let bias = dict.keys().cloned().collect::<Vec<_>>().join(", ");
                    stt.set_prompt_bias(&bias);
                    cleanup.set_dictionary(dict);
                    cleanup.set_snippets(state.storage.lock().unwrap().get_snippets());
                    llm.set_config(settings.llm.clone());
                }
            }
        }
    });
}

fn pushed_transcript(state: &AppState) -> String {
    state.status.lock().map(|s| s.transcript.clone()).unwrap_or_default()
}

/// Root folder holding models/, bin/ and data/.
/// Debug builds keep using the repo checkout; the installed release app is
/// self-contained and uses the folder next to its own executable.
fn resolve_app_root() -> PathBuf {
    if cfg!(debug_assertions) {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .to_path_buf();
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// One-time migration for the installed app: copies models/, bin/ and data/
/// from the original checkout folder next to the installed exe, so the app
/// works standalone and the checkout folder can be deleted.
fn migrate_runtime_assets(app_root: &PathBuf) {
    if cfg!(debug_assertions) {
        return;
    }
    let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(&PathBuf::from("."))
        .to_path_buf();
    if src_root == *app_root {
        return;
    }

    let mut migrated: Vec<&str> = Vec::new();
    for sub in ["models", "bin", "data"] {
        let dst = app_root.join(sub);
        if fs::read_dir(&dst).map(|mut entries| entries.next().is_some()).unwrap_or(false) {
            continue; // Destination already populated.
        }

        // 1) Bundled resources shipped inside the installer, unpacked next to
        //    the exe (self-contained installs on any machine).
        let bundled = app_root.join("resources").join(sub);
        if bundled.is_dir() {
            if copy_dir_recursive(&bundled, &dst).is_ok() {
                let _ = fs::remove_dir_all(&bundled);
                migrated.push(sub);
                continue;
            }
        }

        // 2) The original build-time checkout (same-machine reinstall flow).
        let src = src_root.join(sub);
        if !src.is_dir() || src == dst {
            continue;
        }
        if copy_dir_recursive(&src, &dst).is_ok() {
            migrated.push(sub);
        }
    }
    for m in &migrated {
        eprintln!("[FlowLocal] migrated '{}' into install folder", m);
    }

    // Tidy up: drop the empty resources dir left after extraction.
    if let Ok(mut entries) = fs::read_dir(app_root.join("resources")) {
        if entries.next().is_none() {
            let _ = fs::remove_dir(app_root.join("resources"));
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("create_dir_all failed: {}", e))?;
    for entry in fs::read_dir(src).map_err(|e| format!("read_dir failed: {}", e))? {
        let entry = entry.map_err(|e| format!("entry failed: {}", e))?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            fs::copy(&path, &target).map_err(|e| format!("copy failed for {:?}: {}", path, e))?;
        }
    }
    Ok(())
}

#[tauri::command]
fn get_status(state: State<'_, AppState>) -> StatusSnapshot {
    let mut snapshot = state.status.lock().unwrap().clone();
    snapshot.rms = *state.level.lock().unwrap();
    snapshot.paused = state.paused.load(Ordering::SeqCst);
    if snapshot.paused && snapshot.state == "idle" {
        snapshot.state = "paused".into();
    }
    snapshot
}

#[tauri::command]
fn get_waveform(state: State<'_, AppState>) -> Vec<f32> {
    state
        .waveform
        .lock()
        .map(|bars| bars.iter().copied().collect())
        .unwrap_or_default()
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> AppSettings {
    eprintln!("[Info] get_settings called");
    state.storage.lock().unwrap().get_settings()
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: AppSettings) {
    let previous = state.storage.lock().unwrap().get_settings();
    state.storage.lock().unwrap().save_settings(&settings);
    if previous.launch_at_login != settings.launch_at_login {
        match autostart::set_launch_at_login(settings.launch_at_login) {
            Ok(()) => eprintln!(
                "[Autostart] launch at login {}",
                if settings.launch_at_login { "enabled" } else { "disabled" }
            ),
            Err(e) => eprintln!("[Autostart] error: {}", e),
        }
    }
    let _ = state.tx.send(HotkeyEvent::ReloadSettings);
}

#[tauri::command]
fn sync_launch_at_login(state: State<'_, AppState>) -> bool {
    let enabled = state.storage.lock().unwrap().get_settings().launch_at_login;
    if cfg!(not(debug_assertions)) && autostart::is_launch_at_login_enabled() != enabled {
        let _ = autostart::set_launch_at_login(enabled);
    }
    enabled
}

#[tauri::command]
fn get_history(state: State<'_, AppState>) -> Vec<HistoryEntry> {
    eprintln!("[Info] get_history called");
    state.storage.lock().unwrap().get_history()
}

#[tauri::command]
fn clear_history(state: State<'_, AppState>) {
    state.storage.lock().unwrap().clear_history();
}

#[tauri::command]
fn get_dictionary(state: State<'_, AppState>) -> HashMap<String, String> {
    state.storage.lock().unwrap().get_dictionary()
}

#[tauri::command]
fn save_dictionary(state: State<'_, AppState>, dictionary: HashMap<String, String>) {
    state.storage.lock().unwrap().save_dictionary(&dictionary);
    let _ = state.tx.send(HotkeyEvent::ReloadSettings);
}

#[tauri::command]
fn get_snippets(state: State<'_, AppState>) -> HashMap<String, String> {
    state.storage.lock().unwrap().get_snippets()
}

#[tauri::command]
fn save_snippets(state: State<'_, AppState>, snippets: HashMap<String, String>) {
    state.storage.lock().unwrap().save_snippets(&snippets);
    let _ = state.tx.send(HotkeyEvent::ReloadSettings);
}

#[tauri::command]
fn get_app_info(state: State<'_, AppState>) -> AppInfo {
    eprintln!("[Info] get_app_info: start");
    let settings = state.storage.lock().unwrap().get_settings();
    let mut models: Vec<String> = fs::read_dir(state.app_root.join("models"))
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter_map(|name| {
                    if name.starts_with("ggml-") && name.ends_with(".bin") {
                        Some(name.trim_start_matches("ggml-").trim_end_matches(".bin").to_string())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    models.sort();
    if models.is_empty() {
        models.push(settings.active_model.clone());
    }

    let engine_ready = state
        .app_root
        .join("bin")
        .join("whisper")
        .join("Release")
        .join("whisper-cli.exe")
        .exists()
        && state
            .app_root
            .join("models")
            .join(format!("ggml-{}.bin", settings.active_model))
            .exists();

    let microphones = {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(AudioRecorder::list_microphones());
        });
        match rx.recv_timeout(Duration::from_millis(900)) {
            Ok(list) => list,
            Err(_) => {
                eprintln!("[Info] microphone enumeration timed out");
                Vec::new()
            }
        }
    };

    eprintln!("[Info] get_app_info: done ({} mics)", microphones.len());
    AppInfo {
        model: settings.active_model.clone(),
        hotkey: settings.hotkey.clone(),
        models,
        engine_ready,
        cpu: "Intel Core i5-7200U · 2C/4T · AVX2/FMA".to_string(),
        gpu: "CPU (whisper.cpp AVX2) · no CUDA GPU detected".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        microphones,
    }
}

#[tauri::command]
fn set_paused(state: State<'_, AppState>, paused: bool) -> bool {
    state.paused.store(paused, Ordering::SeqCst);
    if paused {
        let _ = state.tx.send(HotkeyEvent::Cancel);
    }
    paused
}

#[tauri::command]
fn ptt_start(state: State<'_, AppState>) {
    let _ = state.tx.send(HotkeyEvent::StartPushToTalk);
}

#[tauri::command]
fn ptt_stop(state: State<'_, AppState>) {
    let _ = state.tx.send(HotkeyEvent::StopPushToTalk);
}

#[tauri::command]
fn hands_free_toggle(state: State<'_, AppState>) {
    let _ = state.tx.send(HotkeyEvent::ToggleHandsFree);
}

#[tauri::command]
fn cancel_dictation(state: State<'_, AppState>) {
    let _ = state.tx.send(HotkeyEvent::Cancel);
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    PasteEngine::set_clipboard_text(&text)
}

#[tauri::command]
fn test_paste(text: String, delay_ms: u64) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(delay_ms));
        let _ = PasteEngine::paste(&text);
    });
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn show_window(app: AppHandle) {
    show_main(&app);
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn setup_overlay(app: &tauri::App) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.set_ignore_cursor_events(true);

        if let Ok(Some(monitor)) = window.current_monitor() {
            let monitor_size = monitor.size();
            let monitor_pos = monitor.position();
            if let Ok(window_size) = window.outer_size() {
                let x = monitor_pos.x + (monitor_size.width as i32 - window_size.width as i32) / 2;
                let y = monitor_pos.y + monitor_size.height as i32 - window_size.height as i32 - 60;
                let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
            }
        }

        #[cfg(target_os = "windows")]
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetWindowLongW, SetWindowLongW, ShowWindow, GWL_EXSTYLE, SW_SHOWNOACTIVATE,
                WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            };
            if let Ok(hwnd) = window.hwnd() {
                let h = hwnd.0 as isize;
                let ex_style = GetWindowLongW(h as _, GWL_EXSTYLE);
                SetWindowLongW(
                    h as _,
                    GWL_EXSTYLE,
                    ex_style | WS_EX_NOACTIVATE as i32 | WS_EX_TOOLWINDOW as i32,
                );
                ShowWindow(h as _, SW_SHOWNOACTIVATE);
            }
        }
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Open FlowLocal", true, None::<&str>)?;
    let pause_item = MenuItem::with_id(app, "pause", "Pause / resume dictation", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &pause_item, &quit_item])?;

    let mut builder = TrayIconBuilder::with_id("flowlocal-tray")
        .tooltip("FlowLocal")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "pause" => {
                let state = app.state::<AppState>();
                let now_paused = !state.paused.load(Ordering::SeqCst);
                state.paused.store(now_paused, Ordering::SeqCst);
                if now_paused {
                    let _ = state.tx.send(HotkeyEvent::Cancel);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin: a second launch should hand control to the
        // already-running instance instead of installing a second keyboard
        // hook (which would paste every dictation twice).
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }))
        .setup(|app| {
            eprintln!("[FlowLocal] setup: begin");
        let app_root = resolve_app_root();
        migrate_runtime_assets(&app_root);

            let storage = Arc::new(Mutex::new(StorageManager::new(app_root.join("data"))));
            let settings = storage.lock().unwrap().get_settings();
            eprintln!("[FlowLocal] setup: storage ready");

            // Ensure the autostart entry matches the saved setting (repairs
            // externally deleted entries, and fires on first launch when the
            // setting was prerolled true).
            if settings.launch_at_login {
                if let Err(e) = autostart::set_launch_at_login(true) {
                    eprintln!("[Autostart] error: {}", e);
                }
            }

            let mut recorder = AudioRecorder::new();
            recorder.set_microphone(settings.microphone.clone());
            let level = recorder.level_handle();
            let waveform = recorder.waveform_handle();
            let audio_handle = recorder.handle();

            let (tx, rx) = crossbeam_channel::unbounded::<HotkeyEvent>();

            // Manage state before any slow work: the webview may start calling
            // commands while setup is still running.
            let state = AppState {
                tx: tx.clone(),
                status: Arc::new(Mutex::new(StatusSnapshot::default())),
                level,
                waveform,
                storage: storage.clone(),
                paused: Arc::new(AtomicBool::new(false)),
                app_root: app_root.clone(),
            };
            app.manage(state.clone());
            eprintln!("[FlowLocal] setup: state managed");

            if let Err(e) = recorder.start_stream() {
                eprintln!("[FlowLocal] Audio stream start error: {}", e);
            }
            eprintln!("[FlowLocal] setup: audio ready");
            // The cpal stream is not Send; keep it alive on the UI thread for the app lifetime.
            let _stream_owner: &'static mut AudioRecorder = Box::leak(Box::new(recorder));

            let hotkey_manager = HotkeyManager::new();
            if let Err(e) = hotkey_manager.start(tx.clone()) {
                eprintln!("[FlowLocal] Failed to install global keyboard hook: {}", e);
            }

            eprintln!("[FlowLocal] setup: hotkey installed, spawning engine");
            spawn_engine(app.handle().clone(), state.clone(), audio_handle, rx, settings);

            eprintln!(
                "[FlowLocal] windows -> main: {}, overlay: {}",
                app.get_webview_window("main").is_some(),
                app.get_webview_window("overlay").is_some()
            );
            setup_overlay(app);
            eprintln!("[FlowLocal] setup: overlay ready");
            setup_tray(app)?;
            eprintln!("[FlowLocal] setup: tray ready");

            Ok(())
        })
        .on_window_event(|window, event| {
            eprintln!("[FlowLocal] window event [{}]: {:?}", window.label(), event);
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_waveform,
            get_settings,
            save_settings,
            sync_launch_at_login,
            get_history,
            clear_history,
            get_dictionary,
            save_dictionary,
            get_snippets,
            save_snippets,
            get_app_info,
            set_paused,
            ptt_start,
            ptt_stop,
            hands_free_toggle,
            cancel_dictation,
            copy_text,
            test_paste,
            quit_app,
            show_window
        ])
        .build(tauri::generate_context!())
        .expect("error while building FlowLocal")
        .run(|_app, event| {
            eprintln!("[FlowLocal] run event: {:?}", event);
        });
}
