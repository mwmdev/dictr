use std::path::Path;

use dictr::transcribe::{LocalWhisper, TranscribeBackend};

/// Generate 2 seconds of 440Hz sine wave at 16kHz as f32 samples.
fn generate_test_audio() -> Vec<f32> {
    let sample_rate = 16000;
    let duration_secs = 2;
    let n = sample_rate * duration_secs;
    (0..n)
        .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate as f32).sin() * 0.3)
        .collect()
}

#[test]
fn test_local_whisper_transcribes_audio() {
    let model_path = format!(
        "{}/.local/share/dictr/models/ggml-base.bin",
        std::env::var("HOME").unwrap()
    );
    if !Path::new(&model_path).exists() {
        eprintln!("skipping: model not found at {model_path}");
        return;
    }

    let mut backend = LocalWhisper::new(&model_path).expect("failed to create LocalWhisper");
    let audio = generate_test_audio();

    // A sine wave isn't speech, so we just verify the pipeline completes without error
    let result = backend.transcribe(&audio, None, None);
    assert!(result.is_ok(), "transcribe failed: {}", result.unwrap_err());
}

#[test]
fn test_local_whisper_with_language() {
    let model_path = format!(
        "{}/.local/share/dictr/models/ggml-base.bin",
        std::env::var("HOME").unwrap()
    );
    if !Path::new(&model_path).exists() {
        eprintln!("skipping: model not found at {model_path}");
        return;
    }

    let mut backend = LocalWhisper::new(&model_path).expect("failed to create LocalWhisper");
    let audio = generate_test_audio();

    let result = backend.transcribe(&audio, Some("en"), None);
    assert!(
        result.is_ok(),
        "transcribe with language failed: {}",
        result.unwrap_err()
    );
}

#[test]
fn test_local_whisper_with_initial_prompt() {
    let model_path = format!(
        "{}/.local/share/dictr/models/ggml-base.bin",
        std::env::var("HOME").unwrap()
    );
    if !Path::new(&model_path).exists() {
        eprintln!("skipping: model not found at {model_path}");
        return;
    }

    let mut backend = LocalWhisper::new(&model_path).expect("failed to create LocalWhisper");
    let audio = generate_test_audio();

    let result = backend.transcribe(&audio, Some("en"), Some("NixOS, Rust"));
    assert!(
        result.is_ok(),
        "transcribe with prompt failed: {}",
        result.unwrap_err()
    );
}

#[test]
fn test_local_whisper_empty_audio() {
    let model_path = format!(
        "{}/.local/share/dictr/models/ggml-base.bin",
        std::env::var("HOME").unwrap()
    );
    if !Path::new(&model_path).exists() {
        eprintln!("skipping: model not found at {model_path}");
        return;
    }

    let mut backend = LocalWhisper::new(&model_path).expect("failed to create LocalWhisper");

    // Empty audio should succeed with empty text, not panic
    let result = backend.transcribe(&[], None, None);
    assert!(result.is_ok());
}
