//! `keyboard`: key state arrays, KLID decoding (pure), and read-only layout queries.

use raumakey_windows::keyboard::{KeyboardSnapshot, KlidSource, klid_from_hkl};

#[test]
fn the_key_state_array_reflects_the_snapshot() {
    let empty = KeyboardSnapshot::default().key_state();
    assert!(empty.iter().all(|&byte| byte == 0));

    let shift_caps = KeyboardSnapshot {
        shift: true,
        caps_lock: true,
        ..KeyboardSnapshot::default()
    }
    .key_state();
    assert_eq!(shift_caps[0x10], 0x80);
    assert_eq!(shift_caps[0xA0], 0x80);
    assert_eq!(shift_caps[0x14], 0x01);
    assert_eq!(shift_caps[0x11], 0);

    let ctrl_alt = KeyboardSnapshot {
        ctrl: true,
        alt: true,
        ..KeyboardSnapshot::default()
    }
    .key_state();
    for index in [0x11, 0xA2, 0x12, 0xA4] {
        assert_eq!(ctrl_alt[index], 0x80, "{index:#x}");
    }

    let altgr = KeyboardSnapshot {
        alt_gr: true,
        ..KeyboardSnapshot::default()
    }
    .key_state();
    // AltGr is reported to layouts as Ctrl+Alt with the right Alt key.
    for index in [0x11, 0xA2, 0x12, 0xA5] {
        assert_eq!(altgr[index], 0x80, "{index:#x}");
    }
    assert_eq!(altgr[0xA4], 0);
}

#[test]
fn klids_are_derived_from_keyboard_layout_handles() {
    assert_eq!(
        klid_from_hkl(0x0409_0409),
        KlidSource::Direct("00000409".to_owned())
    );
    // Vietnamese input language with the US keyboard.
    assert_eq!(
        klid_from_hkl(0x0409_042A),
        KlidSource::Direct("00000409".to_owned())
    );
    assert_eq!(
        klid_from_hkl(0x042A_042A),
        KlidSource::Direct("0000042A".to_owned())
    );
    assert_eq!(
        klid_from_hkl(0xE001_0411),
        KlidSource::Direct("E0010411".to_owned())
    );
    assert_eq!(
        klid_from_hkl(0x0000_0409),
        KlidSource::Direct("00000409".to_owned())
    );
    assert_eq!(
        klid_from_hkl(0xF002_0409),
        KlidSource::LayoutId {
            layout_id: 0x0002,
            language: 0x0409
        }
    );
}

#[cfg(target_pointer_width = "64")]
#[test]
fn sign_extended_handles_are_truncated_to_32_bits() {
    assert_eq!(
        klid_from_hkl(0xFFFF_FFFF_F002_0409_u64 as usize),
        KlidSource::LayoutId {
            layout_id: 0x0002,
            language: 0x0409
        }
    );
}

#[cfg(windows)]
mod win {
    use raumakey_windows::keyboard::{
        KeyboardSnapshot, is_altgr_layout, layout_for_thread, layout_name, translate_key,
    };

    #[test]
    fn the_current_layout_has_a_klid() {
        let hkl = layout_for_thread(0);
        assert_ne!(hkl, 0);
        let name = layout_name(hkl).expect("the active layout has a KLID");
        assert_eq!(name.len(), 8, "{name}");
        assert!(name.bytes().all(|byte| byte.is_ascii_hexdigit()), "{name}");
        // Read-only heuristic; the value depends on the user's layout.
        let _ = is_altgr_layout(hkl);
    }

    #[test]
    fn space_and_enter_translate_on_every_layout() {
        let hkl = layout_for_thread(0);
        let none = KeyboardSnapshot::default();
        let space = translate_key(0x20, 0x39, &none, hkl);
        assert_eq!(space.units, [0x20]);
        assert!(!space.dead_key);
        let enter = translate_key(0x0D, 0x1C, &none, hkl);
        assert_eq!(enter.units, [0x0D]);
        let shift = translate_key(
            0x10,
            0x2A,
            &KeyboardSnapshot {
                shift: true,
                ..none
            },
            hkl,
        );
        assert!(shift.units.is_empty(), "modifiers produce no text");
    }

    #[test]
    fn capturing_the_snapshot_is_read_only() {
        let _ = KeyboardSnapshot::capture();
    }
}
