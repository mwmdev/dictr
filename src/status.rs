use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

const I3BLOCKS_SIGNAL: u8 = 11;
const MAX_PATH_LEN: usize = 128;

/// Null-terminated path for async-signal-safe access in the signal handler.
/// Accessed via raw pointer to avoid `static mut` reference warnings.
static mut STATUS_PATH_BUF: [u8; MAX_PATH_LEN] = [0; MAX_PATH_LEN];
static REGISTERED: AtomicBool = AtomicBool::new(false);

fn status_path() -> String {
    "/tmp/dictr-status".to_string()
}

pub fn set(state: &str) {
    if !REGISTERED.swap(true, Ordering::Relaxed) {
        let path = status_path();
        let bytes = path.as_bytes();
        // Store null-terminated path for the signal handler
        unsafe {
            let ptr = std::ptr::addr_of_mut!(STATUS_PATH_BUF);
            let buf = &mut *ptr;
            let len = bytes.len().min(MAX_PATH_LEN - 1);
            buf[..len].copy_from_slice(&bytes[..len]);
            buf[len] = 0;
        }
        register_cleanup();
    }
    let _ = std::fs::write(status_path(), state);
    signal_i3blocks();
}

fn signal_i3blocks() {
    let _ = Command::new("pkill")
        .args([&format!("-RTMIN+{I3BLOCKS_SIGNAL}"), "i3blocks"])
        .status();
}

fn register_cleanup() {
    unsafe {
        libc::signal(
            libc::SIGINT,
            cleanup_handler as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            cleanup_handler as *const () as libc::sighandler_t,
        );
    }
}

extern "C" fn cleanup_handler(sig: libc::c_int) {
    // Only async-signal-safe operations: libc::unlink, libc::signal, libc::raise
    unsafe {
        let ptr = std::ptr::addr_of!(STATUS_PATH_BUF) as *const libc::c_char;
        libc::unlink(ptr);
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}
