//! `hooks`: decoding of low-level hook records (pure). Real hooks are manual tests only.

use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_windows::hooks::{
    KeyboardEvent, MouseButton, MouseEvent, decode_keyboard, decode_mouse, messages,
};

#[test]
fn keyboard_records_decode_flags_and_fields() {
    let event = decode_keyboard(messages::WM_KEYDOWN, 0x41, 0x1E, 0, 1234, 0);
    assert_eq!(
        event,
        KeyboardEvent {
            vk: 0x41,
            scan: 0x1E,
            extended: false,
            injected: false,
            injected_lower_il: false,
            alt_down: false,
            key_up: false,
            time: 1234,
            extra_info: 0,
        }
    );
    let up = decode_keyboard(messages::WM_KEYUP, 0x41, 0x1E, 0x80, 1, 0);
    assert!(up.key_up);
    let extended = decode_keyboard(messages::WM_KEYDOWN, 0x25, 0x4B, 0x01, 1, 0);
    assert!(extended.extended);
    let injected = decode_keyboard(messages::WM_KEYDOWN, 0x41, 0, 0x10, 1, 0);
    assert!(injected.injected && !injected.injected_lower_il);
    let lower = decode_keyboard(messages::WM_KEYDOWN, 0x41, 0, 0x12, 1, 0);
    assert!(lower.injected && lower.injected_lower_il);
    let sys = decode_keyboard(messages::WM_SYSKEYUP, 0x5A, 0x2C, 0xA0, 1, 0);
    assert!(sys.key_up && sys.alt_down);
    // The message decides key-up when the flag is missing (defensive).
    let by_message = decode_keyboard(messages::WM_SYSKEYUP, 0x5A, 0x2C, 0, 1, 0);
    assert!(by_message.key_up);
}

#[test]
fn raumakey_injected_keys_are_recognized_by_their_extra_info() {
    let ours = decode_keyboard(
        messages::WM_KEYDOWN,
        0x08,
        0x0E,
        0x10,
        1,
        RAUMAKEY_INJECTED_TAG as usize,
    );
    assert!(ours.is_raumakey_injected());
    let foreign = decode_keyboard(messages::WM_KEYDOWN, 0x08, 0x0E, 0x10, 1, 7);
    assert!(!foreign.is_raumakey_injected());
}

#[test]
fn only_button_presses_produce_mouse_events() {
    let left = decode_mouse(messages::WM_LBUTTONDOWN, 0, 0, 5, 0).unwrap();
    assert_eq!(
        left,
        MouseEvent {
            button: MouseButton::Left,
            injected: false,
            injected_lower_il: false,
            time: 5,
            extra_info: 0,
        }
    );
    assert_eq!(
        decode_mouse(messages::WM_RBUTTONDOWN, 0, 0, 0, 0)
            .unwrap()
            .button,
        MouseButton::Right
    );
    assert_eq!(
        decode_mouse(messages::WM_MBUTTONDOWN, 0, 0, 0, 0)
            .unwrap()
            .button,
        MouseButton::Middle
    );
    assert_eq!(
        decode_mouse(messages::WM_XBUTTONDOWN, 0x0001_0000, 0, 0, 0)
            .unwrap()
            .button,
        MouseButton::X1
    );
    assert_eq!(
        decode_mouse(messages::WM_XBUTTONDOWN, 0x0002_0000, 0, 0, 0)
            .unwrap()
            .button,
        MouseButton::X2
    );
    let injected = decode_mouse(messages::WM_LBUTTONDOWN, 0, 0x03, 0, 9).unwrap();
    assert!(injected.injected && injected.injected_lower_il);
    assert_eq!(injected.extra_info, 9);
    for ignored in [
        messages::WM_MOUSEMOVE,
        messages::WM_LBUTTONUP,
        messages::WM_RBUTTONUP,
        messages::WM_MOUSEWHEEL,
        messages::WM_MOUSEHWHEEL,
        0x9999,
    ] {
        assert_eq!(decode_mouse(ignored, 0, 0, 0, 0), None, "{ignored:#x}");
    }
    assert_eq!(
        decode_mouse(messages::WM_XBUTTONDOWN, 0x0003_0000, 0, 0, 0),
        None
    );
}

mod dispatch {
    use raumakey_windows::hooks::{
        HookDecision, HookHandler, KeyboardEvent, MouseEvent, decode_keyboard, decode_mouse,
        dispatch_key, dispatch_mouse, messages,
    };

    #[derive(Default)]
    struct Recorder {
        keys: usize,
        clicks: usize,
        swallow: bool,
        panic: bool,
    }

    impl HookHandler for Recorder {
        fn on_key(&mut self, _event: &KeyboardEvent) -> HookDecision {
            self.keys += 1;
            assert!(!self.panic, "handler bug");
            if self.swallow {
                HookDecision::Swallow
            } else {
                HookDecision::PassThrough
            }
        }

        fn on_mouse(&mut self, _event: &MouseEvent) {
            self.clicks += 1;
            assert!(!self.panic, "handler bug");
        }
    }

    #[test]
    fn handler_decisions_are_forwarded() {
        let key = decode_keyboard(messages::WM_KEYDOWN, 0x41, 0x1E, 0, 0, 0);
        let mut handler = Recorder {
            swallow: true,
            ..Recorder::default()
        };
        assert_eq!(dispatch_key(&mut handler, &key), HookDecision::Swallow);
        handler.swallow = false;
        assert_eq!(dispatch_key(&mut handler, &key), HookDecision::PassThrough);
        assert_eq!(handler.keys, 2);
        let click = decode_mouse(messages::WM_LBUTTONDOWN, 0, 0, 0, 0).unwrap();
        dispatch_mouse(&mut handler, &click);
        assert_eq!(handler.clicks, 1);
    }

    #[test]
    fn a_panicking_handler_passes_the_key_through() {
        // The default hook's message is captured by the test harness.
        let key = decode_keyboard(messages::WM_KEYDOWN, 0x41, 0x1E, 0, 0, 0);
        let mut handler = Recorder {
            swallow: true,
            panic: true,
            ..Recorder::default()
        };
        assert_eq!(dispatch_key(&mut handler, &key), HookDecision::PassThrough);
        let click = decode_mouse(messages::WM_LBUTTONDOWN, 0, 0, 0, 0).unwrap();
        dispatch_mouse(&mut handler, &click);
        assert_eq!(handler.keys, 1);
    }
}

#[test]
fn the_heartbeat_keeps_the_tick_of_the_last_event() {
    use raumakey_windows::hooks::Heartbeat;

    let heartbeat = Heartbeat::default();
    assert_eq!(heartbeat.last(), None, "no event yet");
    heartbeat.beat(0);
    assert_eq!(heartbeat.last(), Some(0), "tick 0 is a tick like any other");
    heartbeat.beat(1234);
    assert_eq!(heartbeat.last(), Some(1234));
    heartbeat.beat(u32::MAX);
    assert_eq!(heartbeat.last(), Some(u32::MAX));
    let _ = format!("{heartbeat:?}");
}

#[cfg(windows)]
mod manual {
    use std::time::Duration;

    use raumakey_windows::hooks::{
        HookDecision, HookHandler, HookThread, KeyboardEvent, MouseEvent,
    };

    struct PassAll;

    impl HookHandler for PassAll {
        fn on_key(&mut self, _event: &KeyboardEvent) -> HookDecision {
            HookDecision::PassThrough
        }

        fn on_mouse(&mut self, _event: &MouseEvent) {}
    }

    /// Installs real global hooks for a moment. Manual test only.
    #[test]
    #[ignore = "manual: installs global low-level keyboard and mouse hooks"]
    fn manual_hook_thread_installs_and_stops() {
        let hooks = HookThread::spawn(Box::new(PassAll)).unwrap();
        // Move the mouse meanwhile: every event of either hook beats, moves included.
        std::thread::sleep(Duration::from_millis(2000));
        let _ = hooks.heartbeat().last();
        assert!(hooks.stop().is_ok());
    }
}
