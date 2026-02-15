use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

const STATUS_PATH: &str = "/tmp/dictr-status";
const I3BLOCKS_SIGNAL: u8 = 11;

static REGISTERED: AtomicBool = AtomicBool::new(false);

pub fn set(state: &str) {
    if !REGISTERED.swap(true, Ordering::Relaxed) {
        register_cleanup();
    }
    let _ = fs::write(STATUS_PATH, state);
    signal_i3blocks();
}

fn signal_i3blocks() {
    let _ = Command::new("pkill")
        .args([&format!("-RTMIN+{I3BLOCKS_SIGNAL}"), "i3blocks"])
        .status();
}

fn register_cleanup() {
    unsafe {
        libc::signal(libc::SIGINT, cleanup_handler as libc::sighandler_t);
        libc::signal(libc::SIGTERM, cleanup_handler as libc::sighandler_t);
    }
}

extern "C" fn cleanup_handler(_sig: libc::c_int) {
    let _ = std::fs::remove_file(STATUS_PATH);
    // Re-raise to get default behavior (exit)
    unsafe {
        libc::signal(_sig, libc::SIG_DFL);
        libc::raise(_sig);
    }
}
