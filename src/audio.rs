use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use rubato::{FftFixedIn, Resampler};
use std::sync::{Arc, Mutex};

const TARGET_SAMPLE_RATE: u32 = 16_000;

pub struct AudioRecorder {
    device: Device,
    config: StreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<Stream>,
    display_name: String,
}

impl AudioRecorder {
    pub fn new(device_name: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();

        let (device, display_name) = match device_name {
            Some(name) => {
                // Try index, then pactl name/description match
                let pactl_match = query_pactl_sources().and_then(|sources| {
                    if let Ok(idx) = name.parse::<usize>() {
                        return sources.into_iter().nth(idx);
                    }
                    sources.into_iter().find(|(n, d, _)| {
                        n == name || d.to_lowercase().contains(&name.to_lowercase())
                    })
                });

                if let Some((pactl_name, desc, _)) = pactl_match {
                    std::env::set_var("PIPEWIRE_NODE", &pactl_name);
                    let dev = host
                        .default_input_device()
                        .with_context(|| format!("failed to open device '{desc}'"))?;
                    (dev, desc)
                } else {
                    // Fallback: cpal device name matching
                    let devices = host
                        .input_devices()
                        .context("failed to enumerate input devices")?;
                    let dev = devices
                        .into_iter()
                        .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                        .with_context(|| format!("input device '{name}' not found"))?;
                    let display = dev.name().unwrap_or_else(|_| name.to_string());
                    (dev, display)
                }
            }
            None => {
                let dev = host
                    .default_input_device()
                    .context("no input device found")?;
                let display = default_source_description()
                    .unwrap_or_else(|| dev.name().unwrap_or_else(|_| "unknown".into()));
                (dev, display)
            }
        };

        let supported = device.default_input_config()?;
        let config: StreamConfig = supported.into();

        Ok(Self {
            device,
            config,
            buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            display_name,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        // Clear previous recording
        self.buffer.lock().expect("audio buffer poisoned").clear();

        let buffer = Arc::clone(&self.buffer);
        let channels = self.config.channels as usize;

        let err_fn = |err| eprintln!("audio stream error: {err}");

        let stream = self.device.build_input_stream(
            &self.config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Downmix to mono inline
                let mut buf = buffer.lock().expect("audio buffer poisoned");
                for chunk in data.chunks(channels) {
                    let mono: f32 = chunk.iter().sum::<f32>() / channels as f32;
                    buf.push(mono);
                }
            },
            err_fn,
            None,
        )?;

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<Vec<f32>> {
        // Drop stream to stop recording
        self.stream.take();

        let raw = std::mem::take(&mut *self.buffer.lock().expect("audio buffer poisoned"));
        let source_rate = self.config.sample_rate.0 as usize;

        if source_rate == TARGET_SAMPLE_RATE as usize {
            return Ok(raw);
        }

        resample(&raw, source_rate, TARGET_SAMPLE_RATE as usize)
    }

    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate.0
    }

    pub fn device_name(&self) -> &str {
        &self.display_name
    }
}

/// Returns (pactl_name, description, is_default) for each input source.
pub fn list_input_devices() -> Result<Vec<(String, String, bool)>> {
    if let Some(devices) = query_pactl_sources() {
        if !devices.is_empty() {
            return Ok(devices);
        }
    }
    // Fallback to cpal
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.name().ok());
    let devices = host
        .input_devices()
        .context("failed to enumerate input devices")?;
    let mut result = Vec::new();
    for device in devices {
        if let Ok(name) = device.name() {
            let is_default = default_name.as_deref() == Some(&name);
            result.push((name.clone(), name, is_default));
        }
    }
    Ok(result)
}

/// Query PipeWire/PulseAudio sources via pactl. Returns None if pactl is unavailable.
fn query_pactl_sources() -> Option<Vec<(String, String, bool)>> {
    let default = std::process::Command::new("pactl")
        .args(["get-default-source"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let output = std::process::Command::new("pactl")
        .args(["list", "sources"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();
    let mut current_name = None;

    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("Name: ") {
            current_name = Some(name.to_string());
        } else if let Some(desc) = trimmed.strip_prefix("Description: ") {
            if let Some(name) = current_name.take() {
                // Skip monitor sources (output capture, not mic input)
                if !name.contains(".monitor") {
                    devices.push((name.clone(), desc.to_string(), name == default));
                }
            }
        }
    }

    Some(devices)
}

/// Get the description of the current default source.
fn default_source_description() -> Option<String> {
    query_pactl_sources()?
        .into_iter()
        .find(|(_, _, is_default)| *is_default)
        .map(|(_, desc, _)| desc)
}

fn resample(input: &[f32], from_rate: usize, to_rate: usize) -> Result<Vec<f32>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let chunk_size = 1024;
    let mut resampler = FftFixedIn::<f32>::new(from_rate, to_rate, chunk_size, 2, 1)?;

    let mut output = Vec::with_capacity(input.len() * to_rate / from_rate + 1024);

    // Process full chunks
    let mut pos = 0;
    while pos + chunk_size <= input.len() {
        let chunk = &input[pos..pos + chunk_size];
        let result = resampler.process(&[chunk], None)?;
        output.extend_from_slice(&result[0]);
        pos += chunk_size;
    }

    // Process remaining samples by padding with zeros
    if pos < input.len() {
        let mut last_chunk = vec![0.0f32; chunk_size];
        let remaining = input.len() - pos;
        last_chunk[..remaining].copy_from_slice(&input[pos..]);
        let result = resampler.process(&[&last_chunk], None)?;
        // Only take proportional output
        let expected = remaining * to_rate / from_rate;
        let take = expected.min(result[0].len());
        output.extend_from_slice(&result[0][..take]);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_empty_input() {
        let result = resample(&[], 44100, 16000).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn resample_preserves_approximate_duration() {
        // 1 second of silence at 44100Hz
        let input = vec![0.0f32; 44100];
        let output = resample(&input, 44100, 16000).unwrap();
        // Should be approximately 16000 samples (within 5% tolerance)
        let ratio = output.len() as f64 / 16000.0;
        assert!(
            (0.95..=1.05).contains(&ratio),
            "expected ~16000 samples, got {}",
            output.len()
        );
    }

    #[test]
    fn resample_44100_to_16000_sine() {
        // Generate 440Hz sine wave at 44100Hz for 0.5s
        let n = 44100 / 2;
        let input: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        let output = resample(&input, 44100, 16000).unwrap();

        let expected = n * 16000 / 44100;
        let ratio = output.len() as f64 / expected as f64;
        assert!(
            (0.9..=1.1).contains(&ratio),
            "expected ~{expected} samples, got {}",
            output.len()
        );

        // Output should have non-trivial signal (not all zeros)
        let energy: f32 = output.iter().map(|s| s * s).sum();
        assert!(energy > 0.1, "resampled signal has no energy");
    }

    #[test]
    fn resample_same_rate_skipped_in_recorder() {
        // When source == target, resample isn't called, but test it anyway
        let input = vec![0.5f32; 16000];
        let output = resample(&input, 16000, 16000).unwrap();
        assert_eq!(output.len(), input.len());
    }

    #[test]
    fn list_input_devices_returns_results() {
        // May return empty on CI, but should not panic
        let devices = list_input_devices().unwrap();
        let default_count = devices
            .iter()
            .filter(|(_, _, is_default)| *is_default)
            .count();
        assert!(default_count <= 1, "at most one default device");
    }

    #[test]
    fn resample_non_chunk_aligned_length() {
        // Input not evenly divisible by chunk_size (1024)
        let input = vec![0.1f32; 3000];
        let output = resample(&input, 48000, 16000).unwrap();
        assert!(!output.is_empty());
    }
}
