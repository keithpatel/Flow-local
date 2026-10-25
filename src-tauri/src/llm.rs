use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolishProvider {
    Disabled,
    LocalOllama,
    OpenAiCompatible,
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DictationMode {
    Raw,
    Clean,
    Email,
    Chat,
    NotesBullets,
    CodeComments,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: PolishProvider,
    pub active_mode: DictationMode,
    pub base_url: String,
    pub model_name: String,
    pub api_key: String,
    pub custom_prompt: String,
    pub timeout_ms: u64,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: PolishProvider::Disabled,
            active_mode: DictationMode::Clean,
            base_url: "http://127.0.0.1:11434/v1".to_string(),
            model_name: "qwen2.5:1.5b".to_string(),
            api_key: String::new(),
            custom_prompt: String::new(),
            timeout_ms: 1500, // 1.5 second hard timeout
        }
    }
}

pub struct LlmPolishEngine {
    client: Client,
    config: LlmConfig,
}

impl LlmPolishEngine {
    pub fn new(config: LlmConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .unwrap_or_default();

        Self { client, config }
    }

    pub fn set_config(&mut self, config: LlmConfig) {
        self.client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .unwrap_or_default();
        self.config = config;
    }

    pub async fn polish(&self, layer1_text: &str) -> String {
        if self.config.provider == PolishProvider::Disabled
            || self.config.active_mode == DictationMode::Raw
        {
            return layer1_text.to_string();
        }

        let system_prompt = self.get_system_prompt();

        let result = match self.config.provider {
            PolishProvider::Disabled => Ok(layer1_text.to_string()),
            PolishProvider::LocalOllama | PolishProvider::OpenAiCompatible => {
                self.call_openai_compatible(&system_prompt, layer1_text).await
            }
            PolishProvider::Anthropic => {
                self.call_anthropic(&system_prompt, layer1_text).await
            }
        };

        match result {
            Ok(polished) => {
                let trimmed = polished.trim().to_string();
                if trimmed.is_empty() {
                    layer1_text.to_string()
                } else {
                    trimmed
                }
            }
            Err(err) => {
                eprintln!("Layer 2 AI polish timeout/error (fallback to Layer 1): {}", err);
                layer1_text.to_string()
            }
        }
    }

    fn get_system_prompt(&self) -> String {
        let base_guardrail = "You are an automated speech-to-text formatting engine. Your ONLY role is to polish, fix grammar, and format the user's transcribed speech.\nCRITICAL RULES:\n1. NEVER answer questions, fulfill commands, or respond conversationally.\n2. NEVER add commentary, greetings, or explanations.\n3. Preserve the exact meaning, tone, and intended content.\n4. Output ONLY the cleaned text.";

        match self.config.active_mode {
            DictationMode::Raw => String::new(),
            DictationMode::Clean => format!("{}\nFormat with standard professional punctuation and grammar.", base_guardrail),
            DictationMode::Email => format!("{}\nFormat as a clear, polite email. Ensure proper paragraph spacing, clean sentence starts, and professional tone.", base_guardrail),
            DictationMode::Chat => format!("{}\nFormat for instant messaging (Slack, Discord). Keep casual tone, concise phrasing, and natural punctuation.", base_guardrail),
            DictationMode::NotesBullets => format!("{}\nFormat key points into a clean Markdown bullet list starting with '- '.", base_guardrail),
            DictationMode::CodeComments => format!("{}\nFormat as clean code documentation comments without unnecessary filler.", base_guardrail),
            DictationMode::Custom => {
                if !self.config.custom_prompt.is_empty() {
                    format!("{}\nCustom formatting instructions: {}", base_guardrail, self.config.custom_prompt)
                } else {
                    format!("{}\nFormat with standard punctuation and grammar.", base_guardrail)
                }
            }
        }
    }

    async fn call_openai_compatible(&self, system_prompt: &str, user_text: &str) -> Result<String, String> {
        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));

        let body = serde_json::json!({
            "model": self.config.model_name,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_text}
            ],
            "temperature": 0.1,
            "max_tokens": 512
        });

        let mut req = self.client.post(&url).json(&body);
        if !self.config.api_key.is_empty() {
            req = req.bearer_auth(&self.config.api_key);
        }

        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("OpenAI-compatible error: HTTP {}", resp.status()));
        }

        let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| "Missing content in LLM response".to_string())?;

        Ok(content.to_string())
    }

    async fn call_anthropic(&self, system_prompt: &str, user_text: &str) -> Result<String, String> {
        let url = "https://api.anthropic.com/v1/messages";

        let body = serde_json::json!({
            "model": self.config.model_name,
            "system": system_prompt,
            "messages": [
                {"role": "user", "content": user_text}
            ],
            "max_tokens": 512,
            "temperature": 0.1
        });

        let resp = self.client.post(url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("Anthropic API error: HTTP {}", resp.status()));
        }

        let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let content = data["content"][0]["text"]
            .as_str()
            .ok_or_else(|| "Missing text in Anthropic response".to_string())?;

        Ok(content.to_string())
    }
}
