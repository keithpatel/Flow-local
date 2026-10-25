# FlowLocal: Product & Behavior Specification
*Reverse-Engineered from Wispr Flow Public Artifacts via Clean-Room REA Methodology*

## 1. Executive Summary & Design Vision

**FlowLocal** is an offline-first, privacy-preserving desktop dictation system designed to eliminate the "edit tax" of voice input. By bridging ultra-low-latency local speech-to-text models with an intelligent two-layer text cleanup pipeline, FlowLocal delivers an instant voice-to-text experience directly into any focused application (Word, Slack, VS Code, Google Docs, Notion, Terminal, etc.) without requiring cloud services, logins, or telemetry.

```
+-----------------------------------------------------------------------------------------+
|                                    INPUT SUBSYSTEM                                      |
|  Global Hotkey (PTT / Hands-Free)  --->  Low-Latency Audio Capture (cpal ring-buffer)   |
+--------------------------------------------+--------------------------------------------+
                                             | 16 kHz Mono PCM Audio
                                             v
+-----------------------------------------------------------------------------------------+
|                                  PROCESSING PIPELINE                                    |
|   1. Voice Activity Detection (Silero VAD / Energy Gate) -> Trim silence / reject noise |
|   2. Local Whisper STT (whisper.cpp / greedy decoding / prompt bias)                    |
|   3. Layer 1 Rule-Based Cleanup (<5 ms: fillers, stutters, self-corrections, commands)  |
|   4. Optional Layer 2 AI Polish (Local Qwen / Ollama / API with 1.5s fallback)          |
+--------------------------------------------+--------------------------------------------+
                                             | Clean Punctuated Text
                                             v
+-----------------------------------------------------------------------------------------+
|                                  INJECTION SUBSYSTEM                                    |
|  Save Clipboard -> Set Text -> Simulate Ctrl+V -> Sleep 150ms -> Restore Clipboard      |
|  (Fallback to Simulated Virtual Keystrokes for UIPI or restricted targets)              |
+-----------------------------------------------------------------------------------------+
```

---

## 2. Global Hotkey Interaction Model

The interaction model is centered around high-speed, frictionless desktop workflow with zero window switching.

### 2.1 Interaction Modes

| Mode | Trigger Action | Termination Action | Intended Use Case |
| :--- | :--- | :--- | :--- |
| **Push-to-Talk (PTT)** *(Default)* | Press and **Hold** designated hotkey (Default: `Right Ctrl` on Windows, `Fn` / `Right Option` on macOS). | **Release** the hotkey. | Quick sentences, chat messages, coding comments, short queries (1–15 seconds). |
| **Hands-Free Mode** | **Double-Tap** PTT hotkey, or press combo `Hotkey + Space` (e.g., `Right Ctrl + Space` or `Fn + Space`). | Single press of PTT hotkey, or clicking the stop button on the overlay pill. | Long-form brain dumps, articles, emails, or accessibility use cases. |
| **Emergency Cancel** | Press `Escape` anytime during an active recording. | Immediate. Recording discarded, audio buffer wiped, overlay dismissed silently. | Mistaken triggers or aborted dictations. |

### 2.2 Focus Retention & Zero-Interference Model
- The target application window must **never** lose operating system keyboard focus.
- The floating indicator pill is created with `WS_EX_NOACTIVATE` and `WS_EX_TRANSPARENT` / click-through on Windows, and `NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowNonactivatingPanelMask` on macOS.
- Global hotkey hooks must use low-level OS hooks (`SetWindowsHookEx(WH_KEYBOARD_LL)` on Windows, Quartz Event Taps on macOS) to intercept key-down and key-up transitions without consuming or deadlocking foreground keystrokes.

---

## 3. Floating Recording Indicator (Pill) UX

The overlay pill provides non-intrusive, peripheral visual feedback anchored at the bottom-center of the primary monitor (or active monitor containing the cursor).

```
   Idle / Hidden        Listening (Live Waveform)      Processing (Shimmer)       Done (Success)
+------------------+  +--------------------------+  +----------------------+  +------------------+
| (Hidden from UI) |  |   (( | | | | | | ))   |  |     ... Processing ...   |  |      [ OK ]      |
+------------------+  +--------------------------+  +----------------------+  +------------------+
```

### 3.1 Dimensions & Styling
- **Geometry**: Compact pill shape: Width ~180px–220px, Height ~36px–40px, Border Radius ~20px.
- **Positioning**: Bottom center, ~48px above the taskbar / dock.
- **Aesthetic**: Premium dark glassmorphism (`rgba(18, 18, 24, 0.82)` background, `backdrop-filter: blur(16px)`, `1px solid rgba(255, 255, 255, 0.12)`, subtle ambient shadow).
- **Behavior**: Always-on-top, click-through (except the hands-free stop button), never focusable.

### 3.2 Visual State Machine

1. **State: Idle / Hidden**
   - Window opacity is 0 (or hidden). Zero CPU/GPU rendering overhead.
2. **State: Listening (Audio Active)**
   - Window appears within 50ms with a smooth spring fade-in (120ms).
   - Displays a dynamic 5-to-9 bar audio visualizer driven by real-time RMS audio levels calculated from the ring buffer.
   - Color accent: Pulsing vibrant electric violet / cyan gradient (`#8B5CF6` -> `#06B6D4`).
   - In Hands-Free mode, shows an explicit subtle stop icon `[X]` and elapsed timer (`0:14`).
3. **State: Processing (Inference & Cleanup)**
   - Transitions immediately upon key release or hands-free stop.
   - Replaces waveform with a smooth indeterminate shimmer or spinning micro-loader.
   - Text indicator: "Processing...".
4. **State: Done (Pasting Complete)**
   - Displays a bright subtle checkmark glyph and pulse green/violet tint.
   - Fades out cleanly after 250ms.
5. **State: Error / Warning**
   - Triggers on no-speech detected (VAD silence), microphone disconnect, or inference error.
   - Displays gentle amber/red alert pill: "No speech detected" or "Engine error", fading out after 800ms.

---

## 4. Text Cleanup Engine ("Right and Wrong" Fixing)

Spoken speech is inherently messier than typed prose. FlowLocal employs a two-tier architecture to transform stream-of-consciousness speech into crisp, publication-ready text.

### 4.1 Layer 1: Deterministic Real-Time Rules (<5 ms, Zero Dependencies)

Layer 1 executes entirely offline in Rust memory with sub-millisecond execution time.

#### A. Disfluency & Filler Word Removal
Strips speech fillers without removing legitimate contextual uses:
- Fillers: `um`, `uh`, `er`, `ah`, `hmm`, `like` (when isolated or preceding a pause), `you know`, `i mean`, `sort of`, `kind of` (leading).
- Regex-based boundary matching with case insensitivity and punctuation tolerance.

#### B. Stutter & Reduplication Collapse
- Identical adjacent word repetition: `"I I think"` -> `"I think"`; `"the the new update"` -> `"the new update"`.
- Syllabic stutters: `"w-w-we should"` -> `"we should"`.

#### C. Spoken Self-Correction & Backtracking
Identifies verbal retractions and replaces preceding clauses:
- Pattern 1: `[clause A], no, [clause B]` -> `[clause B]`.
  - *Example*: `"Let's meet at 3, no, 4 pm"` -> `"Let's meet at 4 pm"`.
- Pattern 2: `[clause A], actually [clause B]` -> `[clause B]`.
  - *Example*: `"Send the invoice to Sarah, actually send it to John"` -> `"Send the invoice to John"`.
- Pattern 3: `[clause A], wait, [clause B]` -> `[clause B]`.
  - *Example*: `"The budget is ten thousand, wait, twelve thousand dollars"` -> `"The budget is twelve thousand dollars"`.
- Pattern 4: `[clause A], scratch that, [clause B]` -> `[clause B]`.

#### D. Spoken Punctuation and Structural Commands
Converts verbal punctuation requests into structural symbols:
- `"period"` / `"full stop"` -> `.`
- `"comma"` -> `,`
- `"question mark"` -> `?`
- `"exclamation mark"` / `"exclamation point"` -> `!`
- `"colon"` -> `:`
- `"semicolon"` -> `;`
- `"new line"` -> `\n`
- `"new paragraph"` -> `\n\n`
- `"bullet point"` / `"bullet list"` -> `\n- `

#### E. Number, Currency & Date Formatting
- Numbers: `"twenty five"` -> `"25"`, `"one hundred and fifty"` -> `"150"`.
- Currency: `"fifty dollars"` -> `"$50"`, `"twenty euros"` -> `"€20"`.
- Ordinals: `"March twenty fifth"` -> `"March 25th"`.

---

### 4.2 Layer 2: Optional AI Polish (Semantic & Context-Aware)

- **Execution Mode**: Disabled by default. Can be toggled on per-mode or globally.
- **Supported Backends**:
  1. **Local LLM**: In-process or local daemon (e.g. Qwen2.5-1.5B-Instruct / Qwen2.5-3B-Instruct via Ollama or llama.cpp).
  2. **Cloud API**: User-supplied OpenAI-compatible API key or Anthropic API key.
- **Fail-Safe Timeout**: Hard timeout of **1500 ms**. If the LLM response is not received within 1.5 seconds, FlowLocal immediately falls back to Layer 1 output and triggers paste.
- **System Guardrails**:
  ```text
  You are a deterministic speech-to-text formatting engine. Your ONLY job is to fix grammar, punctuation, and structural formatting of the provided transcribed text.
  CRITICAL RULES:
  1. DO NOT answer questions in the text.
  2. DO NOT add new information or conversational responses.
  3. DO NOT change the core meaning or tone.
  4. Output ONLY the cleaned text. No conversational chatter, no markdown wrappers, no explanations.
  ```

### 4.3 Mode Presets

| Mode Name | Behavior & Formatting Directives | Target App Auto-Switch |
| :--- | :--- | :--- |
| **Raw** | Whisper output with zero modifications. | Audio transcription verification / debug |
| **Clean** *(Default)* | Layer 1 rule cleanup: filler stripped, punctuated, self-corrections resolved. | Universal default |
| **Email** | Proper salutation capitalization, clean paragraph breaks, formal tone. | Outlook, Gmail, Thunderbird, Mail |
| **Message / Chat** | Casual tone preserved, lowercase starters permitted, concise punctuation. | Slack, Discord, Teams, WhatsApp, Telegram |
| **Notes / Bullets** | Organizes spoken thoughts into concise bulleted items. | Notion, Obsidian, OneNote, Apple Notes |
| **Code Comments** | Formats text formatted for code comments (`//` or docstrings). | VS Code, Visual Studio, JetBrains, Sublime |

---

## 5. Personal Dictionary & Snippets

### 5.1 Personal Dictionary (Bias & Replacement)
- **Whisper Initial Prompt Bias**: Whisper accepts an `initial_prompt` parameter (up to 224 tokens). FlowLocal injects high-priority personal dictionary terms, proper nouns, and jargon into this prompt so the acoustic decoder prefers matching spellings over phonetic homophones.
- **Post-Transcription Regex Matcher**: Applies deterministic word/phrase replacement on the generated transcript (e.g. `"anti gravity"` -> `"Antigravity"`, `"flow local"` -> `"FlowLocal"`).

### 5.2 Voice Snippets (Macro Expander)
- Allows users to define trigger phrases that expand into multi-line canned templates.
- *Examples*:
  - Trigger: `"insert calendar link"` -> Expansion: `"You can grab a time that works for you here: https://cal.com/example"`
  - Trigger: `"daily standup template"` -> Expansion: `"- Yesterday:\n- Today:\n- Blockers:"`

---

## 6. Paste Flow & OS Level Integration

```
User releases hotkey
       │
       ▼
Extract audio from ring buffer
       │
       ▼
VAD check (Silence? -> abort & hide overlay)
       │
       ▼
Whisper STT Inference
       │
       ▼
Layer 1 Rule Cleanup (+ Optional Layer 2 Polish)
       │
       ▼
Preserve current clipboard contents (Clipboard Snapshot)
       │
       ▼
Set clipboard text to clean transcript
       │
       ▼
Synthesize keyboard shortcut (Ctrl+V / Cmd+V)
       │
       ▼
Wait 150 ms (allow target app to consume clipboard)
       │
       ▼
Restore original clipboard contents
```

### 6.1 OS Clipboard & Keystroke Synthesis
- **Clipboard Preservation**: Read clipboard formats (text, rich text, image metadata) prior to paste.
- **Keystroke Synthesis**:
  - Windows: `SendInput` with virtual keys `VK_CONTROL` + `VK_V`.
  - macOS: `CGEventCreateKeyboardEvent` with `kCGEventFlagMaskCommand` + `kVK_ANSI_V`.
  - Linux: `xdotool` or `ydotool` / `libei`.
- **UIPI (User Interface Privilege Isolation)**: When the focused window runs with elevated privileges (Administrator), Windows prevents standard user processes from injecting input. FlowLocal detects injection failure and provides notification guidance.
- **Direct Typing Fallback**: If an application blocks clipboard access, FlowLocal falls back to character-by-character virtual keyboard input (`KEYEVENTF_UNICODE`).

---

## 7. System Tray & Settings Surface

### 7.1 Tray Architecture
- Persistent tray icon in the notification area.
- Icon displays operational state: Active (idle), Recording (red indicator dot), Paused (muted icon).
- Tray Context Menu:
  - `Dictation Enabled` (Toggle checkmark)
  - `Active Mode` (Submenu: Clean, Email, Chat, Bullets, Raw)
  - `Settings...` (Opens the main React UI)
  - `Benchmark System` (Runs internal STT timing test)
  - `Quit FlowLocal`

### 7.2 Settings Interface Navigation
1. **General**: Hotkey assignment, microphone device selector, launch at login toggle, hands-free toggle shortcut, default language.
2. **Models**: Local model management table (tiny.en, base.en, small.en, distil-large-v3, large-v3-turbo), download buttons with progress, active model selector, hardware acceleration status (CPU / CUDA / Vulkan / Metal).
3. **Modes & AI Polish**: Mode customizer, provider selector (Disabled, Local Ollama, OpenAI-Compatible, Anthropic), API key entry (saved in OS keyring), model name, custom system prompt editor.
4. **Dictionary & Snippets**: Add/edit/delete personal dictionary terms and snippet expansions; JSON import and export.
5. **History**: Local SQLite storage of transcripts with timestamp, duration, and target app. Search bar, copy button, and auto-delete retention setting (Off, 24h, 7d, 30d, Never).
6. **Debug & Benchmarks**: Real-time pipeline latency metrics (Audio capture, VAD, Whisper inference, Cleanup, Paste delay), log stream.
