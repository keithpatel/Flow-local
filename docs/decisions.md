# FlowLocal: Architecture & Technical Decisions

This document records the architectural and engineering decisions made during the implementation of FlowLocal, addressing operating system detection, toolchain constraints, audio pipelines, inference engines, and input injection.

---

## 1. Environment & Toolchain Selection

### Context
FlowLocal is developed on a Windows laptop with an **Intel Core i5-7200U CPU (2 physical cores, 4 threads)**, **Intel HD Graphics 620 integrated GPU**, and **8 GB RAM**. The host operating system does not have Microsoft Visual Studio C++ Build Tools or the Windows SDK installed.

### Decision
1. **Rust GNU Toolchain (`stable-x86_64-pc-windows-gnu`)**:
   - We installed Rust `1.99.1` with the `x86_64-pc-windows-gnu` host tuple.
   - **Rationale**: Avoids multi-gigabyte Visual Studio C++ build tool installation dependencies, enabling fast, clean native compilation and linking on Windows.
2. **Audio, Hotkey, and Core Engine in Rust**:
   - The hot path (audio capture, pre-roll ring buffer, global low-level keyboard hooks, VAD, Layer 1 text cleanup, clipboard snapshot/injection) is implemented in pure native Rust for zero-overhead, sub-millisecond execution.
3. **Settings Dashboard in React + TypeScript + Tailwind**:
   - For configuration, model management, personal dictionary editing, history viewer, and benchmarking analytics, a clean, modern web interface is provided.

---

## 2. Audio Capture & Pre-Roll Ring Buffer

### Requirements
- 16 kHz Mono PCM audio.
- Under 50 ms latency from hotkey press to audio streaming.
- ~300 ms pre-roll buffer so the user's initial syllable is never truncated.
- Zero disk writes (100% RAM only).

### Implementation
- **Audio Driver**: Rust `cpal` (Cross-Platform Audio Library) utilizing the Windows WASAPI backend in shared mode with low latency buffer frames.
- **Ring Buffer**: A circular ring buffer of 4,800 samples (300 ms at 16,000 samples/sec) continuously records in transient memory while idle.
- When the hotkey is pressed (PTT start or Hands-Free start):
  1. The pre-roll buffer is cloned as the head of the active recording slice.
  2. Incoming audio packets are appended to the active recording buffer.
  3. RMS volume metrics are calculated on 20 ms frames and pushed to the visualizer channel for live waveform animation.
- **Zero Disk Persistence**: Raw audio is stored exclusively in RAM (`Vec<f32>` / `Vec<i16>`) and deallocated immediately following transcription.

---

## 3. Voice Activity Detection (VAD) & Silence Gating

### Strategy
Before passing audio to the speech-to-text inference engine, audio frames pass through a low-latency VAD gate:
1. **Energy & Spectral Gate**: Quick RMS energy threshold combined with zero-crossing rate rejects keyboard clicks, quiet breath sounds, and ambient background hiss.
2. **Minimum Duration Check**: Any recording with less than 250 ms of active speech is rejected as empty/accidental key tap.
3. **Trimming**: Leading and trailing silence buffers (beyond 150 ms comfort padding) are trimmed to minimize inference computation time.

---

## 4. Local Speech-to-Text: Whisper Engine & Optimization

### Target Models & Hardware Detection
On the detected Intel i5-7200U (2 cores, 4 threads, Intel HD Graphics 620):
- **Integrated GPU (No CUDA)**: CPU execution with AVX2 instruction set using all physical cores (2 cores, 4 threads).
- **Default Recommended Model**: `base.en` (142 MB) or `tiny.en` (75 MB) providing real-time factor (RTF) of ~0.25–0.45x on this hardware, achieving under 1s inference for 5s of speech.
- **Model Selection Catalog**:
  - `tiny.en` (~75 MB): Ultra-fast (<300 ms).
  - `base.en` (~142 MB): Recommended balance of accuracy and speed.
  - `small.en` (~466 MB): High accuracy, recommended for complex vocabulary.
  - `distil-large-v3` (~1.5 GB): Distilled large model.
  - `large-v3-turbo` (~1.6 GB): Latest high-precision model.
  - Multilingual variants (`tiny`, `base`, `small`).

### In-Memory Residency & Greedy Decoding
- **Warm Model**: The selected Whisper model is loaded into memory on application launch and kept hot. This eliminates the 1.5s–3s cold-load delay on every recording.
- **Decoding Configuration**: Greedy decoding (`beam_size = 1`, `temperature = 0.0`), timestamps disabled, initial prompt bias active.

---

## 5. Global Hotkey & Target Focus Retention

### Windows Hotkey Hook
- Using `SetWindowsHookExW(WH_KEYBOARD_LL)`:
  - Detects `VK_RCONTROL` (Right Ctrl) key-down to start recording and key-up to stop and trigger transcription.
  - Intercepts `VK_ESCAPE` to immediately cancel recording.
  - Intercepts double-tap or `Right Ctrl + Space` to toggle hands-free mode.
- Non-blocking: Hotkey events are processed asynchronously on a dedicated Windows message pump thread.
- **No Focus Stealing**: The floating overlay window is created with `WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW` and `WS_POPUP`, ensuring Windows never diverts keyboard focus away from the active target application.

---

## 6. Instant Paste & Clipboard Preservation

### Paste Pipeline
1. Capture snapshot of active clipboard:
   - Text formats (`CF_UNICODETEXT`).
2. Populate clipboard with the clean transcribed text.
3. Synthesize `Ctrl+V` using `SendInput`:
   - `KEYEVENTF_EXTENDEDKEY` + `VK_CONTROL` down
   - `VK_V` down
   - `VK_V` up
   - `VK_CONTROL` up
4. Asynchronous delay (150 ms) to give the active app sufficient time to read the clipboard.
5. Restore the user's prior clipboard snapshot.

### Fallback
If clipboard paste is blocked by the target application or security policy, FlowLocal falls back to direct character typing via `SendInput` with `KEYEVENTF_UNICODE`.

---

## 7. Two-Layer Text Cleanup Architecture

### Layer 1: Deterministic Engine (<5 ms)
- 100% offline, zero network, zero AI latency.
- Implemented with compiled Rust regexes:
  - Filler words (`um`, `uh`, `you know`, etc.).
  - Stutters and repeat tokens (`I I` -> `I`).
  - Spoken self-corrections (`meet at 3, no, 4 pm` -> `meet at 4 pm`).
  - Spoken punctuation (`period`, `comma`, `new line`, `bullet list`).
  - Number & currency formatting.
  - Personal dictionary replacements.
  - Voice snippet expansion.

### Layer 2: Semantic AI Polish (Optional)
- Supports local Ollama instances or user-configured API endpoints (OpenAI-compatible / Anthropic).
- Hard-coded 1500 ms timeout: guarantees paste is never delayed beyond acceptable human typing thresholds.
- Strict system prompt: strictly prohibits conversational answers, commentary, or hallucinations.
