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
│   ├── Cargo.toml             # Rust dependencies (cpal, hound, windows-sys/windows, tokio, etc.)
│   └── src/
│       ├── lib.rs             # Main orchestrator: setup, engine pipeline, Tauri commands
│       ├── audio.rs           # CPAL capture, 300ms ring buffer pre-roll, in-memory WAV
│       ├── vad.rs             # Energy & zero-crossing Voice Activity Detection
│       ├── stt.rs             # Local Whisper integration (greedy decoding, prompt bias)
│       ├── cleanup.rs         # Layer 1 deterministic cleanup (<5ms)
│       ├── llm.rs             # Layer 2 optional AI polish (Ollama, OpenAI, Claude)
│       ├── paste.rs           # Clipboard paste with verify-and-escalate chain (see §4)
│       ├── autostart.rs       # Launch-at-login registry management (HKCU Run key)
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
 • Sets clipboard to the clean text
 • Injects a paste shortcut into the focused window via SendInput — trying, in
   order, the app-specific combo (Ctrl+V, Ctrl+Shift+V or Shift+Insert for
   terminals, chosen by window class), then Shift+Insert, then Ctrl+Shift+V
 • After every injection, verifies with UI Automation that the focused
   element actually changed; retries only on strong evidence of failure.
   As a last resort it falls back to synthesized unicode typing (works in
   every app, including terminals that ignore clipboard paste)
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

## 7. Restore on a Fresh Machine (Agent / Human Runbook)

This section exists so that **any AI agent or person** can clone this repository on a fresh Windows machine and end up with a working, packaged, auto-starting FlowLocal. Follow it top to bottom and verify each checkpoint before moving on. Total time budget: ~20–40 minutes (mostly downloads and one compile).

### 7.0 Scope and assumptions

- **Target OS**: Windows 10 or 11, x64. Admin rights are NOT required (everything installs per-user).
- **Everything in the repo is source code only.** The Whisper engine (`bin/`), models (`models/`) and user data (`data/`) are **gitignored** and must be downloaded or generated on the new machine (steps 7.3 and 7.4 do this for you).
- Install (non-dev) locations on the new machine:
  - App + models + data: `C:\Users\<you>\AppData\Local\FlowLocal\`
  - Build cache: `C:\Users\<you>\AppData\Local\flowlocal_target\`
  - Toolchain: `C:\Users\<you>\AppData\Local\w64devkit\`

### 7.1 Prerequisites

1. **Python 3.x** — needed for the asset-download scripts. Check: `python --version`.
2. **Node.js (LTS) + npm** — frontend build. Check: `node --version && npm --version`. (Built with Node v22 / npm 10.)
3. **Rust with the GNU toolchain** — the app is built with `x86_64-pc-windows-gnu`, not MSVC:
   ```
   rustup default stable-x86_64-pc-windows-gnu
   ```
   Check: `rustup show`.
4. **WebView2 Runtime** — preinstalled on Windows 11 and most updated Windows 10 machines.

If Rust is not installed at all: download and run `rustup-init.exe` in *default mode*; then run the `rustup default` command above.

### 7.2 Get the code

```powershell
git clone https://github.com/keithpatel/Flow-local.git
cd Flow-local
npm install
```

### 7.3 MinGW toolchain (w64devkit)

The GNU Rust toolchain needs a mingw gcc. The repo ships a script that downloads w64devkit into `%LOCALAPPDATA%\w64devkit` (no admin, no system PATH changes):

```powershell
python scripts\setup_w64devkit.py
```

Verify: `%LOCALAPPDATA%\w64devkit\w64devkit\bin\gcc.exe` exists.

### 7.4 Whisper engine + model (required — the repo does not ship binaries)

```powershell
python scripts\setup_whisper_engine.py
```

This downloads:
- the official precompiled whisper.cpp binaries (AVX2, ~23 MB) into `bin\whisper\`
- the `ggml-tiny.en.bin` model (~75 MB, the default model) into `models\`

Optional (recommended for better accuracy, ~142 MB):
```powershell
python -c "import scripts.setup_whisper_engine as s; s.setup_model('base.en')"
```
Or download `ggml-base.en.bin` (or other models from the `ggml-*` list) manually into `models\`.

**Important**: this step is *also* what makes the installer self-contained. At build time, `scripts\prepare_bundle_resources.js` (run automatically by `npx tauri build`) copies `bin\` and `models\` into `src-tauri\resources\`, and the bundler packs them into the installer. On first launch after installation the app unpacks them next to the installed exe (`%LOCALAPPDATA%\FlowLocal\`) and works standalone. Skipping this step produces an installer without a speech engine.

Verify: `bin\whisper\Release\whisper-cli.exe` and `models\ggml-tiny.en.bin` exist.

### 7.5 Build environment (PowerShell)

Set these for **every** build session (the same values live in `run.bat` for bash/cmd):

```powershell
$W = "$env:LOCALAPPDATA\w64devkit\w64devkit"
$env:PATH = "$W\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:LIBRARY_PATH = "$W\lib\gcc\x86_64-w64-mingw32\<GCC_VERSION>;$W\x86_64-w64-mingw32\lib"
$env:CARGO_TARGET_DIR = "$env:LOCALAPPDATA\flowlocal_target"
```

Notes for the agent:
- Replace `<GCC_VERSION>` with your actual gcc directory — run `gcc --version` and check `dir "$W\lib\gcc\x86_64-w64-mingw32"` (e.g. `16.2.0`).
- `CARGO_TARGET_DIR` must stay **outside any OneDrive-synced folder**. OneDrive's file-sharing locks cause `os error 32` failures in the middle of builds.

Quick sanity check (fails with `linker gcc not found` if step 7.5 was skipped):
```powershell
cargo check --manifest-path src-tauri\Cargo.toml
```

### 7.6 Build the installer

```powershell
npx tauri build
```

Artifacts (adjust prefix if you used a different `CARGO_TARGET_DIR`):
- NSIS installer: `%LOCALAPPDATA%\flowlocal_target\release\bundle\nsis\FlowLocal_0.1.0_x64-setup.exe` (**~194 MB** — fully self-contained: app + whisper engine + models)
- MSI installer: `%LOCALAPPDATA%\flowlocal_target\release\bundle\msi\FlowLocal_0.1.0_x64_en-US.msi` (~207 MB)

Verify: both files exist with sizes near the values above.

**This one file is all you ever need to install FlowLocal on any Windows x64 machine.** Copy it via USB or any transfer, run it (or `... /S` for silent), and the app works — no Python, Node, Rust, or network downloads required on the target machine.

### 7.7 Install

```powershell
Start-Process -Wait "$env:LOCALAPPDATA\flowlocal_target\release\bundle\nsis\FlowLocal_0.1.0_x64-setup.exe" -ArgumentList "/S"
```

On first launch the packaged app unpacks the bundled whisper engine and models from its own `resources` into the install folder (`%LOCALAPPDATA%\FlowLocal`), then usually copies `data\` from the build-time checkout folder for same-machine reinstalls. Afterwards the app is fully standalone — the repo folder can be deleted.

### 7.8 Verify (do not skip)

1. `tasklist | findstr flowlocal` → process running after first launch.
2. `%LOCALAPPDATA%\FlowLocal\models\ggml-tiny.en.bin` and `bin\whisper\Release\whisper-cli.exe` exist.
3. Hold **Right Ctrl** in Notepad, speak, release → clean text appears within a second or two.
4. Repeat inside a **terminal** (VS Code integrated terminal or Windows Terminal) → text pastes there too (it uses a different shortcut and may retry — this is the verify-and-escalate chain from §4).
5. Dictations appear under History in the app window.
6. Optional: enable **Launch at login** in settings, then check the registry:
   ```
   reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v FlowLocal
   ```

### 7.9 Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| Build dies with `os error 32` (file in use) | flowlocal.exe running, or build dir inside OneDrive | `taskkill /IM flowlocal.exe /F`; ensure `CARGO_TARGET_DIR` is outside OneDrive |
| App shows "Hmmm… can't reach this page" | plain `cargo run` in dev mode without Vite | Use `npx tauri dev` (starts Vite) or `npm run dev` in a second terminal |
| Transcription works but nothing pastes in one app | The app's paste chain needs larger/trying shortcuts | Read `%LOCALAPPDATA%\FlowLocal\data\paste.log` — it logs every shortcut attempt and its verdict (`landed=Yes/No/Unknown`) |
| `engine_ready: false` / "model not found" | whisper-cli or model missing in the *install folder* | Check `bin\whisper\Release\whisper-cli.exe` and `models\ggml-*.bin` under `%LOCALAPPDATA%\FlowLocal`. On a fresh install they are unpacked from the installer's own binaries on first launch — if missing there, run step 7.4 and rebuild; or launch once from a machine that has them |
| `rustc: linker gcc not found` | w64devkit not in PATH for this session | Repeat step 7.5 |
| Duplicated text pasted | Old build | Ensure you ran the installer built from the current source (§7.6), then re-dictate |

### 7.10 Everyday use after install

- **Launch**: double-click the `FlowLocal` desktop icon or use the Start Menu entry.
- **Auto-start**: the app registers itself in `HKCU\...\Run` (desktop icon shown, tray menu available). Toggle in settings ("Launch at login").
- **Distribute**: to install on another machine just copy `FlowLocal_0.1.0_x64-setup.exe` and run it — it carries the app, engine and every model bundled at build time.
- **Reinstall/update**: rebuild (§7.6) and re-run the installer with `/S`. Existing models in the install folder are reused.
- **Note**: the NSIS uninstaller only removes what it installed; any assets the app unpacked later (`bin\`, `models\`, `data\`, settings) are left behind by design so history is safe across reinstalls. Delete the folder after uninstalling if you want those gone.
- **Local data locations**: settings/history/dictionary: `%LOCALAPPDATA%\FlowLocal\data\*.json`; paste diagnostics: same folder, `paste.log`.

## 8. What Is Being Worked On Next

1. **Floating Indicator Pill Overlay & Tray**:
   * Compact always-on-top, non-activating, click-through overlay showing live audio waveform and state transitions (Idle, Listening, Processing, Done).
2. **Settings UI**:
   * React + TypeScript + Tailwind settings window (General, Models, AI Polish, Dictionary & Snippets, History, Debug & Benchmarks).
