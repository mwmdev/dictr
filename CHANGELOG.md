# Changelog

## 0.2.0

### Added
- `--file` flag to transcribe audio files directly (via ffmpeg)

### Fixed
- Signal handler now uses async-signal-safe `libc::unlink` instead of `std::fs::remove_file` (could deadlock on exit)
- API backend has 30s HTTP timeout (previously could hang indefinitely)
- Pre-allocate audio buffer to prevent realtime reallocation glitches
- Replacement order is now deterministic (sorted keys)
- `check_deps` verifies xdotool/xclip exit codes, not just spawnability
- File mode no longer sets wrong `.pt` model path
- Temp WAV file uses PID-based name to avoid collisions
- Failed transcription segments are logged instead of silently dropped
- API response uses typed struct instead of brittle `HashMap<String, String>`
- Embed CUDA toolkit rpaths so binary works outside nix-shell

## 0.1.1

- Append space to all transcriptions for seamless typing continuation
- Handle all BLANK_AUDIO variants (with/without parentheses and underscores) as empty transcription
- Simplify i3blocks indicators by removing REC text and transcribing dots
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
