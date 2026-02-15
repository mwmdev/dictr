#!/usr/bin/env sh
# shellcheck disable=SC2059,SC2088
set -eu

# --- Colors (only when stdout is a tty) ---
if [ -t 1 ]; then
    BOLD='\033[1m'    DIM='\033[2m'
    GREEN='\033[32m'  YELLOW='\033[33m'
    RED='\033[31m'    CYAN='\033[36m'
    RESET='\033[0m'
else
    BOLD='' DIM='' GREEN='' YELLOW='' RED='' CYAN='' RESET=''
fi

info()  { printf "${GREEN}>${RESET} %s\n" "$*"; }
warn()  { printf "${YELLOW}!${RESET} %s\n" "$*"; }
error() { printf "${RED}x${RESET} %s\n" "$*" >&2; }

to_lower() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }

expand_tilde() {
    case "$1" in
        '~'/*) printf '%s' "$HOME/${1#\~/}" ;;
        '~')   printf '%s' "$HOME" ;;
        *)     printf '%s' "$1" ;;
    esac
}

collapse_tilde() {
    case "$1" in
        "$HOME"/*) printf '~/%s' "${1#"$HOME"/}" ;;
        "$HOME")   printf '~' ;;
        *)         printf '%s' "$1" ;;
    esac
}

ask() {
    _ask_prompt="$1" _ask_default="${2-}"
    if [ -n "$_ask_default" ]; then
        printf "${CYAN}%s${RESET} [${BOLD}%s${RESET}]: " "$_ask_prompt" "$_ask_default" >&2
    else
        printf "${CYAN}%s${RESET}: " "$_ask_prompt" >&2
    fi
    read -r _ask_input
    printf '%s' "${_ask_input:-$_ask_default}"
}

ask_yn() {
    _yn_prompt="$1" _yn_default="$2"
    if [ "$_yn_default" = "y" ]; then _yn_hint="Y/n"; else _yn_hint="y/N"; fi
    printf "${CYAN}%s${RESET} [${BOLD}%s${RESET}]: " "$_yn_prompt" "$_yn_hint" >&2
    read -r _yn_input
    _yn_input="${_yn_input:-$_yn_default}"
    case "$(to_lower "$_yn_input")" in
        y*) return 0 ;;
        *)  return 1 ;;
    esac
}

GITHUB_REPO="mwmdev/dictr"
HF_BASE="https://huggingface.co/ggerganov/whisper.cpp/resolve/main"
VALID_HOTKEYS="AltGr Alt Ctrl RCtrl Shift RShift Super CapsLock Space Escape F1 F2 F3 F4 F5 F6 F7 F8 F9 F10 F11 F12"

# ── 1. Welcome ───────────────────────────────────────────────────────────────
echo ""
printf "${BOLD}dictr${RESET} — push-to-talk voice dictation for Linux\n"
echo ""

# ── 2. Check deps ────────────────────────────────────────────────────────────
if ! command -v curl >/dev/null 2>&1; then
    error "curl is required but not found. Please install it and re-run."
    exit 1
fi

for dep in xdotool xclip; do
    if ! command -v "$dep" >/dev/null 2>&1; then
        warn "$dep not found — dictr needs it at runtime"
    fi
done

HAS_PACTL=false
if command -v pactl >/dev/null 2>&1; then
    HAS_PACTL=true
else
    info "pactl not found — skipping microphone detection"
fi

# ── 3. Detect GPU ────────────────────────────────────────────────────────────
HAS_GPU=false
if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi >/dev/null 2>&1; then
    HAS_GPU=true
elif command -v lspci >/dev/null 2>&1 && lspci 2>/dev/null | grep -qi nvidia; then
    HAS_GPU=true
fi

# ── 4. Choose release ────────────────────────────────────────────────────────
if $HAS_GPU; then
    default_variant="cuda"
    info "NVIDIA GPU detected"
else
    default_variant="cpu"
fi
variant=$(ask "Release variant (cpu/cuda)" "$default_variant")
variant=$(to_lower "$variant")
case "$variant" in
    cpu|cuda) ;;
    *) warn "Unknown variant '$variant', defaulting to cpu"; variant="cpu" ;;
esac

if [ "$variant" = "cuda" ]; then
    binary_url="https://github.com/$GITHUB_REPO/releases/latest/download/dictr-x86_64-linux-cuda"
else
    binary_url="https://github.com/$GITHUB_REPO/releases/latest/download/dictr-x86_64-linux"
fi

# ── 5. Choose install path ───────────────────────────────────────────────────
install_path=$(ask "Install path" "$HOME/.local/bin/dictr")
install_path=$(expand_tilde "$install_path")
install_dir="$(dirname "$install_path")"

case ":$PATH:" in
    *":$install_dir:"*) ;;
    *) warn "$install_dir is not in \$PATH — you may need to add it" ;;
esac

NEED_SUDO=false
if [ -d "$install_dir" ] && [ ! -w "$install_dir" ]; then
    NEED_SUDO=true
    info "sudo required to write to $install_dir"
elif [ ! -d "$install_dir" ]; then
    parent="$install_dir"
    while [ ! -d "$parent" ]; do parent="$(dirname "$parent")"; done
    if [ ! -w "$parent" ]; then
        NEED_SUDO=true
        info "sudo required to create $install_dir"
    fi
fi

# ── 6. Download binary ───────────────────────────────────────────────────────
download_binary=true
if [ -f "$install_path" ]; then
    current_version=$("$install_path" --version 2>/dev/null || echo "unknown")
    info "Existing binary: $current_version"
    if ! ask_yn "Replace it?" "y"; then
        download_binary=false
    fi
fi

if $download_binary; then
    info "Downloading dictr ($variant)..."
    tmpfile=$(mktemp)
    trap 'rm -f "$tmpfile"' EXIT
    if ! curl -fSL --progress-bar -o "$tmpfile" "$binary_url"; then
        error "Download failed"
        exit 1
    fi
    chmod +x "$tmpfile"
    if $NEED_SUDO; then
        sudo mkdir -p "$install_dir"
        sudo mv "$tmpfile" "$install_path"
    else
        mkdir -p "$install_dir"
        mv "$tmpfile" "$install_path"
    fi
    info "Installed to $install_path"
fi

# ── 7. Choose model ──────────────────────────────────────────────────────────
echo ""
printf "  ${DIM}%-8s %8s  %s${RESET}\n" "tiny" "75 MB" "Fastest, lower accuracy"
printf "  ${DIM}%-8s %8s  %s${RESET}\n" "base" "142 MB" "Good balance (default)"
printf "  ${DIM}%-8s %8s  %s${RESET}\n" "small" "466 MB" "Better accuracy, slower"
model_choice=$(ask "Model" "base")
model_choice=$(to_lower "$model_choice")
case "$model_choice" in
    tiny|base|small) ;;
    *) warn "Unknown model '$model_choice', defaulting to base"; model_choice="base" ;;
esac
model_file="ggml-${model_choice}.bin"

# ── 8. Choose model path ─────────────────────────────────────────────────────
model_dir=$(ask "Model directory" "~/.local/share/dictr/models")
model_dir_expanded=$(expand_tilde "$model_dir")
model_path_full="$model_dir_expanded/$model_file"
model_path_config="$(collapse_tilde "$model_dir_expanded")/$model_file"

# ── 9. Download model ────────────────────────────────────────────────────────
download_model=true
if [ -f "$model_path_full" ]; then
    size=$(du -h "$model_path_full" | cut -f1)
    info "Model already exists: $model_path_full ($size)"
    if ! ask_yn "Re-download?" "n"; then
        download_model=false
    fi
fi

if $download_model; then
    mkdir -p "$model_dir_expanded"
    info "Downloading $model_file..."
    if ! curl -fSL --progress-bar -o "$model_path_full" "$HF_BASE/$model_file"; then
        error "Model download failed"
        exit 1
    fi
    info "Model saved to $model_path_full"
fi

# ── 10. Choose hotkey ─────────────────────────────────────────────────────────
echo ""
info "Valid hotkeys: $VALID_HOTKEYS"
hotkey=$(ask "Hotkey" "AltGr")
hotkey_lower=$(to_lower "$hotkey")
valid=false
for k in $VALID_HOTKEYS; do
    if [ "$(to_lower "$k")" = "$hotkey_lower" ]; then
        hotkey="$k"
        valid=true
        break
    fi
done
if ! $valid; then
    warn "Unknown hotkey '$hotkey', using AltGr"
    hotkey="AltGr"
fi

# ── 11. Detect mics ──────────────────────────────────────────────────────────
device=""
if $HAS_PACTL; then
    mic_names="" mic_descs="" mic_count=0
    current_name=""
    while IFS= read -r line; do
        trimmed=$(printf '%s' "$line" | sed 's/^[[:space:]]*//')
        case "$trimmed" in
            "Name: "*)
                current_name="${trimmed#Name: }"
                ;;
            "Description: "*)
                if [ -n "$current_name" ]; then
                    case "$current_name" in
                        *.monitor) ;;
                        *)
                            mic_count=$((mic_count + 1))
                            mic_names="$mic_names$current_name
"
                            mic_descs="$mic_descs${trimmed#Description: }
"
                            ;;
                    esac
                    current_name=""
                fi
                ;;
        esac
    done <<PACTL_EOF
$(pactl list sources 2>/dev/null)
PACTL_EOF

    if [ "$mic_count" -gt 0 ]; then
        echo ""
        info "Detected microphones:"
        i=1
        while [ "$i" -le "$mic_count" ]; do
            desc=$(printf '%s' "$mic_descs" | sed -n "${i}p")
            name=$(printf '%s' "$mic_names" | sed -n "${i}p")
            printf "  ${DIM}%d)${RESET} %s\n" "$i" "$desc"
            printf "     ${DIM}%s${RESET}\n" "$name"
            i=$((i + 1))
        done
        echo ""
        pick=$(ask "Microphone (number, name, or blank to skip)" "")
        if [ -n "$pick" ]; then
            case "$pick" in
                *[!0-9]*|'')
                    device="$pick"
                    ;;
                *)
                    if [ "$pick" -ge 1 ] && [ "$pick" -le "$mic_count" ]; then
                        device=$(printf '%s' "$mic_descs" | sed -n "${pick}p")
                    else
                        device="$pick"
                    fi
                    ;;
            esac
        fi
    fi
fi

# ── 12. Choose language ──────────────────────────────────────────────────────
echo ""
language=$(ask "Language code (blank for auto-detect)" "en")

# ── 13. Generate config ──────────────────────────────────────────────────────
echo ""
config_dir="$HOME/.config/dictr"
config_file="$config_dir/config.toml"

if [ -f "$config_file" ]; then
    cp "$config_file" "${config_file}.bak"
    info "Existing config backed up to ${config_file}.bak"
fi

mkdir -p "$config_dir"

# Only write non-default values
{
    [ "$hotkey" != "AltGr" ] && printf 'hotkey = "%s"\n' "$hotkey"
    [ "$model_path_config" != "~/.local/share/dictr/models/ggml-base.bin" ] && \
        printf 'model_path = "%s"\n' "$model_path_config"
    [ -n "$device" ] && printf 'device = "%s"\n' "$device"
    [ -n "$language" ] && printf 'language = "%s"\n' "$language"
    true
} > "$config_file"

info "Config written to $config_file"

# ── 14. Systemd service ──────────────────────────────────────────────────────
echo ""
if ask_yn "Install systemd user service?" "n"; then
    service_dir="$HOME/.config/systemd/user"
    service_file="$service_dir/dictr.service"
    mkdir -p "$service_dir"
    cat > "$service_file" <<EOF
[Unit]
Description=dictr push-to-talk voice dictation
After=graphical-session.target

[Service]
Type=simple
ExecStart=$install_path --paste
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
EOF
    info "Service written to $service_file"

    if ask_yn "Enable and start now?" "n"; then
        systemctl --user daemon-reload
        systemctl --user enable --now dictr.service
        info "Service enabled and started"
    fi
fi

# ── 15. Summary ───────────────────────────────────────────────────────────────
echo ""
printf "${BOLD}Done!${RESET}\n"
echo ""
printf "  Binary:  %s\n" "$install_path"
printf "  Model:   %s\n" "$model_path_full"
printf "  Config:  %s\n" "$config_file"
echo ""
printf "  Run: ${BOLD}dictr${RESET}\n"
printf "  Or:  ${BOLD}dictr --verbose${RESET} to verify setup\n"
echo ""
