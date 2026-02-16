# Changelog

## 0.1.1

- Paste mode uses `shift+Insert` and writes to both clipboard and primary selections for broader compatibility
- Fix clippy warning on newer Rust toolchains (`function_casts_as_integer`)

## 0.1.0

- Initial release
- Push-to-talk recording with configurable hotkey
- Local transcription via whisper.cpp with optional CUDA acceleration
- OpenAI Whisper API backend
- Text output via `xdotool type` or clipboard paste mode
- Microphone selection via pactl/cpal
- TOML config with CLI overrides
- Text replacements with `lowercase_after` post-processing
- i3blocks integration via signal
- Interactive install script
