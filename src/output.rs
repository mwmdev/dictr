use anyhow::{Context, Result};
use std::process::Command;

pub fn check_deps() -> Result<()> {
    Command::new("xdotool")
        .arg("--version")
        .output()
        .context("xdotool not found — install it (e.g. apt install xdotool)")?;
    Command::new("xclip")
        .arg("-version")
        .output()
        .context("xclip not found — install it (e.g. apt install xclip)")?;
    Ok(())
}

pub fn type_text(text: &str, delay_ms: u64) -> Result<()> {
    let status = Command::new("xdotool")
        .args([
            "type",
            "--clearmodifiers",
            "--delay",
            &delay_ms.to_string(),
            "--",
            text,
        ])
        .status()
        .context("failed to run xdotool")?;
    if !status.success() {
        anyhow::bail!("xdotool type failed with {status}");
    }
    Ok(())
}

pub fn paste_text(text: &str) -> Result<()> {
    // Write to both clipboard and primary so shift+Insert works everywhere
    for selection in ["clipboard", "primary"] {
        let mut child = Command::new("xclip")
            .args(["-selection", selection])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .context("failed to run xclip")?;
        if let Some(ref mut stdin) = child.stdin {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            anyhow::bail!("xclip ({selection}) failed with {status}");
        }
    }

    let status = Command::new("xdotool")
        .args(["key", "--clearmodifiers", "shift+Insert"])
        .status()
        .context("failed to run xdotool")?;
    if !status.success() {
        anyhow::bail!("xdotool key failed with {status}");
    }
    Ok(())
}
