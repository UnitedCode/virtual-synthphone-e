//! Receive K2's versioned MIDI bridge. The peer cannot power on, capture a mic,
//! change files, or invoke arbitrary firmware-worker commands.
#[cfg(any(test, target_arch = "wasm32"))]
fn allowed_origin(origin: &str) -> bool {
    matches!(
        origin,
        "https://nathansbradshaw.github.io" | "http://127.0.0.1:8080" | "http://localhost:8080"
    )
}
#[cfg(any(test, target_arch = "wasm32"))]
fn valid_session(session: &str) -> bool {
    session.len() == 32 && session.bytes().all(|c| c.is_ascii_hexdigit())
}
#[cfg(any(test, target_arch = "wasm32"))]
fn valid_midi(bytes: &[u8]) -> bool {
    let Some(&status) = bytes.first() else {
        return false;
    };
    let length = match status & 0xf0 {
        0x80..=0xb0 | 0xe0 => 3,
        0xc0 | 0xd0 => 2,
        _ => return false,
    };
    bytes.len() == length && bytes[1..].iter().all(|&byte| byte < 128)
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use js_sys::Uint8Array;
    use std::cell::RefCell;
    use wasm_bindgen::{closure::Closure, JsCast, JsValue};
    use web_sys::{MessageEvent, Window};

    const TYPE: &str = "k2-synthphone";
    const VERSION: f64 = 1.0;
    const TIMEOUT_MS: f64 = 3500.0;

    struct Peer {
        window: Window,
        origin: String,
        session: String,
        last_seen: f64,
    }
    thread_local! { static PEER: RefCell<Option<Peer>> = const { RefCell::new(None) }; }

    fn powered() -> bool {
        crate::APP.with_borrow(|app| app.is_some())
            && crate::el("power").get_attribute("data-on").as_deref() == Some("true")
    }

    fn panic_notes() {
        // CC120 is important for one-shot drums and sustained voices too.
        for channel in 0..16 {
            for controller in [120, 123] {
                let message = crate::object("midi");
                crate::set(
                    &message,
                    "bytes",
                    Uint8Array::from([0xb0 | channel, controller, 0].as_slice()),
                );
                crate::send(&message);
            }
        }
    }

    fn related_window(source: &JsValue) -> bool {
        let window = web_sys::window().unwrap();
        window
            .parent()
            .ok()
            .flatten()
            .is_some_and(|parent| parent != window && AsRef::<JsValue>::as_ref(&parent) == source)
            || window
                .opener()
                .ok()
                .is_some_and(|opener| !opener.is_null() && opener == *source)
    }

    pub fn install() {
        let window = web_sys::window().unwrap();
        let receive = Closure::wrap(Box::new(move |event: MessageEvent| {
            if !allowed_origin(&event.origin()) {
                return;
            }
            let Some(source) = event.source() else {
                return;
            };
            if !related_window(source.as_ref()) {
                return;
            }
            let data = event.data();
            if crate::get(&data, "type").as_string().as_deref() != Some(TYPE)
                || crate::get(&data, "version").as_f64() != Some(VERSION)
            {
                return;
            }
            let Some(session) = crate::get(&data, "session")
                .as_string()
                .filter(|s| valid_session(s))
            else {
                return;
            };
            let Some(kind) = crate::get(&data, "kind").as_string() else {
                return;
            };
            if kind == "connect" {
                let changed = PEER.with_borrow(|peer| {
                    peer.as_ref().is_some_and(|peer| {
                        peer.origin != event.origin()
                            || peer.session != session
                            || AsRef::<JsValue>::as_ref(&peer.window)
                                != AsRef::<JsValue>::as_ref(&source)
                    })
                });
                if changed {
                    panic_notes();
                }
                PEER.with_borrow_mut(|peer| {
                    *peer = Some(Peer {
                        window: source.clone().unchecked_into(),
                        origin: event.origin(),
                        session: session.clone(),
                        last_seen: js_sys::Date::now(),
                    })
                });
                // Local power/MIC remain explicit user gestures. Off phones
                // do not acknowledge readiness or retain incoming notes.
                if powered() {
                    let reply = crate::object("ready");
                    crate::set(&reply, "type", TYPE);
                    crate::set(&reply, "version", VERSION);
                    crate::set(&reply, "session", session);
                    let target: Window = source.unchecked_into();
                    let _ = target.post_message(&reply, &event.origin());
                }
                return;
            }
            let authorized = PEER.with_borrow(|peer| {
                peer.as_ref().is_some_and(|peer| {
                    peer.origin == event.origin()
                        && peer.session == session
                        && AsRef::<JsValue>::as_ref(&peer.window)
                            == AsRef::<JsValue>::as_ref(&source)
                        && js_sys::Date::now() - peer.last_seen < TIMEOUT_MS
                })
            });
            if !authorized {
                return;
            }
            if kind == "disconnect" {
                panic_notes();
                PEER.with_borrow_mut(|peer| *peer = None);
            } else if kind == "midi" && powered() {
                let bytes = crate::get(&data, "bytes");
                if !bytes.is_instance_of::<Uint8Array>() {
                    return;
                }
                let bytes = Uint8Array::new(&bytes);
                if bytes.length() > 3 || !valid_midi(&bytes.to_vec()) {
                    return;
                }
                let message = crate::object("midi");
                crate::set(&message, "bytes", bytes.clone());
                crate::send(&message);
                // Browser-visible diagnostics for checking the real iframe
                // route without granting debugger access to the audio engine.
                if let Some(body) = crate::doc().body() {
                    let count = body
                        .get_attribute("data-k2-midi-count")
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(0)
                        .saturating_add(1);
                    let _ = body.set_attribute("data-k2-midi-count", &count.to_string());
                    let _ = body
                        .set_attribute("data-k2-last-midi", &format!("{:02x?}", bytes.to_vec()));
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        let _ =
            window.add_event_listener_with_callback("message", receive.as_ref().unchecked_ref());
        receive.forget();
        let timeout = Closure::wrap(Box::new(move || {
            let expired = PEER.with_borrow(|peer| {
                peer.as_ref()
                    .is_some_and(|peer| js_sys::Date::now() - peer.last_seen >= TIMEOUT_MS)
            });
            if expired {
                panic_notes();
                PEER.with_borrow_mut(|peer| *peer = None);
            }
        }) as Box<dyn FnMut()>);
        let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(
            timeout.as_ref().unchecked_ref(),
            1000,
        );
        timeout.forget();
    }
}
#[cfg(target_arch = "wasm32")]
pub use browser::install;
#[cfg(not(target_arch = "wasm32"))]
pub fn install() {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_production_k2_origin_and_explicit_local_preview_are_trusted() {
        assert!(allowed_origin("https://nathansbradshaw.github.io"));
        assert!(allowed_origin("http://127.0.0.1:8080"));
        for origin in [
            "null",
            "http://nathansbradshaw.github.io",
            "https://nathansbradshaw.github.io.attacker.test",
            "http://localhost:3030",
            "https://attacker.test",
        ] {
            assert!(!allowed_origin(origin));
        }
    }
    #[test]
    fn rejects_truncated_system_messages_and_invalid_values() {
        for bytes in [
            &[][..],
            &[60, 100],
            &[0x90, 60],
            &[0xc0, 1, 0],
            &[0x90, 60, 128],
            &[0xf0, 1, 0xf7],
            &[0xf8],
        ] {
            assert!(!valid_midi(bytes));
        }
        for channel in 0..16 {
            assert!(valid_midi(&[0x90 | channel, 60, 127]));
            assert!(valid_midi(&[0xb0 | channel, 120, 0]));
            assert!(valid_midi(&[0xc0 | channel, 2]));
        }
    }
    #[test]
    fn sessions_have_a_fixed_validated_shape() {
        assert!(valid_session("0123456789abcdef0123456789abcdef"));
        assert!(!valid_session(""));
        assert!(!valid_session("0123456789abcdef0123456789abcdeg"));
    }
}
