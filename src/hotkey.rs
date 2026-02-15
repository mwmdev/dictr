use anyhow::{bail, Result};
use rdev::{listen, Event, EventType, Key};
use std::sync::mpsc;
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

struct Debouncer {
    target_key: Key,
    is_pressed: bool,
}

impl Debouncer {
    fn new(target_key: Key) -> Self {
        Self {
            target_key,
            is_pressed: false,
        }
    }

    fn handle(&mut self, event_type: EventType) -> Option<HotkeyEvent> {
        match event_type {
            EventType::KeyPress(key) if key == self.target_key => {
                if !self.is_pressed {
                    self.is_pressed = true;
                    return Some(HotkeyEvent::Pressed);
                }
            }
            EventType::KeyRelease(key) if key == self.target_key => {
                if self.is_pressed {
                    self.is_pressed = false;
                    return Some(HotkeyEvent::Released);
                }
            }
            _ => {}
        }
        None
    }
}

pub fn start_listener(
    hotkey_name: &str,
    tx: mpsc::Sender<HotkeyEvent>,
) -> Result<thread::JoinHandle<()>> {
    let target_key = parse_key(hotkey_name)?;

    let handle = thread::spawn(move || {
        let mut debouncer = Debouncer::new(target_key);

        let callback = move |event: Event| {
            if let Some(hotkey_event) = debouncer.handle(event.event_type) {
                let _ = tx.send(hotkey_event);
            }
        };

        if let Err(e) = listen(callback) {
            eprintln!("hotkey listener error: {:?}", e);
        }
    });

    Ok(handle)
}

fn parse_key(name: &str) -> Result<Key> {
    match name.to_lowercase().as_str() {
        "altgr" | "alt_gr" | "ralt" => Ok(Key::AltGr),
        "alt" | "lalt" => Ok(Key::Alt),
        "ctrl" | "lctrl" | "controlleft" => Ok(Key::ControlLeft),
        "rctrl" | "controlright" => Ok(Key::ControlRight),
        "shift" | "lshift" | "shiftleft" => Ok(Key::ShiftLeft),
        "rshift" | "shiftright" => Ok(Key::ShiftRight),
        "super" | "meta" | "metaleft" => Ok(Key::MetaLeft),
        "capslock" => Ok(Key::CapsLock),
        "f1" => Ok(Key::F1),
        "f2" => Ok(Key::F2),
        "f3" => Ok(Key::F3),
        "f4" => Ok(Key::F4),
        "f5" => Ok(Key::F5),
        "f6" => Ok(Key::F6),
        "f7" => Ok(Key::F7),
        "f8" => Ok(Key::F8),
        "f9" => Ok(Key::F9),
        "f10" => Ok(Key::F10),
        "f11" => Ok(Key::F11),
        "f12" => Ok(Key::F12),
        "space" => Ok(Key::Space),
        "escape" | "esc" => Ok(Key::Escape),
        other => bail!("unknown hotkey: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_altgr_variants() {
        assert_eq!(parse_key("AltGr").unwrap(), Key::AltGr);
        assert_eq!(parse_key("altgr").unwrap(), Key::AltGr);
        assert_eq!(parse_key("alt_gr").unwrap(), Key::AltGr);
        assert_eq!(parse_key("ralt").unwrap(), Key::AltGr);
    }

    #[test]
    fn parse_modifiers() {
        assert_eq!(parse_key("ctrl").unwrap(), Key::ControlLeft);
        assert_eq!(parse_key("rctrl").unwrap(), Key::ControlRight);
        assert_eq!(parse_key("shift").unwrap(), Key::ShiftLeft);
        assert_eq!(parse_key("super").unwrap(), Key::MetaLeft);
    }

    #[test]
    fn parse_function_keys() {
        assert_eq!(parse_key("f1").unwrap(), Key::F1);
        assert_eq!(parse_key("F9").unwrap(), Key::F9);
        assert_eq!(parse_key("F12").unwrap(), Key::F12);
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(parse_key("CAPSLOCK").unwrap(), Key::CapsLock);
        assert_eq!(parse_key("Space").unwrap(), Key::Space);
        assert_eq!(parse_key("ESCAPE").unwrap(), Key::Escape);
        assert_eq!(parse_key("esc").unwrap(), Key::Escape);
    }

    #[test]
    fn parse_unknown_key_fails() {
        assert!(parse_key("nonexistent").is_err());
        assert!(parse_key("").is_err());
    }

    #[test]
    fn debounce_repeated_press_emits_once() {
        let mut d = Debouncer::new(Key::AltGr);
        // First press emits
        assert_eq!(
            d.handle(EventType::KeyPress(Key::AltGr)),
            Some(HotkeyEvent::Pressed)
        );
        // X11 key repeat: subsequent presses without release are suppressed
        assert_eq!(d.handle(EventType::KeyPress(Key::AltGr)), None);
        assert_eq!(d.handle(EventType::KeyPress(Key::AltGr)), None);
        assert_eq!(d.handle(EventType::KeyPress(Key::AltGr)), None);
        // Release emits
        assert_eq!(
            d.handle(EventType::KeyRelease(Key::AltGr)),
            Some(HotkeyEvent::Released)
        );
    }

    #[test]
    fn debounce_release_without_press_ignored() {
        let mut d = Debouncer::new(Key::AltGr);
        assert_eq!(d.handle(EventType::KeyRelease(Key::AltGr)), None);
    }

    #[test]
    fn debounce_ignores_other_keys() {
        let mut d = Debouncer::new(Key::AltGr);
        assert_eq!(d.handle(EventType::KeyPress(Key::Space)), None);
        assert_eq!(d.handle(EventType::KeyRelease(Key::Space)), None);
    }

    #[test]
    fn debounce_press_release_press_cycle() {
        let mut d = Debouncer::new(Key::F9);
        assert_eq!(
            d.handle(EventType::KeyPress(Key::F9)),
            Some(HotkeyEvent::Pressed)
        );
        assert_eq!(
            d.handle(EventType::KeyRelease(Key::F9)),
            Some(HotkeyEvent::Released)
        );
        assert_eq!(
            d.handle(EventType::KeyPress(Key::F9)),
            Some(HotkeyEvent::Pressed)
        );
        assert_eq!(
            d.handle(EventType::KeyRelease(Key::F9)),
            Some(HotkeyEvent::Released)
        );
    }
}
