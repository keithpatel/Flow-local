use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use hound::{WavSpec, WavWriter};
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

pub struct AudioRecorder {
    device_name: Option<String>,
    is_recording: Arc<Mutex<bool>>,
    pre_roll_buffer: Arc<Mutex<VecDeque<f32>>>,
    active_buffer: Arc<Mutex<Vec<f32>>>,
    current_rms: Arc<Mutex<f32>>,
    waveform: Arc<Mutex<VecDeque<f32>>>,
    stream: Option<Stream>,
}

impl AudioRecorder {
    pub const TARGET_SAMPLE_RATE: u32 = 16000;
    // 300 ms pre-roll at 16,000 Hz = 4,800 samples
    pub const PRE_ROLL_SAMPLES: usize = 4800;
    pub const WAVEFORM_BARS: usize = 56;

    pub fn new() -> Self {
        Self {
            device_name: None,
            is_recording: Arc::new(Mutex::new(false)),
            pre_roll_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(Self::PRE_ROLL_SAMPLES))),
            active_buffer: Arc::new(Mutex::new(Vec::new())),
            current_rms: Arc::new(Mutex::new(0.0)),
            waveform: Arc::new(Mutex::new(VecDeque::with_capacity(Self::WAVEFORM_BARS))),
            stream: None,
        }
    }

    pub fn list_microphones() -> Vec<String> {
        let host = cpal::default_host();
        match host.input_devices() {
            Ok(devices) => devices.filter_map(|d| d.name().ok()).collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn set_microphone(&mut self, name: Option<String>) {
        self.device_name = name;
    }

    pub fn start_stream(&mut self) -> Result<(), String> {
        let host = cpal::default_host();
        let device = if let Some(ref name) = self.device_name {
            host.input_devices()
                .map_err(|e| e.to_string())?
                .find(|d| d.name().map(|n| n == *name).unwrap_or(false))
                .ok_or_else(|| "Specified input device not found".to_string())?
        } else {
            host.default_input_device()
                .ok_or_else(|| "No default input device found".to_string())?
        };

        let default_config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get default input config: {}", e))?;

        let sample_rate = default_config.sample_rate().0;
        let channels = default_config.channels() as usize;

        let is_recording_clone = Arc::clone(&self.is_recording);
        let pre_roll_clone = Arc::clone(&self.pre_roll_buffer);
        let active_buf_clone = Arc::clone(&self.active_buffer);
        let current_rms_clone = Arc::clone(&self.current_rms);
        let waveform_clone = Arc::clone(&self.waveform);

        let err_fn = |err| eprintln!("Audio stream error: {}", err);

        let stream = match default_config.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &default_config.into(),
                move |data: &[f32], _| {
                    Self::process_incoming_samples(
                        data,
                        channels,
                        sample_rate,
                        &is_recording_clone,
                        &pre_roll_clone,
                        &active_buf_clone,
                        &current_rms_clone,
                        &waveform_clone,
                    );
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &default_config.into(),
                move |data: &[i16], _| {
                    let float_data: Vec<f32> =
                        data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                    Self::process_incoming_samples(
                        &float_data,
                        channels,
                        sample_rate,
                        &is_recording_clone,
                        &pre_roll_clone,
                        &active_buf_clone,
                        &current_rms_clone,
                        &waveform_clone,
                    );
                },
                err_fn,
                None,
            ),
            _ => return Err("Unsupported audio sample format".to_string()),
        }
        .map_err(|e| format!("Failed to build input stream: {}", e))?;

        stream
            .play()
            .map_err(|e| format!("Failed to play stream: {}", e))?;

        self.stream = Some(stream);
        Ok(())
    }

    fn process_incoming_samples(
        data: &[f32],
        channels: usize,
        src_sample_rate: u32,
        is_recording: &Arc<Mutex<bool>>,
        pre_roll: &Arc<Mutex<VecDeque<f32>>>,
        active_buf: &Arc<Mutex<Vec<f32>>>,
        current_rms: &Arc<Mutex<f32>>,
        waveform: &Arc<Mutex<VecDeque<f32>>>,
    ) {
        if data.is_empty() {
            return;
        }

        // Convert multi-channel to mono
        let mut mono_samples: Vec<f32> = Vec::with_capacity(data.len() / channels);
        for chunk in data.chunks(channels) {
            let sum: f32 = chunk.iter().sum();
            mono_samples.push(sum / channels as f32);
        }

        // Resample to 16,000 Hz if necessary
        let resampled = if src_sample_rate == Self::TARGET_SAMPLE_RATE {
            mono_samples
        } else {
            Self::simple_resample(
                &mono_samples,
                src_sample_rate as f32,
                Self::TARGET_SAMPLE_RATE as f32,
            )
        };

        // Calculate RMS volume for live waveform
        let rms = crate::vad::VadGate::calculate_rms(&resampled);
        if let Ok(mut r) = current_rms.try_lock() {
            *r = rms;
        }

        let peak = resampled.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
        if let Ok(mut bars) = waveform.try_lock() {
            if bars.len() >= Self::WAVEFORM_BARS {
                bars.pop_front();
            }
            bars.push_back(peak);
        }

        let recording = *is_recording.lock().unwrap();

        if recording {
            if let Ok(mut buf) = active_buf.lock() {
                buf.extend_from_slice(&resampled);
            }
        } else {
            // Idle: populate circular pre-roll buffer
            if let Ok(mut ring) = pre_roll.lock() {
                for sample in resampled {
                    if ring.len() >= Self::PRE_ROLL_SAMPLES {
                        ring.pop_front();
                    }
                    ring.push_back(sample);
                }
            }
        }
    }

    fn simple_resample(input: &[f32], src_rate: f32, dst_rate: f32) -> Vec<f32> {
        let ratio = src_rate / dst_rate;
        let target_len = (input.len() as f32 / ratio).round() as usize;
        let mut output = Vec::with_capacity(target_len);

        for i in 0..target_len {
            let src_idx = i as f32 * ratio;
            let idx_floor = src_idx.floor() as usize;
            let idx_ceil = (idx_floor + 1).min(input.len().saturating_sub(1));
            let t = src_idx - idx_floor as f32;

            let sample = if idx_floor < input.len() {
                input[idx_floor] * (1.0 - t) + input[idx_ceil] * t
            } else {
                0.0
            };
            output.push(sample);
        }
        output
    }

    pub fn start_recording(&self) {
        let mut rec = self.is_recording.lock().unwrap();
        if !*rec {
            *rec = true;
            let mut active = self.active_buffer.lock().unwrap();
            active.clear();
            if let Ok(mut bars) = self.waveform.lock() {
                bars.clear();
            }

            // Pre-seed active buffer with pre-roll contents so initial phoneme is never clipped
            let ring = self.pre_roll_buffer.lock().unwrap();
            active.extend(ring.iter());
        }
    }

    pub fn stop_recording(&self) -> Vec<f32> {
        let mut rec = self.is_recording.lock().unwrap();
        *rec = false;

        let mut active = self.active_buffer.lock().unwrap();
        let samples = active.clone();
        active.clear();
        samples
    }

    pub fn cancel_recording(&self) {
        let mut rec = self.is_recording.lock().unwrap();
        *rec = false;
        let mut active = self.active_buffer.lock().unwrap();
        active.clear();
    }

    pub fn get_current_rms(&self) -> f32 {
        *self.current_rms.lock().unwrap()
    }

    pub fn handle(&self) -> AudioHandle {
        AudioHandle {
            is_recording: Arc::clone(&self.is_recording),
            pre_roll_buffer: Arc::clone(&self.pre_roll_buffer),
            active_buffer: Arc::clone(&self.active_buffer),
            current_rms: Arc::clone(&self.current_rms),
            waveform: Arc::clone(&self.waveform),
        }
    }

    pub fn level_handle(&self) -> Arc<Mutex<f32>> {
        Arc::clone(&self.current_rms)
    }

    pub fn waveform_handle(&self) -> Arc<Mutex<VecDeque<f32>>> {
        Arc::clone(&self.waveform)
    }

    /// Converts raw 16kHz f32 samples to 16-bit PCM WAV bytes in memory
    pub fn samples_to_wav(samples: &[f32]) -> Result<Vec<u8>, String> {
        let spec = WavSpec {
            channels: 1,
            sample_rate: Self::TARGET_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = WavWriter::new(&mut cursor, spec).map_err(|e| e.to_string())?;
            for &sample in samples {
                // Clamp and convert to i16
                let clamped = sample.max(-1.0).min(1.0);
                let val = (clamped * i16::MAX as f32) as i16;
                writer.write_sample(val).map_err(|e| e.to_string())?;
            }
            writer.finalize().map_err(|e| e.to_string())?;
        }

        Ok(cursor.into_inner())
    }
}

/// A thread-safe control handle for an already-running capture stream.
/// The underlying cpal `Stream` stays on the thread that created it; this
/// handle only shares the lock-free buffers with the processing thread.
#[derive(Clone)]
pub struct AudioHandle {
    is_recording: Arc<Mutex<bool>>,
    pre_roll_buffer: Arc<Mutex<VecDeque<f32>>>,
    active_buffer: Arc<Mutex<Vec<f32>>>,
    current_rms: Arc<Mutex<f32>>,
    waveform: Arc<Mutex<VecDeque<f32>>>,
}

impl AudioHandle {
    pub fn start_recording(&self) {
        let mut rec = self.is_recording.lock().unwrap();
        if !*rec {
            *rec = true;
            let mut active = self.active_buffer.lock().unwrap();
            active.clear();
            if let Ok(mut bars) = self.waveform.lock() {
                bars.clear();
            }
            let ring = self.pre_roll_buffer.lock().unwrap();
            active.extend(ring.iter());
        }
    }

    pub fn stop_recording(&self) -> Vec<f32> {
        let mut rec = self.is_recording.lock().unwrap();
        *rec = false;
        let mut active = self.active_buffer.lock().unwrap();
        let samples = active.clone();
        active.clear();
        samples
    }

    pub fn cancel_recording(&self) {
        let mut rec = self.is_recording.lock().unwrap();
        *rec = false;
        let mut active = self.active_buffer.lock().unwrap();
        active.clear();
    }

    pub fn get_current_rms(&self) -> f32 {
        *self.current_rms.lock().unwrap()
    }

    pub fn samples_to_wav(samples: &[f32]) -> Result<Vec<u8>, String> {
        AudioRecorder::samples_to_wav(samples)
    }
}
