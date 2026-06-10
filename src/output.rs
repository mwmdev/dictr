use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::OutputMode;

const SELECTIONS: [&str; 2] = ["clipboard", "primary"];
const PASTE_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const PASTE_REQUEST_POLL: Duration = Duration::from_millis(10);

struct SelectionOwner {
    selection: &'static str,
    child: Child,
    done: bool,
}

pub fn check_deps(output_mode: OutputMode) -> Result<()> {
    let out = Command::new("xdotool")
        .arg("--version")
        .output()
        .context("xdotool not found — install it")?;
    if !out.status.success() {
        anyhow::bail!("xdotool check failed");
    }
    if output_mode == OutputMode::Paste {
        let out = Command::new("xclip")
            .arg("-version")
            .output()
            .context("xclip not found — install it")?;
        if !out.status.success() {
            anyhow::bail!("xclip check failed");
        }
    }
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
    let saved = save_selections();
    let mut owners = match start_selection_owners(text) {
        Ok(owners) => owners,
        Err(err) => {
            restore_selections(&saved)?;
            return Err(err);
        }
    };

    let paste_result = paste_with_saved_clipboard(&mut owners);
    let restore_result = restore_selections(&saved);
    cleanup_selection_owners(&mut owners);

    match (paste_result, restore_result) {
        (Err(err), _) => Err(err),
        (Ok(()), Err(err)) => Err(err),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn paste_with_saved_clipboard(owners: &mut [SelectionOwner]) -> Result<()> {
    let status = Command::new("xdotool")
        .args(["key", "--clearmodifiers", "shift+Insert"])
        .status()
        .context("failed to run xdotool")?;
    if !status.success() {
        anyhow::bail!("xdotool key failed with {status}");
    }
    wait_for_selection_request(owners)
}

fn start_selection_owners(text: &str) -> Result<Vec<SelectionOwner>> {
    let mut owners = Vec::new();
    for selection in SELECTIONS {
        match start_selection_owner(selection, text.as_bytes()) {
            Ok(owner) => owners.push(owner),
            Err(err) => {
                cleanup_selection_owners(&mut owners);
                return Err(err);
            }
        }
    }
    Ok(owners)
}

fn start_selection_owner(selection: &'static str, contents: &[u8]) -> Result<SelectionOwner> {
    let mut child = Command::new("xclip")
        .args(["-quiet", "-loops", "1", "-selection", selection])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to run xclip")?;

    let write_result = child
        .stdin
        .as_mut()
        .context("failed to open xclip stdin")
        .and_then(|stdin| {
            stdin
                .write_all(contents)
                .context("failed to write text to xclip")
        });
    if let Err(err) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(err);
    }
    drop(child.stdin.take());

    Ok(SelectionOwner {
        selection,
        child,
        done: false,
    })
}

fn wait_for_selection_request(owners: &mut [SelectionOwner]) -> Result<()> {
    let started = Instant::now();
    let mut failures = Vec::new();

    loop {
        for owner in owners.iter_mut().filter(|owner| !owner.done) {
            if let Some(status) = owner
                .child
                .try_wait()
                .with_context(|| format!("failed to wait for xclip ({})", owner.selection))?
            {
                owner.done = true;
                if status.success() {
                    return Ok(());
                }
                failures.push(format!("xclip ({}) exited with {status}", owner.selection));
            }
        }

        if owners.iter().all(|owner| owner.done) {
            if failures.is_empty() {
                anyhow::bail!("paste failed before any X selection was requested");
            }
            anyhow::bail!(
                "paste failed before any X selection was requested: {}",
                failures.join("; ")
            );
        }

        if started.elapsed() >= PASTE_REQUEST_TIMEOUT {
            anyhow::bail!(
                "timed out waiting for pasted text to be requested after {:.1}s",
                PASTE_REQUEST_TIMEOUT.as_secs_f32()
            );
        }

        thread::sleep(PASTE_REQUEST_POLL);
    }
}

fn cleanup_selection_owners(owners: &mut [SelectionOwner]) {
    for owner in owners.iter_mut().filter(|owner| !owner.done) {
        match owner.child.try_wait() {
            Ok(Some(_)) => {
                owner.done = true;
            }
            Ok(None) => {
                let _ = owner.child.kill();
                let _ = owner.child.wait();
                owner.done = true;
            }
            Err(_) => {
                let _ = owner.child.kill();
                let _ = owner.child.wait();
                owner.done = true;
            }
        }
    }
}

fn save_selections() -> Vec<(&'static str, Option<Vec<u8>>)> {
    SELECTIONS
        .into_iter()
        .map(|selection| (selection, read_selection(selection).ok().flatten()))
        .collect()
}

fn restore_selections(saved: &[(&str, Option<Vec<u8>>)]) -> Result<()> {
    for (selection, contents) in saved {
        write_selection(selection, contents.as_deref().unwrap_or_default())?;
    }
    Ok(())
}

fn read_selection(selection: &str) -> Result<Option<Vec<u8>>> {
    let output = Command::new("xclip")
        .args(["-selection", selection, "-o"])
        .output()
        .context("failed to run xclip")?;
    if output.status.success() {
        Ok(Some(output.stdout))
    } else {
        Ok(None)
    }
}

fn write_selection(selection: &str, contents: &[u8]) -> Result<()> {
    let mut child = Command::new("xclip")
        .args(["-selection", selection])
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to run xclip")?;
    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(contents)?;
    }
    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("xclip ({selection}) failed with {status}");
    }
    Ok(())
}
