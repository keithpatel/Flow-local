use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub hotkey: String,
    pub microphone: Option<String>,
    pub active_model: String,
    pub launch_at_login: bool,
    pub auto_punctuation: bool,
    pub default_language: String,
    pub history_retention_days: u32, // 0 = never delete, 1, 7, 30
    pub llm: crate::llm::LlmConfig,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "Right Ctrl".to_string(),
            microphone: None,
            active_model: "tiny.en".to_string(),
            launch_at_login: false,
            auto_punctuation: true,
            default_language: "en".to_string(),
            history_retention_days: 30,
            llm: crate::llm::LlmConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: u64,
    pub duration_ms: u64,
    pub raw_text: String,
    pub clean_text: String,
    pub latency_ms: u128,
    pub mode: String,
}

pub struct StorageManager {
    data_dir: PathBuf,
    settings: Arc<Mutex<AppSettings>>,
    dictionary: Arc<Mutex<HashMap<String, String>>>,
    snippets: Arc<Mutex<HashMap<String, String>>>,
    history: Arc<Mutex<Vec<HistoryEntry>>>,
}

impl StorageManager {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let mut storage = Self {
            data_dir,
            settings: Arc::new(Mutex::new(AppSettings::default())),
            dictionary: Arc::new(Mutex::new(HashMap::new())),
            snippets: Arc::new(Mutex::new(HashMap::new())),
            history: Arc::new(Mutex::new(Vec::new())),
        };
        storage.load_all();
        storage
    }

    fn settings_path(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }

    fn dict_path(&self) -> PathBuf {
        self.data_dir.join("dictionary.json")
    }

    fn snippets_path(&self) -> PathBuf {
        self.data_dir.join("snippets.json")
    }

    fn history_path(&self) -> PathBuf {
        self.data_dir.join("history.json")
    }

    pub fn load_all(&mut self) {
        if let Ok(data) = fs::read_to_string(self.settings_path()) {
            if let Ok(s) = serde_json::from_str::<AppSettings>(&data) {
                *self.settings.lock().unwrap() = s;
            }
        }
        if let Ok(data) = fs::read_to_string(self.dict_path()) {
            if let Ok(d) = serde_json::from_str::<HashMap<String, String>>(&data) {
                *self.dictionary.lock().unwrap() = d;
            }
        }
        if let Ok(data) = fs::read_to_string(self.snippets_path()) {
            if let Ok(snip) = serde_json::from_str::<HashMap<String, String>>(&data) {
                *self.snippets.lock().unwrap() = snip;
            }
        }
        if let Ok(data) = fs::read_to_string(self.history_path()) {
            if let Ok(h) = serde_json::from_str::<Vec<HistoryEntry>>(&data) {
                *self.history.lock().unwrap() = h;
            }
        }
    }

    pub fn save_settings(&self, settings: &AppSettings) {
        *self.settings.lock().unwrap() = settings.clone();
        if let Ok(json) = serde_json::to_string_pretty(settings) {
            let _ = fs::write(self.settings_path(), json);
        }
    }

    pub fn get_settings(&self) -> AppSettings {
        self.settings.lock().unwrap().clone()
    }

    pub fn save_dictionary(&self, dict: &HashMap<String, String>) {
        *self.dictionary.lock().unwrap() = dict.clone();
        if let Ok(json) = serde_json::to_string_pretty(dict) {
            let _ = fs::write(self.dict_path(), json);
        }
    }

    pub fn get_dictionary(&self) -> HashMap<String, String> {
        self.dictionary.lock().unwrap().clone()
    }

    pub fn save_snippets(&self, snippets: &HashMap<String, String>) {
        *self.snippets.lock().unwrap() = snippets.clone();
        if let Ok(json) = serde_json::to_string_pretty(snippets) {
            let _ = fs::write(self.snippets_path(), json);
        }
    }

    pub fn get_snippets(&self) -> HashMap<String, String> {
        self.snippets.lock().unwrap().clone()
    }

    pub fn add_history_entry(&self, raw: &str, clean: &str, duration_ms: u64, latency_ms: u128, mode: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let entry = HistoryEntry {
            id: format!("hist_{}", now),
            timestamp: now,
            duration_ms,
            raw_text: raw.to_string(),
            clean_text: clean.to_string(),
            latency_ms,
            mode: mode.to_string(),
        };

        let mut hist = self.history.lock().unwrap();
        hist.insert(0, entry);

        // Retention policy: trim entries older than retention days if > 0
        let retention = self.settings.lock().unwrap().history_retention_days;
        if retention > 0 {
            let cutoff = now.saturating_sub(retention as u64 * 86400);
            hist.retain(|e| e.timestamp >= cutoff);
        }

        if let Ok(json) = serde_json::to_string_pretty(&*hist) {
            let _ = fs::write(self.history_path(), json);
        }
    }

    pub fn get_history(&self) -> Vec<HistoryEntry> {
        self.history.lock().unwrap().clone()
    }

    pub fn clear_history(&self) {
        let mut hist = self.history.lock().unwrap();
        hist.clear();
        let _ = fs::write(self.history_path(), "[]");
    }
}
