# dictr

[![CI](https://github.com/mwmdev/dictr/actions/workflows/ci.yml/badge.svg)](https://github.com/mwmdev/dictr/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/dictr)](https://crates.io/crates/dictr)
[![License](https://img.shields.io/crates/l/dictr)](LICENSE-MIT)

Push-to-talk voice dictation for Linux.

Singel binary - Private - Fast - Customizable

## Features
- **Push-to-talk** — hold a hotkey to record, release to transcribe and paste
- **Local inference** — runs [Whisper](https://github.com/ggerganov/whisper.cpp) locally, your audio never leaves your machine
- **CUDA GPU acceleration** — optional NVIDIA GPU support for sub-second transcription
- **OpenAI API fallback** — use the OpenAI Whisper API as an alternative backend
- **Text replacements** — custom post-processing rules for text replacement

## Usage

```sh
dictr                          # Default: AltGr hotkey, local whisper, xdotool type
dictr --hotkey F9              # Use F9 instead of AltGr
dictr --backend api            # Use OpenAI Whisper API (requires OPENAI_API_KEY)
dictr --api-url http://...     # Custom API endpoint
dictr --model /path/to/model   # Specific model file
dictr --paste                  # Use clipboard paste (better for accents/Unicode)
dictr --device AT2020          # Select mic by name substring
dictr --list-devices           # List available input devices
dictr --language fr            # Transcribe in French
dictr --initial-prompt '...'   # Guide transcription with context
dictr --min-duration 500       # Min recording duration in ms (default: 300)
dictr --verbose                # Debug output
```

## Install

### Interactive installer

```sh
curl -fsSL https://raw.githubusercontent.com/mwmdev/dictr/main/install.sh | sh
```

### Cargo

```sh
cargo install dictr
```

Then download a [Whisper model](https://huggingface.co/ggerganov/whisper.cpp/tree/main) to `~/.local/share/dictr/models/`.

### Build from source

Requires Linux with X11, `xdotool`, `xclip`, ALSA or PipeWire, plus build deps: `cmake`, `clang`, `pkg-config`, `libasound2-dev`, `libx11-dev`, `libxi-dev`, `libxtst-dev`, `libxrandr-dev`, `libssl-dev`. For CUDA: NVIDIA CUDA toolkit.

```sh
cargo build --release                  # CPU only
cargo build --release --features cuda  # With GPU
```

On NixOS, use `nix-shell --run "cargo build --release"`

## Configuration

`~/.config/dictr/config.toml`:

```toml
hotkey = "AltGr"                 # Supported hotkeys: AltGr, Alt, Ctrl, RCtrl, Shift, RShift, Super, CapsLock, Space, Escape, F1-F12
backend = "local"                # "local" or "api"
model_path = "~/.local/share/dictr/models/ggml-base.bin"
api_key = ""                     # or set OPENAI_API_KEY env var
api_url = "https://api.openai.com/v1/audio/transcriptions"
typing_delay_ms = 2
min_duration_ms = 300
device = "AT2020USB+"
language = "en"
initial_prompt = "commit, readme, build, test, deploy, refactor" # Guide transcription with context (e.g. expected words, domain-specific terms)

[replacements]
"slash " = "/"
"new line" = "\n"
```

### Text replacements

The `[replacements]` table performs substitution on transcription output. Useful for special cases like "slash" → "/" or "new line" → "\n". Keys are replaced with their corresponding values in the final transcribed text. 

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
