//! `input`: synthetic input builders (pure) and, on Windows, the extra information of the
//! current message (read on fresh threads, set only with `SetMessageExtraInfo` on that thread).
//! Nothing here sends input outside the manual test.

use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::{ModifierSet, ReplayKey, vk};
use raumakey_windows::input::{
    InputError, MAX_BACKSPACES, MAX_TEXT_UNITS, SyntheticKey, is_extended_key,
    is_raumakey_extra_info, keyboard_inputs_for_backspaces, replay_inputs, unicode_text_inputs,
};

const TAG: usize = RAUMAKEY_INJECTED_TAG as usize;

fn down(virtual_key: u16, scan_code: u16) -> SyntheticKey {
    SyntheticKey {
        virtual_key,
        scan_code,
        key_up: false,
        unicode: false,
        extended: false,
        extra_info: TAG,
    }
}

fn up(virtual_key: u16, scan_code: u16) -> SyntheticKey {
    SyntheticKey {
        key_up: true,
        ..down(virtual_key, scan_code)
    }
}

#[test]
fn backspaces_are_down_up_pairs_carrying_the_tag() {
    let events = keyboard_inputs_for_backspaces(3).unwrap();
    assert_eq!(events.len(), 6);
    for pair in events.chunks(2) {
        assert_eq!(pair[0], down(0x08, 0x0E));
        assert_eq!(pair[1], up(0x08, 0x0E));
    }
    assert!(keyboard_inputs_for_backspaces(0).unwrap().is_empty());
    assert_eq!(
        keyboard_inputs_for_backspaces(MAX_BACKSPACES + 1),
        Err(InputError::TooManyEvents)
    );
    assert_eq!(
        keyboard_inputs_for_backspaces(MAX_BACKSPACES)
            .unwrap()
            .len(),
        2 * MAX_BACKSPACES as usize
    );
}

#[test]
fn unicode_text_sends_each_code_unit_including_both_surrogates() {
    let units: Vec<u16> = "ế😀".encode_utf16().collect();
    assert_eq!(units, [0x1EBF, 0xD83D, 0xDE00]);
    let events = unicode_text_inputs(&units).unwrap();
    assert_eq!(events.len(), 6);
    for (pair, unit) in events.chunks(2).zip(&units) {
        let key_down = SyntheticKey {
            virtual_key: 0,
            scan_code: *unit,
            key_up: false,
            unicode: true,
            extended: false,
            extra_info: TAG,
        };
        assert_eq!(pair[0], key_down);
        assert_eq!(
            pair[1],
            SyntheticKey {
                key_up: true,
                ..key_down
            }
        );
    }
    assert!(unicode_text_inputs(&[]).unwrap().is_empty());
}

#[test]
fn ill_formed_or_oversized_text_is_refused() {
    assert_eq!(
        unicode_text_inputs(&[0x61, 0xD800]),
        Err(InputError::IllFormedText)
    );
    assert_eq!(
        unicode_text_inputs(&[0xDC00, 0x61]),
        Err(InputError::IllFormedText)
    );
    let long = vec![0x61_u16; MAX_TEXT_UNITS + 1];
    assert_eq!(unicode_text_inputs(&long), Err(InputError::TooManyEvents));
}

#[test]
fn replay_presses_modifiers_around_the_key_in_nesting_order() {
    let plain = ReplayKey {
        virtual_key: vk::RETURN,
        scan_code: Some(0x1C),
        modifiers: ModifierSet::default(),
    };
    assert_eq!(
        replay_inputs(&plain).unwrap(),
        [down(0x0D, 0x1C), up(0x0D, 0x1C)]
    );

    let shortcut = ReplayKey {
        virtual_key: vk::TAB,
        scan_code: None,
        modifiers: ModifierSet {
            shift: true,
            ctrl: true,
            alt: false,
            win: false,
        },
    };
    assert_eq!(
        replay_inputs(&shortcut).unwrap(),
        [
            down(0x11, 0x1D),
            down(0x10, 0x2A),
            down(0x09, 0),
            up(0x09, 0),
            up(0x10, 0x2A),
            up(0x11, 0x1D),
        ]
    );

    let all = ReplayKey {
        virtual_key: vk::SPACE,
        scan_code: Some(0x39),
        modifiers: ModifierSet {
            shift: true,
            ctrl: true,
            alt: true,
            win: true,
        },
    };
    let events = replay_inputs(&all).unwrap();
    assert_eq!(events.len(), 10);
    let win_down = SyntheticKey {
        extended: true,
        ..down(0x5B, 0x5B)
    };
    assert_eq!(events[3], win_down);
    assert_eq!(
        events[6],
        SyntheticKey {
            key_up: true,
            ..win_down
        }
    );
    assert_eq!(events[2], down(0x12, 0x38));
    assert!(events.iter().all(|event| event.extra_info == TAG));
}

#[test]
fn extended_keys_carry_the_extended_flag() {
    let arrow = ReplayKey {
        virtual_key: vk::LEFT,
        scan_code: Some(0x4B),
        modifiers: ModifierSet::default(),
    };
    let events = replay_inputs(&arrow).unwrap();
    assert!(events.iter().all(|event| event.extended));
    for key in [
        vk::LEFT,
        vk::RIGHT,
        vk::UP,
        vk::DOWN,
        vk::HOME,
        vk::END,
        vk::PRIOR,
        vk::NEXT,
        vk::INSERT,
        vk::DELETE,
        vk::RCONTROL,
        vk::RMENU,
        vk::LWIN,
        vk::RWIN,
    ] {
        assert!(is_extended_key(key), "{key:#x}");
    }
    for key in [vk::RETURN, vk::TAB, vk::BACK, vk::SPACE, 0x41, vk::LSHIFT] {
        assert!(!is_extended_key(key), "{key:#x}");
    }
}

#[test]
fn replay_rejects_keys_that_cannot_be_injected() {
    for virtual_key in [0, 0xFF, 0x100, u32::MAX] {
        let key = ReplayKey {
            virtual_key,
            scan_code: None,
            modifiers: ModifierSet::default(),
        };
        assert_eq!(replay_inputs(&key), Err(InputError::InvalidVirtualKey));
    }
    let key = ReplayKey {
        virtual_key: vk::RETURN,
        scan_code: Some(0x1_0000),
        modifiers: ModifierSet::default(),
    };
    assert_eq!(replay_inputs(&key), Err(InputError::InvalidScanCode));
}

#[test]
fn the_extra_info_tag_identifies_raumakey_input() {
    assert!(is_raumakey_extra_info(TAG));
    assert!(!is_raumakey_extra_info(0));
    assert!(!is_raumakey_extra_info(0xEBCA_1234));
    assert!(!is_raumakey_extra_info(TAG + 1));
}

#[test]
fn flags_match_the_keybdinput_bits() {
    let mut key = down(0x41, 0x1E);
    assert_eq!(key.flags(), 0);
    key.extended = true;
    assert_eq!(key.flags(), 0x1);
    key.key_up = true;
    assert_eq!(key.flags(), 0x3);
    key.unicode = true;
    assert_eq!(key.flags(), 0x7);
}

#[cfg(windows)]
#[test]
fn conversion_to_win32_input_preserves_every_field() {
    use windows::Win32::UI::Input::KeyboardAndMouse::INPUT_KEYBOARD;

    let key = SyntheticKey {
        virtual_key: 0,
        scan_code: 0x1EBF,
        key_up: true,
        unicode: true,
        extended: false,
        extra_info: TAG,
    };
    let input = key.to_input();
    assert_eq!(input.r#type, INPUT_KEYBOARD);
    // SAFETY: `to_input` always initializes the keyboard member of the union.
    let ki = unsafe { input.Anonymous.ki };
    assert_eq!(ki.wVk.0, 0);
    assert_eq!(ki.wScan, 0x1EBF);
    assert_eq!(ki.dwFlags.0, 0x6);
    assert_eq!(ki.time, 0);
    assert_eq!(ki.dwExtraInfo, TAG);
}

#[cfg(windows)]
mod extra_info {
    use raumakey_windows::input::{current_message_is_raumakey_injected, message_extra_info};
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::UI::WindowsAndMessaging::SetMessageExtraInfo;

    use super::TAG;
    use crate::support::on_fresh_thread;

    /// The legacy EVKey sentinel (clean-room spec §3.1), which is not Raumakey's tag.
    const LEGACY_SENTINEL: u32 = 0xEBCA_1234;

    fn set_thread_extra_info(value: usize) -> usize {
        // SAFETY: changes only the calling thread's extra message information.
        unsafe { SetMessageExtraInfo(LPARAM(value as isize)) }.0 as usize
    }

    #[test]
    fn a_fresh_thread_has_no_extra_information() {
        on_fresh_thread(|| {
            assert_eq!(message_extra_info(), 0);
            assert!(!current_message_is_raumakey_injected());
        });
    }

    #[test]
    fn the_extra_information_of_the_thread_is_read_back_and_classified() {
        on_fresh_thread(|| {
            assert_eq!(set_thread_extra_info(TAG), 0);
            assert_eq!(message_extra_info(), TAG);
            assert!(current_message_is_raumakey_injected());

            // Values with the top bit set (negative LPARAMs on i686) come back unchanged.
            assert_eq!(set_thread_extra_info(LEGACY_SENTINEL as usize), TAG);
            assert_eq!(message_extra_info(), LEGACY_SENTINEL as usize);
            assert!(!current_message_is_raumakey_injected());

            set_thread_extra_info(TAG + 1);
            assert!(!current_message_is_raumakey_injected());
            set_thread_extra_info(0);
            assert_eq!(message_extra_info(), 0);
        });
    }
}

/// Sends real input to the desktop. Manual test only: run with `--ignored` on a test machine
/// with a focused text field that may receive "a".
#[cfg(windows)]
#[test]
#[ignore = "manual: sends real keyboard input to the focused window"]
fn manual_send_types_one_character() {
    let events = unicode_text_inputs(&[0x61]).unwrap();
    let report = raumakey_windows::input::send(&events).unwrap();
    assert!(report.is_complete());
}
