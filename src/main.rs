mod audio;
mod config;
mod hotkey;
mod output;
mod status;
mod transcribe;

use anyhow::{bail, Context, Result};
use clap::Parser;
use std::sync::mpsc;
use std::time::Instant;

use hotkey::HotkeyEvent;
use transcribe::TranscribeBackend;

#[derive(Parser)]
#[command(name = "dictr", version, about = "Push-to-talk voice dictation")]
struct Cli {
    /// Transcription backend: "local" or "api"
    #[arg(long)]
    backend: Option<String>,

    /// Path to whisper model (.bin)
    #[arg(long)]
    model: Option<String>,

    /// Hotkey name (e.g. AltGr, F9)
    #[arg(long)]
    hotkey: Option<String>,

    /// Use clipboard paste instead of xdotool type
    #[arg(long)]
    paste: bool,

    /// List available input devices and exit
    #[arg(long)]
    list_devices: bool,

    /// Input device: index, name, or substring (see --list-devices)
    #[arg(long)]
    device: Option<String>,

    /// Language code for transcription (e.g. en, fr, de)
    #[arg(long)]
    language: Option<String>,

    /// API endpoint URL for the api backend
    #[arg(long)]
    api_url: Option<String>,

    /// Initial prompt to guide transcription (e.g. technical terms)
    #[arg(long)]
    initial_prompt: Option<String>,

    /// Minimum recording duration in milliseconds (default: 300)
    #[arg(long)]
    min_duration: Option<u64>,

    /// Show verbose output (model loading, debug info)
    #[arg(long, short)]
    verbose: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.list_devices {
        let devices = audio::list_input_devices()?;
        if devices.is_empty() {
            eprintln!("no input devices found");
        }
        for (i, (name, desc, is_default)) in devices.iter().enumerate() {
            let marker = if *is_default { " (default)" } else { "" };
            if name != desc {
                println!("{i}: {desc}{marker}");
                println!("   {name}");
            } else {
                println!("{i}: {name}{marker}");
            }
        }
        return Ok(());
    }

    let mut config = config::Config::load()?;
    output::check_deps()?;

    // Suppress whisper.cpp/ggml logging unless --verbose
    if !cli.verbose {
        whisper_rs::install_logging_hooks();
    }

    // CLI overrides
    apply_cli_overrides(&mut config, &cli);

    // Init transcription backend
    let mut backend: Box<dyn TranscribeBackend> = match config.backend.as_str() {
        "local" => {
            let path = config.resolved_model_path();
            if !path.exists() {
                bail!(
                    "model not found at {}. Download from https://huggingface.co/ggerganov/whisper.cpp/tree/main",
                    path.display()
                );
            }
            if cli.verbose {
                eprintln!("loading model from {}...", path.display());
            }
            let path_str = path.to_str().context("invalid UTF-8 in model path")?;
            Box::new(transcribe::LocalWhisper::new(path_str)?)
        }
        "api" => {
            if config.api_key.is_empty() {
                bail!("API key required. Set api_key in config or OPENAI_API_KEY env var");
            }
            Box::new(transcribe::ApiWhisper::new(
                config.api_key.clone(),
                config.api_url.clone(),
            )?)
        }
        other => bail!("unknown backend: {other}"),
    };

    // Init audio
    let mut recorder = audio::AudioRecorder::new(config.device.as_deref())?;
    if cli.verbose {
        eprintln!(
            "mic ready: {} ({}Hz)",
            recorder.device_name(),
            recorder.sample_rate()
        );
    }

    // Start hotkey listener
    let (tx, rx) = mpsc::channel();
    let _hotkey_thread = hotkey::start_listener(&config.hotkey, tx)?;
    if cli.verbose {
        eprintln!("hold [{}] to record, release to transcribe", config.hotkey);
    }
    status::set("idle");

    // Main event loop
    let mut press_time: Option<Instant> = None;

    loop {
        match rx.recv()? {
            HotkeyEvent::Pressed => {
                press_time = Some(Instant::now());
                recorder.start()?;
                status::set("recording");
                if cli.verbose {
                    eprint!("recording... ");
                }
            }
            HotkeyEvent::Released => {
                let audio = recorder.stop()?;

                // Skip short presses
                let duration = press_time.take().map(|t| t.elapsed());
                if let Some(d) = duration {
                    let min_secs = config.min_duration_ms as f32 / 1000.0;
                    if d.as_secs_f32() < min_secs {
                        if cli.verbose {
                            eprintln!("too short ({:.1}s), skipping", d.as_secs_f32());
                        }
                        status::set("idle");
                        continue;
                    }
                    if cli.verbose {
                        eprint!("{:.1}s ", d.as_secs_f32());
                    }
                }

                if audio.is_empty() {
                    if cli.verbose {
                        eprintln!("no audio captured");
                    }
                    status::set("idle");
                    continue;
                }

                status::set("transcribing");
                if cli.verbose {
                    eprint!("transcribing... ");
                }
                match backend.transcribe(
                    &audio,
                    config.language.as_deref(),
                    config.initial_prompt.as_deref(),
                ) {
                    Ok(text) if text.is_empty() => {
                        if cli.verbose {
                            eprintln!("(empty transcription)");
                        }
                    }
                    Ok(text) => {
                        let text = config.apply_replacements(&text);
                        if cli.verbose {
                            eprintln!("{text}");
                        }
                        if cli.paste {
                            output::paste_text(&text)?;
                        } else {
                            output::type_text(&text, config.typing_delay_ms)?;
                        }
                    }
                    Err(e) => {
                        eprintln!("transcription error: {e}");
                    }
                }
                status::set("idle");
            }
        }
    }
}

fn apply_cli_overrides(config: &mut config::Config, cli: &Cli) {
    if let Some(b) = &cli.backend {
        config.backend = b.clone();
    }
    if let Some(m) = &cli.model {
        config.model_path = m.clone();
    }
    if let Some(h) = &cli.hotkey {
        config.hotkey = h.clone();
    }
    if cli.device.is_some() {
        config.device = cli.device.clone();
    }
    if cli.language.is_some() {
        config.language = cli.language.clone();
    }
    if let Some(url) = &cli.api_url {
        config.api_url = url.clone();
    }
    if cli.initial_prompt.is_some() {
        config.initial_prompt = cli.initial_prompt.clone();
    }
    if let Some(ms) = cli.min_duration {
        config.min_duration_ms = ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse_args(args: &[&str]) -> Cli {
        let mut full = vec!["dictr"];
        full.extend_from_slice(args);
        Cli::parse_from(full)
    }

    #[test]
    fn cli_override_backend() {
        let mut config = config::Config::default();
        let cli = parse_args(&["--backend", "api"]);
        apply_cli_overrides(&mut config, &cli);
        assert_eq!(config.backend, "api");
    }

    #[test]
    fn cli_override_model() {
        let mut config = config::Config::default();
        let cli = parse_args(&["--model", "/tmp/model.bin"]);
        apply_cli_overrides(&mut config, &cli);
        assert_eq!(config.model_path, "/tmp/model.bin");
    }

    #[test]
    fn cli_override_hotkey() {
        let mut config = config::Config::default();
        let cli = parse_args(&["--hotkey", "F9"]);
        apply_cli_overrides(&mut config, &cli);
        assert_eq!(config.hotkey, "F9");
    }

    #[test]
    fn cli_override_device() {
        let mut config = config::Config::default();
        let cli = parse_args(&["--device", "AT2020"]);
        apply_cli_overrides(&mut config, &cli);
        assert_eq!(config.device, Some("AT2020".into()));
    }

    #[test]
    fn cli_override_language() {
        let mut config = config::Config::default();
        let cli = parse_args(&["--language", "fr"]);
        apply_cli_overrides(&mut config, &cli);
        assert_eq!(config.language, Some("fr".into()));
    }

    #[test]
    fn cli_override_api_url() {
        let mut config = config::Config::default();
        let cli = parse_args(&["--api-url", "http://localhost:8080/v1/transcriptions"]);
        apply_cli_overrides(&mut config, &cli);
        assert_eq!(config.api_url, "http://localhost:8080/v1/transcriptions");
    }

    #[test]
    fn cli_override_initial_prompt() {
        let mut config = config::Config::default();
        let cli = parse_args(&["--initial-prompt", "NixOS, Rust"]);
        apply_cli_overrides(&mut config, &cli);
        assert_eq!(config.initial_prompt, Some("NixOS, Rust".into()));
    }

    #[test]
    fn cli_override_min_duration() {
        let mut config = config::Config::default();
        let cli = parse_args(&["--min-duration", "500"]);
        apply_cli_overrides(&mut config, &cli);
        assert_eq!(config.min_duration_ms, 500);
    }

    #[test]
    fn cli_no_overrides_preserves_defaults() {
        let mut config = config::Config::default();
        let cli = parse_args(&[]);
        apply_cli_overrides(&mut config, &cli);
        assert_eq!(config.backend, "local");
        assert_eq!(config.hotkey, "AltGr");
        assert_eq!(config.min_duration_ms, 300);
        assert!(config.device.is_none());
        assert!(config.language.is_none());
        assert!(config.initial_prompt.is_none());
    }
}
