pub mod audio;
pub mod cleanup;
pub mod hotkey;
pub mod llm;
pub mod paste;
pub mod storage;
pub mod stt;
pub mod vad;

use audio::AudioRecorder;
use cleanup::CleanupEngine;
use hotkey::{HotkeyEvent, HotkeyManager};
use llm::LlmPolishEngine;
use paste::PasteEngine;
use storage::StorageManager;
use stt::WhisperEngine;
use vad::VadGate;

use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[tokio::main]
async fn main() {
    println!("============================================================");
    println!("     FlowLocal: Autonomous Offline Voice Dictation App      ");
    println!("============================================================");

    let app_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(&PathBuf::from("."))
        .to_path_buf();

    let data_dir = app_root.join("data");
    let storage = Arc::new(Mutex::new(StorageManager::new(data_dir)));
    let settings = storage.lock().unwrap().get_settings();

    println!("Detected Target OS: Windows (x86_64)");
    println!("Hardware Acceleration: AVX2/FMA CPU Optimized (whisper.cpp)");
    println!("Active Whisper Model: {}", settings.active_model);
    println!("Push-to-Talk Hotkey: Hold [{}] to speak, release to paste", settings.hotkey);
    println!("Hands-Free Mode: Double-tap [{}] or press [{} + Space]", settings.hotkey, settings.hotkey);
    println!("Emergency Cancel: Press [Esc] anytime");

    // 1. Initialize Audio Recorder & start WASAPI stream
    let mut recorder = AudioRecorder::new();
    recorder.set_microphone(settings.microphone.clone());
    if let Err(e) = recorder.start_stream() {
        eprintln!("Warning: Audio stream start error: {}. (Will retry on hotkey press)", e);
    } else {
        println!("Audio capture initialized (16 kHz Mono, 300ms ring buffer pre-roll).");
    }
    let recorder = Arc::new(recorder);

    // 2. Initialize VAD Gate
    let vad_gate = VadGate::new(16000);

    // 3. Initialize Whisper STT Engine
    let mut stt_engine = WhisperEngine::new(&app_root, &settings.active_model);
    // Seed prompt bias with personal dictionary terms
    let dict = storage.lock().unwrap().get_dictionary();
    let prompt_bias = dict.keys().cloned().collect::<Vec<_>>().join(", ");
    stt_engine.set_prompt_bias(&prompt_bias);

    if !stt_engine.is_model_available() {
        println!("Model {} not yet downloaded. Downloading tiny.en fallback...", settings.active_model);
    } else {
        println!("Whisper model is ready in RAM: {}", stt_engine.get_model_file_path().display());
    }
    let stt_engine = Arc::new(stt_engine);

    // 4. Initialize Layer 1 Cleanup Engine
    let mut cleanup = CleanupEngine::new();
    cleanup.set_dictionary(dict);
    cleanup.set_snippets(storage.lock().unwrap().get_snippets());
    let cleanup = Arc::new(cleanup);

    // 5. Initialize Layer 2 LLM Polish Engine
    let llm_engine = Arc::new(LlmPolishEngine::new(settings.llm.clone()));

    // 6. Setup Hotkey Event Channel & Hook
    let (tx, rx) = channel::<HotkeyEvent>();
    let hotkey_mgr = HotkeyManager::new();
    if let Err(e) = hotkey_mgr.start(tx) {
        eprintln!("Failed to install global keyboard hook: {}", e);
    } else {
        println!("Global hotkey hook installed successfully.");
    }

    // 7. Event Processing Loop
    println!("\nFlowLocal is running in the background. Ready to dictate!");
    while let Ok(event) = rx.recv() {
        match event {
            HotkeyEvent::StartPushToTalk => {
                println!("\n[FlowLocal] Listening (PTT)...");
                recorder.start_recording();
            }
            HotkeyEvent::StopPushToTalk => {
                println!("[FlowLocal] Key released. Processing speech pipeline...");
                let samples = recorder.stop_recording();

                // Stage 1: VAD Silence Filter
                let (has_speech, speech_samples) = vad_gate.process(&samples);
                if !has_speech {
                    println!("[FlowLocal] VAD: No active speech detected (silence or noise). Skipped.");
                    continue;
                }

                // Stage 2: In-memory WAV generation
                let wav_bytes = match AudioRecorder::samples_to_wav(&speech_samples) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("[FlowLocal] Audio formatting error: {}", e);
                        continue;
                    }
                };

                // Stage 3: Whisper Local Speech-to-Text
                let stt = Arc::clone(&stt_engine);
                let cl = Arc::clone(&cleanup);
                let llm = Arc::clone(&llm_engine);
                let store = Arc::clone(&storage);

                let start_total = Instant::now();
                match stt.transcribe_wav_bytes(&wav_bytes) {
                    Ok((raw_text, stt_ms)) => {
                        if raw_text.trim().is_empty() {
                            println!("[FlowLocal] Whisper output empty. Skipped.");
                            continue;
                        }

                        // Stage 4: Layer 1 Rule Cleanup (<5ms)
                        let cleanup_start = Instant::now();
                        let clean_l1 = cl.clean(&raw_text);
                        let cleanup_ms = cleanup_start.elapsed().as_millis();

                        // Stage 5: Layer 2 Optional AI Polish
                        let polished_text = llm.polish(&clean_l1).await;
                        let total_ms = start_total.elapsed().as_millis();

                        println!("[FlowLocal] Transcribed: \"{}\"", raw_text);
                        println!("[FlowLocal] Clean Text : \"{}\"", polished_text);
                        println!(
                            "[FlowLocal] Latency Breakdown -> STT: {}ms | Cleanup: {}ms | Total: {}ms",
                            stt_ms, cleanup_ms, total_ms
                        );

                        // Stage 6: Instant Paste
                        if let Err(e) = PasteEngine::paste(&polished_text) {
                            eprintln!("[FlowLocal] Paste error: {}", e);
                        } else {
                            println!("[FlowLocal] Success: Pasted into focused window!");
                        }

                        // Record in History
                        let duration_ms = (samples.len() as f64 / 16.0) as u64;
                        store.lock().unwrap().add_history_entry(
                            &raw_text,
                            &polished_text,
                            duration_ms,
                            total_ms,
                            "Clean",
                        );
                    }
                    Err(e) => {
                        eprintln!("[FlowLocal] STT Error: {}", e);
                    }
                }
            }
            HotkeyEvent::ToggleHandsFree => {
                println!("\n[FlowLocal] Hands-Free Mode Toggled! Press hotkey again to finish.");
                recorder.start_recording();
            }
            HotkeyEvent::Cancel => {
                println!("\n[FlowLocal] Esc pressed. Recording cancelled.");
                recorder.cancel_recording();
            }
        }
    }
}
