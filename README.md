# FlowLocal: Offline-First Desktop Voice Dictation

> **Status for Collaborators & Agents**: This document details what has been implemented, what dependencies are installed on this laptop, how the system is architected, and the current state of execution.

---

## 1. Project Overview

**FlowLocal** is an offline-first, privacy-preserving desktop voice dictation app that replicates the core UX of **Wispr Flow**:
1. Place cursor in **any** text field (Notepad, VS Code, Slack, Word, Browser, etc.).
2. **Hold Right Ctrl** (or push-to-talk hotkey) and speak.
3. **Release Right Ctrl**: The audio is processed through local Voice Activity Detection (VAD), transcribed by local Whisper STT in memory, cleaned through a deterministic Layer 1 cleanup engine (<5ms), and immediately pasted into the focused window via clipboard simulation.
4. **100% Local by Default**: Audio is processed exclusively in RAM (never saved to disk). Zero network calls, zero accounts, zero telemetry.

---

## 2. Host Machine Profile & What Has Been Configured

* **Operating System**: Windows 10 x64 (`10.0.19045`)
* **Hardware Detected**:
  * **CPU**: Intel Core i5-7200U @ 2.50GHz (2 physical cores, 4 threads, AVX2, FMA, SSE4.2 supported)
  * **GPU**: Intel HD Graphics 620 (integrated, no CUDA)
  * **RAM**: 8.0 GB
  * **Storage Environment**: Workspace resides inside a OneDrive-synced folder (`c:\Users\keith\OneDrive\Desktop\speech`).

### Toolchains & Paths Configured
1. **Node.js**: `v22.20.0`, `npm 10.9.3`, `pnpm 10.13.1`
2. **Python**: `3.12.1`
3. **Rust Toolchain**: Rust `1.99.1` (`x86_64-pc-windows-gnu`) installed via `rustup-init.exe`.
4. **OneDrive File Lock Prevention**:
   * Cargo build artifacts in OneDrive folders can trigger Windows file-sharing locks (`os error 32`).
   * Configured `CARGO_TARGET_DIR=C:\Users\keith\AppData\Local\flowlocal_target` to isolate intermediate build artifacts outside of OneDrive.
5. **PowerShell Wrapper**:
   * Created `powershell.cmd` in workspace root to automatically inject Cargo into `%PATH%` and set `CARGO_TARGET_DIR`.

---

## 3. Directory Structure & Key Files

```text
c:\Users\keith\OneDrive\Desktop\speech\
├── bin/
│   └── whisper/Release/       # Precompiled AVX2-optimized Whisper binaries
│       ├── whisper-cli.exe    # Local STT CLI runner
│       ├── whisper-server.exe # Persistent in-memory HTTP STT daemon
│       ├── whisper-bench.exe  # Hardware benchmark tool
│       └── *.dll              # AVX2/Haswell CPU backend libraries
├── models/
│   ├── ggml-tiny.en.bin       # Whisper tiny.en (75 MB, ~300ms latency)
│   └── ggml-base.en.bin       # Whisper base.en (142 MB, recommended balance)
├── docs/
│   ├── behavior-spec.md       # Reverse-engineered Wispr Flow spec (REA)
│   ├── decisions.md           # Architecture and technical decision record
│   └── benchmarks.md          # Hardware benchmarks & latency breakdown
├── scripts/
│   ├── setup_whisper_engine.py # Automates Whisper engine & model downloads
│   └── benchmark.py           # Benchmark script testing audio to paste
├── src-tauri/
│   ├── Cargo.toml             # Rust dependencies (cpal, hound, windows-sys, tokio, etc.)
│   └── src/
│       ├── main.rs            # Core orchestrator pipeline
│       ├── audio.rs           # CPAL capture, 300ms ring buffer pre-roll, in-memory WAV
│       ├── vad.rs             # Energy & zero-crossing Voice Activity Detection
│       ├── stt.rs             # Local Whisper integration (greedy decoding, prompt bias)
│       ├── cleanup.rs         # Layer 1 deterministic cleanup (<5ms)
│       ├── llm.rs             # Layer 2 optional AI polish (Ollama, OpenAI, Claude)
│       ├── paste.rs           # Clipboard snapshot, Ctrl+V injection, restore & typing fallback
│       ├── hotkey.rs          # Windows low-level keyboard hook (PTT, double-tap, Esc)
│       └── storage.rs         # Persistent JSON storage for settings, dictionary, history
├── powershell.cmd             # Local runner wrapper with toolchain PATH
└── README.md                  # This state and documentation file
```

---

## 4. Pipeline Execution Flow

```
[User Holds Right Ctrl]
         │
         ▼
[Audio Stream: audio.rs]
 • Pulls 300ms pre-roll circular buffer from RAM (first word never clipped)
 • Records 16 kHz Mono PCM in RAM
         │
[User Releases Right Ctrl]
         │
         ▼
[VAD Gate: vad.rs]
 • Checks RMS energy and speech duration (>250ms active speech)
 • Trims silence padding -> Skips empty noise recordings
         │
         ▼
[Local Whisper STT: stt.rs]
 • Runs greedy decoding (beam size 1), Flash Attention, 4 threads
 • Injects personal dictionary bias via --prompt
         │
         ▼
[Layer 1 Text Cleanup: cleanup.rs] (<5ms)
 • Strips fillers ("um", "uh", "you know", "like")
 • Resolves stutters ("the the" -> "the")
 • Evaluates self-corrections ("meet at 3, no, 4 pm" -> "meet at 4 pm")
 • Applies spoken punctuation ("period" -> ".", "new line" -> "\n")
 • Expands voice snippet macros & personal dictionary terms
         │
         ▼
[Layer 2 AI Polish: llm.rs] (Optional, 1.5s hard timeout)
 • Off by default. Calls local Ollama or user API key if configured
 • Falls back immediately to Layer 1 if timeout expires
         │
         ▼
[Instant Paste: paste.rs]
 • Saves current clipboard contents to memory
 • Sets clipboard to clean text
 • Synthesizes Ctrl+V into focused window via SendInput
 • Sleeps 150ms for target app consumption
 • Restores original user clipboard contents
```

---

## 5. What Was Built & Verified So Far

1. **Step 0 (REA Analysis)**:
   * Produced `/docs/behavior-spec.md` analyzing public Wispr Flow interaction model, indicator states, filler stripping, and macro expansion.
2. **Rust Environment & Toolchain**:
   * Installed and validated Rust `1.99.1` with GNU host tuple.
3. **Local Whisper Inference Engine**:
   * Downloaded `whisper-cli.exe`, `whisper-server.exe`, and AVX2 CPU runtime libraries.
   * Downloaded and verified `ggml-tiny.en.bin` and `ggml-base.en.bin`.
   * Tested on real JFK speech sample (`samples_jfk.wav`): 100% transcript accuracy in <1s.
4. **Core Rust Subsystems**:
   * `audio.rs`: 16 kHz mono with 300ms circular pre-roll buffer.
   * `vad.rs`: RMS energy gating and silence trimming.
   * `stt.rs`: In-memory audio transcription.
   * `cleanup.rs`: Regex disfluency engine, stutters, self-corrections, punctuation, snippets.
   * `paste.rs`: Win32 clipboard snapshot, Ctrl+V synthetic injection, clipboard restoration.
   * `hotkey.rs`: Low-level hook for Right Ctrl (PTT), Double-tap (hands-free), Esc (cancel).
   * `storage.rs`: Settings, personal dictionary, snippets, and history storage.
   * `main.rs`: Full async orchestrator connecting audio -> vad -> stt -> cleanup -> paste.

---

## 6. Daily Use: Relaunching the App

The app is packaged and installed, so it no longer needs Vite, cargo, or the dev repo console.

- **Launch manually**: double-click the `FlowLocal` desktop icon (also in Start Menu). Install location: `C:\Users\keith\AppData\Local\FlowLocal\flowlocal.exe`.
- **Standalone install**: the Whisper engine, all models (`~240 MB`), and your settings/history live inside the install folder (`C:\Users\keith\AppData\Local\FlowLocal`). The app does **not** depend on the `speech` source folder anymore — it copies those assets over on first packaged launch. Keep a backup of the repo (or unpack an installer on any machine) if you plan to develop or rebuild later, because `npx tauri build` needs the sources.
- **Launch automatically**: when Windows boots/the laptop wakes to login, FlowLocal starts by itself (it is registered in `HKCU\...\CurrentVersion\Run`). Toggle this in the app settings ("Launch at login"); the setting is stored and re-registered at every start.
- **Uninstall**: `uninstall.exe` in the install folder removes the app along with models/settings kept there.
- **Repackage after code changes**: from the repo root run `npx tauri build`, then re-run the installer at `C:\Users\keith\AppData\Local\flowlocal_target\release\bundle\nsis\FlowLocal_0.1.0_x64-setup.exe` (silent with `/S`). Reinstalled copies find existing models in the install folder and skip re-copying.
- **Developers**: `npm run tauri dev` runs UI + backend together in dev mode (a plain `cargo run` shows an error page because Vite is not running).

## 7. What Is Being Worked On Next

1. **Floating Indicator Pill Overlay & Tray**:
   * Compact always-on-top, non-activating, click-through overlay showing live audio waveform and state transitions (Idle, Listening, Processing, Done).
2. **Settings UI**:
   * React + TypeScript + Tailwind settings window (General, Models, AI Polish, Dictionary & Snippets, History, Debug & Benchmarks).
