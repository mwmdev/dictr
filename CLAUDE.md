# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test

```bash
# CPU build (default)
nix-shell --run "cargo build --release"

# CUDA GPU build
nix-shell --run "cargo build --release --features cuda"

# Unit tests only (50 tests, no model needed)
nix-shell --run "cargo test --release --bin dictr"

# Lib tests (config + transcribe modules via lib.rs)
nix-shell --run "cargo test --release --lib"

# All tests including e2e (requires model at ~/.local/share/dictr/models/ggml-base.bin)
nix-shell --run "cargo test --release"

# Run a single test
nix-shell --run "cargo test --release --bin dictr debounce_repeated_press"

# Lint and format (CI enforces these)
nix-shell --run "cargo clippy -- -D warnings"
nix-shell --run "cargo fmt --check"
```

On non-NixOS, drop the `nix-shell --run` wrapper and install deps manually (see README).

**Note:** `shell.nix` always includes CUDA packages. CPU-only builds still work — the CUDA libs are just unused.

## Architecture

Single-threaded event loop with two background threads:

```
rdev thread ──HotkeyEvent──> main thread ──> AudioRecorder.start/stop()
                                         ──> TranscribeBackend.transcribe()
                                         ──> output::type_text() (xdotool)

cpal callback thread ──> Arc<Mutex<Vec<f32>>> shared buffer
```

**Module roles:**

- `main.rs` — CLI parsing, config merge via `apply_cli_overrides()`, event loop (blocks on mpsc receiver)
- `config.rs` — TOML config with serde defaults, tilde expansion, env var fallback. Loaded once at startup.
- `hotkey.rs` — rdev listener thread. `Debouncer` struct suppresses X11 key repeat. Sends `Pressed`/`Released` via mpsc.
- `audio.rs` — cpal mic capture. Callback downmixes to mono inline. `stop()` resamples to 16kHz via rubato `FftFixedIn`.
- `transcribe.rs` — `TranscribeBackend` trait with two impls: `LocalWhisper` (whisper-rs) and `ApiWhisper` (reqwest multipart POST). `encode_wav()` converts f32→i16 WAV for API upload.
- `output.rs` — Shells out to `xdotool type` or `xclip` + `xdotool key ctrl+v`. Validates deps at startup.
- `status.rs` — Writes state to `/tmp/dictr-status`, signals i3blocks via `pkill -RTMIN+11`. Registers SIGINT/SIGTERM cleanup via `libc::signal`.
- `lib.rs` — Thin re-export of `config` and `transcribe` modules for integration tests.

## Key Design Details

- **CUDA is opt-in**: The `cuda` feature flag passes through to `whisper-rs/cuda`. No conditional compilation in dictr source — both backends are always compiled.
- **Sync main loop**: Transcription blocks the main thread. API backend uses `tokio::runtime::Runtime` with `block_on()` to bridge sync/async.
- **Reqwest client reuse**: `ApiWhisper` creates `reqwest::Client` once in `new()` and clones it per request (Arc internally, cheap clone).
- **Audio buffer**: `Arc<Mutex<Vec<f32>>>` shared between cpal callback and main thread. Mutex uses `.expect()` (panics if poisoned).
- **Resampler is stateless per recording**: New `FftFixedIn` instance on each `stop()` call. Remainder samples are zero-padded with proportional output truncation.
- **i3blocks signal 11 is hardcoded** in `status.rs` — must match `signal=11` in i3blocks config.
- **Config precedence**: CLI flags > TOML file > defaults. For `api_key`: TOML > `OPENAI_API_KEY` env var.
- **Configurable fields**: `api_url` (custom API endpoint), `initial_prompt` (guide transcription), `min_duration_ms` (minimum recording length).
- **Verbose mode**: `--verbose`/`-v` enables whisper.cpp/ggml log output and dictr status messages. Default is silent — whisper logs suppressed via `install_logging_hooks()`.
- **Replacements**: Config supports a `[replacements]` table for case-insensitive text substitution on transcription output (e.g., `"slash " = "/"`). Empty keys are skipped. Applied in `Config::apply_replacements()`.
- **E2E tests** use `LocalWhisper` via the `TranscribeBackend` trait (not raw whisper-rs). Skip gracefully if model file is missing (no CI failure). CI only runs unit tests (`--bin dictr`).
