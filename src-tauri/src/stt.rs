use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct WhisperEngine {
    bin_dir: PathBuf,
    models_dir: PathBuf,
    active_model: String,
    prompt_bias: String,
    #[allow(dead_code)]
    server_process: Arc<Mutex<Option<Child>>>,
    #[allow(dead_code)]
    server_port: u16,
}

impl WhisperEngine {
    pub fn new(app_root: &Path, active_model: &str) -> Self {
        let bin_dir = app_root.join("bin").join("whisper").join("Release");
        let models_dir = app_root.join("models");

        Self {
            bin_dir,
            models_dir,
            active_model: active_model.to_string(),
            prompt_bias: String::new(),
            server_process: Arc::new(Mutex::new(None)),
            server_port: 8080,
        }
    }

    pub fn set_prompt_bias(&mut self, bias: &str) {
        self.prompt_bias = bias.to_string();
    }

    pub fn set_active_model(&mut self, model: &str) {
        self.active_model = model.to_string();
    }

    pub fn get_model_file_path(&self) -> PathBuf {
        let filename = if self.active_model.starts_with("ggml-") {
            format!("{}.bin", self.active_model)
        } else {
            format!("ggml-{}.bin", self.active_model)
        };
        self.models_dir.join(filename)
    }

    pub fn is_model_available(&self) -> bool {
        self.get_model_file_path().exists()
    }

    /// Transcribes WAV audio bytes in-memory and returns (transcribed_text, latency_ms)
    pub fn transcribe_wav_bytes(&self, wav_bytes: &[u8]) -> Result<(String, u128), String> {
        let model_path = self.get_model_file_path();
        if !model_path.exists() {
            return Err(format!(
                "Whisper model file not found: {}",
                model_path.display()
            ));
        }

        let cli_path = self.bin_dir.join("whisper-cli.exe");
        if !cli_path.exists() {
            return Err(format!(
                "whisper-cli.exe not found at {}",
                cli_path.display()
            ));
        }

        // Write WAV bytes to transient scratch buffer in %TEMP%
        let temp_wav_path = std::env::temp_dir().join(format!("flowlocal_transcribe_{}.wav", std::process::id()));
        fs::write(&temp_wav_path, wav_bytes)
            .map_err(|e| format!("Failed to write transient wav: {}", e))?;

        let start_time = Instant::now();

        let mut cmd = Command::new(&cli_path);
        // Suppress the console window the whisper-cli would otherwise pop up
        // in the user's face on every dictation. 0x08000000 = CREATE_NO_WINDOW.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        cmd.arg("-m").arg(&model_path)
            .arg("-f").arg(&temp_wav_path)
            .arg("-t").arg("4")           // 4 logical cores on i5-7200U
            .arg("-bs").arg("1")          // Greedy decoding
            .arg("-fa")                   // Flash attention
            .arg("-nt")                   // No timestamps
            .arg("-np");                  // No prints / clean stdout

        if !self.prompt_bias.is_empty() {
            cmd.arg("--prompt").arg(&self.prompt_bias);
        }

        let output = cmd.output().map_err(|e| format!("Failed to execute whisper-cli: {}", e))?;
        let latency_ms = start_time.elapsed().as_millis();

        // Remove transient WAV
        let _ = fs::remove_file(&temp_wav_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Whisper error (exit code {:?}): {}", output.status.code(), stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let raw_text = Self::extract_clean_transcript(&stdout);

        Ok((raw_text, latency_ms))
    }

    fn extract_clean_transcript(stdout: &str) -> String {
        let mut clean_lines = Vec::new();
        for line in stdout.lines() {
            let trimmed = line.trim();
            // Filter out internal whisper debug logs if any slipped into stdout
            if trimmed.starts_with("system_info:")
                || trimmed.starts_with("load_backend:")
                || trimmed.starts_with("whisper_")
                || trimmed.starts_with("read_audio_data:")
            {
                continue;
            }
            if !trimmed.is_empty() {
                clean_lines.push(trimmed);
            }
        }
        clean_lines.join(" ")
    }
}
