//! D24, single hotkey owner: while connected to the controller the core never classifies
//! hotkeys; while disconnected it classifies them itself with the configured hotkeys (D5
//! defaults: toggle `Ctrl+Shift`, hard reset `Ctrl+Shift+Alt+F12` and
//! `Ctrl+Shift+Alt+Space`).

use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::vk;
use raumakey_session::{KeyStroke, SelectionChange, ServiceCore};

use crate::support::{Down, FakeDocument, host, release, stroke};

/// A key of `doc` with Ctrl, Shift, and Alt as given.
fn chord(doc: &FakeDocument, virtual_key: u32, text: &str, held: (bool, bool, bool)) -> KeyStroke {
    let mut key = stroke(virtual_key, text, &doc.host);
    (key.modifiers.ctrl, key.modifiers.shift, key.modifiers.alt) = held;
    key
}

/// Taps Ctrl+Shift: Ctrl down, Shift down, Shift up, Ctrl up. Returns whether the core
/// claimed the last key-up (the moment a modifier-only hotkey fires).
fn tap_ctrl_shift(core: &mut ServiceCore, doc: &mut FakeDocument) -> bool {
    let ctrl = chord(doc, vk::CONTROL, "", (false, false, false));
    let shift = chord(doc, vk::SHIFT, "", (true, false, false));
    assert!(
        !doc.hold(core, &ctrl).claimed,
        "modifier key-downs are never claimed"
    );
    assert!(!doc.hold(core, &shift).claimed);
    assert!(!release(core, &shift));
    let claimed = core.test_key_up(&ctrl);
    if claimed {
        assert!(
            !core.key_up(&ctrl),
            "the key-up of a modifier-only hotkey reaches the application"
        );
    }
    claimed
}

/// Holds Ctrl+Shift+Alt, presses `virtual_key`, and releases everything. Returns the
/// key-down of `virtual_key` and whether its key-up was eaten.
fn ctrl_shift_alt(
    core: &mut ServiceCore,
    doc: &mut FakeDocument,
    virtual_key: u32,
    text: &str,
) -> (Down, bool) {
    let modifiers = [
        chord(doc, vk::CONTROL, "", (false, false, false)),
        chord(doc, vk::SHIFT, "", (true, false, false)),
        chord(doc, vk::MENU, "", (true, true, false)),
    ];
    for modifier in &modifiers {
        assert!(!doc.hold(core, modifier).claimed);
    }
    let key = chord(doc, virtual_key, text, (true, true, true));
    let down = doc.hold(core, &key);
    let up_eaten = release(core, &key);
    for modifier in modifiers.iter().rev() {
        assert!(!release(core, modifier), "modifier key-ups are never eaten");
    }
    (down, up_eaten)
}

#[test]
fn d24_disconnected_ctrl_shift_toggles_the_local_mode() {
    let mut core = ServiceCore::new();
    assert!(!core.is_connected());
    let mut doc = FakeDocument::new(1);
    assert!(tap_ctrl_shift(&mut core, &mut doc));
    assert!(!core.vietnamese_mode());
    doc.type_text(&mut core, "aa");
    assert_eq!(doc.text(), "aa");
    assert!(tap_ctrl_shift(&mut core, &mut doc));
    assert!(core.vietnamese_mode());
    doc.type_text(&mut core, " aa");
    assert_eq!(doc.text(), "aa â");
}

#[test]
fn d24_the_toggle_ends_the_word_in_progress() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    tap_ctrl_shift(&mut core, &mut doc);
    tap_ctrl_shift(&mut core, &mut doc);
    assert!(core.vietnamese_mode());
    doc.type_char(&mut core, 'a');
    assert_eq!(
        doc.text(),
        "aa",
        "matrix #24: the word ended before the mode changed"
    );
}

#[test]
fn d24_a_key_typed_during_the_chord_cancels_the_toggle() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let ctrl = chord(&doc, vk::CONTROL, "", (false, false, false));
    let shift = chord(&doc, vk::SHIFT, "", (true, false, false));
    doc.hold(&mut core, &ctrl);
    doc.hold(&mut core, &shift);
    doc.press(
        &mut core,
        &chord(&doc, u32::from(b'T'), "", (true, true, false)),
    );
    release(&mut core, &shift);
    assert!(!core.test_key_up(&ctrl));
    assert!(core.vietnamese_mode());
}

/// Holds Ctrl and Shift, runs `between`, then releases Shift and tests the release of Ctrl.
/// Returns whether that release was claimed, which is when the toggle would fire.
fn ctrl_shift_around(
    core: &mut ServiceCore,
    doc: &mut FakeDocument,
    between: impl FnOnce(&mut ServiceCore, &mut FakeDocument),
) -> bool {
    let ctrl = chord(doc, vk::CONTROL, "", (false, false, false));
    let shift = chord(doc, vk::SHIFT, "", (true, false, false));
    doc.hold(core, &ctrl);
    doc.hold(core, &shift);
    between(core, doc);
    release(core, &shift);
    core.test_key_up(&ctrl)
}

#[test]
fn d24_a_click_reported_while_ctrl_shift_is_held_cancels_the_toggle() {
    // Ctrl+Shift+click extends the selection; the edit sink reports the change.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let claimed = ctrl_shift_around(&mut core, &mut doc, |core, doc| {
        core.selection_changed(doc.id, SelectionChange::Other);
    });
    assert!(!claimed);
    assert!(core.vietnamese_mode());
}

#[test]
fn d24_a_mouse_press_while_ctrl_shift_is_held_cancels_the_toggle() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let claimed = ctrl_shift_around(&mut core, &mut doc, |core, doc| {
        core.mouse_down(&doc.host);
    });
    assert!(!claimed);
    assert!(core.vietnamese_mode());
}

#[test]
fn d24_a_mouse_press_cancels_the_toggle_even_where_it_keeps_the_word() {
    let mut core = ServiceCore::new();
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = 1;
    snapshot.compatibility.default_reset_on_mouse = false;
    core.apply_runtime_snapshot(&snapshot)
        .expect("valid snapshot");
    let mut doc = FakeDocument::new(1);
    let claimed = ctrl_shift_around(&mut core, &mut doc, |core, doc| {
        core.mouse_down(&doc.host);
    });
    assert!(!claimed);
    assert!(core.vietnamese_mode());
}

#[test]
fn d24_disconnected_ctrl_shift_alt_f12_hard_resets_and_is_eaten() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    let (down, up_eaten) = ctrl_shift_alt(&mut core, &mut doc, vk::F12, "");
    assert!(down.claimed);
    assert!(
        down.outcome.eaten,
        "a key hotkey never reaches the application"
    );
    assert!(up_eaten, "nor does its key-up");
    assert!(core.vietnamese_mode(), "a hard reset keeps the mode");
    doc.type_char(&mut core, 's');
    assert_eq!(
        doc.text(),
        "as",
        "matrix #25: the word ended, committed text stayed"
    );
    assert_eq!(doc.attempts, 0);
}

#[test]
fn d24_ctrl_shift_alt_space_is_the_other_hard_reset() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    let (down, up_eaten) = ctrl_shift_alt(&mut core, &mut doc, vk::SPACE, " ");
    assert!(down.outcome.eaten);
    assert!(up_eaten);
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "aa");
}

#[test]
fn d24_the_auto_repeat_of_a_hard_reset_key_is_swallowed() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let modifiers = [
        chord(&doc, vk::CONTROL, "", (false, false, false)),
        chord(&doc, vk::SHIFT, "", (true, false, false)),
        chord(&doc, vk::MENU, "", (true, true, false)),
    ];
    for modifier in &modifiers {
        doc.hold(&mut core, modifier);
    }
    let f12 = chord(&doc, vk::F12, "", (true, true, true));
    assert!(doc.hold(&mut core, &f12).outcome.eaten);
    let repeat = doc.hold(&mut core, &f12);
    assert!(repeat.claimed);
    assert!(repeat.outcome.eaten);
    assert!(release(&mut core, &f12));
}

#[test]
fn d24_connected_the_core_never_classifies_hotkeys() {
    let mut core = ServiceCore::new();
    core.set_connected(true);
    assert!(core.is_connected());
    let mut doc = FakeDocument::new(1);
    assert!(
        !tap_ctrl_shift(&mut core, &mut doc),
        "the controller owns the toggle"
    );
    assert!(core.vietnamese_mode());
    let (down, up_eaten) = ctrl_shift_alt(&mut core, &mut doc, vk::F12, "");
    assert!(
        !down.outcome.eaten,
        "the controller's hook swallows hotkeys before they reach the TIP"
    );
    assert!(!up_eaten);
    doc.type_text(&mut core, "aa");
    assert_eq!(doc.text(), "â");
}

#[test]
fn d24_injected_modifiers_are_never_classified() {
    let mut core = ServiceCore::new();
    let doc = FakeDocument::new(1);
    let mut ctrl = chord(&doc, vk::CONTROL, "", (false, false, false));
    let mut shift = chord(&doc, vk::SHIFT, "", (true, false, false));
    ctrl.extra_info = RAUMAKEY_INJECTED_TAG;
    shift.extra_info = RAUMAKEY_INJECTED_TAG;
    assert!(!core.test_key_down(&ctrl));
    assert!(!core.test_key_down(&shift));
    assert!(!core.test_key_up(&shift));
    assert!(!core.test_key_up(&ctrl));
    assert!(!core.key_up(&ctrl));
    assert!(core.vietnamese_mode());
}

#[test]
fn d24_losing_the_connection_hands_classification_back() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    core.set_connected(true);
    core.set_connected(false);
    assert!(!core.is_connected());
    assert!(tap_ctrl_shift(&mut core, &mut doc));
    assert!(!core.vietnamese_mode());
}

#[test]
fn d24_a_connection_change_mid_chord_never_fires() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    core.set_connected(true);
    let ctrl = chord(&doc, vk::CONTROL, "", (false, false, false));
    let shift = chord(&doc, vk::SHIFT, "", (true, false, false));
    doc.hold(&mut core, &ctrl);
    doc.hold(&mut core, &shift);
    // The controller goes away while the user holds the chord.
    core.set_connected(false);
    release(&mut core, &shift);
    assert!(!core.test_key_up(&ctrl));
    assert!(core.vietnamese_mode());
}

#[test]
fn d24_a_focus_change_mid_chord_never_fires() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let claimed = ctrl_shift_around(&mut core, &mut doc, |core, doc| {
        core.set_focus(Some(doc.id));
    });
    assert!(!claimed);
    assert!(core.vietnamese_mode());
}

#[test]
fn d24_a_document_change_mid_chord_never_fires() {
    let mut core = ServiceCore::new();
    let mut first = FakeDocument::new(1);
    let mut second = FakeDocument::new(2);
    let ctrl = chord(&first, vk::CONTROL, "", (false, false, false));
    let shift = chord(&second, vk::SHIFT, "", (true, false, false));
    first.hold(&mut core, &ctrl);
    // The next key comes from another document: the classifier starts again.
    second.hold(&mut core, &shift);
    release(&mut core, &shift);
    assert!(!core.test_key_up(&ctrl));
    assert!(core.vietnamese_mode());
}

#[test]
fn d24_hotkeys_of_the_snapshot_replace_the_defaults() {
    let mut core = ServiceCore::new();
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = 1;
    snapshot.hotkeys.toggle_mode = vec!["Alt+Z".parse().expect("hotkey")];
    core.apply_runtime_snapshot(&snapshot)
        .expect("valid snapshot");
    let mut doc = FakeDocument::new(1);
    assert!(!tap_ctrl_shift(&mut core, &mut doc));
    assert!(core.vietnamese_mode());
    let alt = chord(&doc, vk::MENU, "", (false, false, false));
    doc.hold(&mut core, &alt);
    let z = chord(&doc, u32::from(b'Z'), "", (false, false, true));
    let press = doc.press(&mut core, &z);
    assert!(press.outcome.eaten);
    assert!(press.up_eaten);
    release(&mut core, &alt);
    assert!(!core.vietnamese_mode());
}

#[test]
fn d24_altgr_types_characters_on_altgr_layouts() {
    let mut core = ServiceCore::new();
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = 1;
    snapshot.hotkeys.hard_reset = vec!["Alt+E".parse().expect("hotkey")];
    core.apply_runtime_snapshot(&snapshot)
        .expect("valid snapshot");
    core.set_altgr_layout(true);
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    // AltGr is right Alt: the generic Alt code with the extended flag. The COM layer drops
    // the synthetic left Ctrl that Windows sends with it.
    let mut altgr = chord(&doc, vk::MENU, "", (false, false, false));
    altgr.extended = true;
    doc.hold(&mut core, &altgr);
    let mut euro = chord(&doc, u32::from(b'E'), "€", (true, false, true));
    euro.modifiers.alt_gr = true;
    let press = doc.press(&mut core, &euro);
    assert!(
        !press.outcome.eaten,
        "an AltGr character is typed, not a hotkey"
    );
    release(&mut core, &altgr);
    assert_eq!(doc.text(), "a€");

    // Without AltGr on the layout, right Alt is Alt and the same keys are the hotkey.
    core.set_altgr_layout(false);
    doc.type_char(&mut core, ' ');
    doc.type_char(&mut core, 'a');
    doc.hold(&mut core, &altgr);
    let e = chord(&doc, u32::from(b'E'), "", (false, false, true));
    let press = doc.press(&mut core, &e);
    assert!(press.outcome.eaten);
    release(&mut core, &altgr);
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "a€ aa", "the hard reset ended the word");
}

#[test]
fn d24_hotkeys_work_without_a_focused_document() {
    let mut core = ServiceCore::new();
    let mut nowhere = host(1);
    nowhere.document_id = None;
    let mut doc = FakeDocument::with_host(nowhere);
    assert!(tap_ctrl_shift(&mut core, &mut doc));
    assert!(!core.vietnamese_mode());
}

#[test]
fn d24_hotkeys_work_in_secure_fields() {
    // Classifying a hotkey is not a transformation of the field's text.
    let mut core = ServiceCore::new();
    let mut secure = host(1);
    secure.is_secure_field = true;
    let mut doc = FakeDocument::with_host(secure);
    assert!(tap_ctrl_shift(&mut core, &mut doc));
    assert!(!core.vietnamese_mode());
}

#[test]
fn d24_hosts_left_to_the_controller_are_not_classified() {
    let mut core = ServiceCore::new();
    let mut fallback = host(1);
    fallback.supports_tsf_range_edit = false;
    let mut doc = FakeDocument::with_host(fallback);
    assert!(!tap_ctrl_shift(&mut core, &mut doc));
    assert!(core.vietnamese_mode());
}
