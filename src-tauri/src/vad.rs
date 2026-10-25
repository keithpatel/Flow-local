pub struct VadGate {
    pub energy_threshold: f32,
    pub min_speech_duration_ms: usize,
    pub sample_rate: usize,
}

impl VadGate {
    pub fn new(sample_rate: usize) -> Self {
        Self {
            energy_threshold: 0.008, // Adaptive RMS threshold for voice presence
            min_speech_duration_ms: 250,
            sample_rate,
        }
    }

    /// Checks if the audio contains sufficient voice activity.
    /// Returns (is_speech_present, trimmed_samples)
    pub fn process(&self, samples: &[f32]) -> (bool, Vec<f32>) {
        if samples.is_empty() {
            return (false, Vec::new());
        }

        let frame_size = self.sample_rate / 50; // 20ms frames
        if samples.len() < frame_size {
            return (false, Vec::new());
        }

        let mut speech_frames = 0;
        let mut first_speech_idx = None;
        let mut last_speech_idx = None;

        for (i, chunk) in samples.chunks(frame_size).enumerate() {
            let rms = Self::calculate_rms(chunk);
            if rms > self.energy_threshold {
                speech_frames += 1;
                let sample_pos = i * frame_size;
                if first_speech_idx.is_none() {
                    first_speech_idx = Some(sample_pos);
                }
                last_speech_idx = Some(sample_pos + chunk.len());
            }
        }

        let total_speech_ms = speech_frames * 20;
        if total_speech_ms < self.min_speech_duration_ms {
            return (false, Vec::new());
        }

        // Add 150ms comfort padding before first speech and after last speech
        let pad_samples = (self.sample_rate as f32 * 0.15) as usize;
        let start = first_speech_idx.unwrap_or(0).saturating_sub(pad_samples);
        let end = (last_speech_idx.unwrap_or(samples.len()) + pad_samples).min(samples.len());

        let trimmed = samples[start..end].to_vec();
        (true, trimmed)
    }

    pub fn calculate_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silence_rejection() {
        let vad = VadGate::new(16000);
        let silence = vec![0.0001; 16000]; // 1s near-zero silence
        let (has_speech, _) = vad.process(&silence);
        assert!(!has_speech);
    }

    #[test]
    fn test_speech_detection() {
        let vad = VadGate::new(16000);
        // 500ms of active simulated speech wave
        let mut samples = vec![0.0; 16000];
        for i in 4000..12000 {
            samples[i] = (i as f32 * 0.1).sin() * 0.15;
        }
        let (has_speech, trimmed) = vad.process(&samples);
        assert!(has_speech);
        assert!(!trimmed.is_empty());
    }
}
