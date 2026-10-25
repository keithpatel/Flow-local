import { invoke } from "@tauri-apps/api/core";

export type Provider = "Disabled" | "LocalOllama" | "OpenAiCompatible" | "Anthropic";
export type Mode = "Raw" | "Clean" | "Email" | "Chat" | "NotesBullets" | "CodeComments" | "Custom";

export interface LlmConfig {
  provider: Provider;
  active_mode: Mode;
  base_url: string;
  model_name: string;
  api_key: string;
  custom_prompt: string;
  timeout_ms: number;
}

export interface Settings {
  hotkey: string;
  microphone: string | null;
  active_model: string;
  launch_at_login: boolean;
  auto_punctuation: boolean;
  default_language: string;
  history_retention_days: number;
  llm: LlmConfig;
}

export interface Status {
  state: "idle" | "listening" | "processing" | "done" | "error" | "paused";
  transcript: string;
  latency_ms: number;
  stt_ms: number;
  cleanup_ms: number;
  rms: number;
  paused: boolean;
}

export interface HistoryEntry {
  id: string;
  timestamp: number;
  duration_ms: number;
  raw_text: string;
  clean_text: string;
  latency_ms: number;
  mode: string;
}

export interface AppInfo {
  model: string;
  hotkey: string;
  models: string[];
  engine_ready: boolean;
  cpu: string;
  gpu: string;
  version: string;
  microphones: string[];
}

export const api = {
  getStatus: () => invoke<Status>("get_status"),
  getWaveform: () => invoke<number[]>("get_waveform"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  getHistory: () => invoke<HistoryEntry[]>("get_history"),
  clearHistory: () => invoke<void>("clear_history"),
  getDictionary: () => invoke<Record<string, string>>("get_dictionary"),
  saveDictionary: (dictionary: Record<string, string>) =>
    invoke<void>("save_dictionary", { dictionary }),
  getSnippets: () => invoke<Record<string, string>>("get_snippets"),
  saveSnippets: (snippets: Record<string, string>) =>
    invoke<void>("save_snippets", { snippets }),
  getAppInfo: () => invoke<AppInfo>("get_app_info"),
  setPaused: (paused: boolean) => invoke<boolean>("set_paused", { paused }),
  startPtt: () => invoke<void>("ptt_start"),
  stopPtt: () => invoke<void>("ptt_stop"),
  toggleHandsFree: () => invoke<void>("hands_free_toggle"),
  cancel: () => invoke<void>("cancel_dictation"),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  testPaste: (text: string, delayMs: number) =>
    invoke<void>("test_paste", { text, delayMs }),
  quit: () => invoke<void>("quit_app"),
  show: () => invoke<void>("show_window"),
};

export function formatTime(ts: number) {
  return new Date(ts * 1000).toLocaleString();
}
