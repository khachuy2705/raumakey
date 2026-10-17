//! Hotkey classification (plan §6, D5, D24; design §11; matrix B22, B24, B25): matching, the
//! verdicts that keep a consumed key press away from the application, and AltGr layouts.

use raumakey_model::hotkey::{Hotkey, HotkeyAction, HotkeyConfig, HotkeyKey};
use raumakey_model::input::vk;
use raumakey_policy::hotkey::{HotkeyClassifier, HotkeyMatch, KeyVerdict};

const A: u32 = 0x41;
const Q: u32 = 0x51;
const Z: u32 = 0x5A;
const F11: u32 = 0x7A;

const HARD_RESET: Option<HotkeyMatch> = Some(HotkeyMatch {
    action: HotkeyAction::HardReset,
    consume: true,
});

const FIRE_HARD_RESET: KeyVerdict = KeyVerdict::Hotkey(HotkeyMatch {
    action: HotkeyAction::HardReset,
    consume: true,
});

const PASS: KeyVerdict = KeyVerdict::Pass;
const SWALLOW: KeyVerdict = KeyVerdict::Swallow;
const DOWN: bool = true;
const UP: bool = false;

/// Classifies `(vk, is_down)` transitions in order.
fn feed(c: &mut HotkeyClassifier, events: &[(u32, bool)]) -> Vec<KeyVerdict> {
    events
        .iter()
        .map(|&(key, is_down)| c.classify(key, is_down))
        .collect()
}

/// Ctrl, Shift, and Alt (left side) key-downs.
const CTRL_SHIFT_ALT_DOWN: [(u32, bool); 3] =
    [(vk::LCONTROL, DOWN), (vk::LSHIFT, DOWN), (vk::LMENU, DOWN)];

fn defaults() -> HotkeyClassifier {
    HotkeyClassifier::new(&HotkeyConfig::default())
}

fn hotkey(text: &str) -> Hotkey {
    text.parse().unwrap()
}

fn config(toggle_mode: Vec<Hotkey>, hard_reset: Vec<Hotkey>) -> HotkeyConfig {
    HotkeyConfig {
        toggle_mode,
        hard_reset,
    }
}

fn down(c: &mut HotkeyClassifier, keys: &[u32]) -> Vec<Option<HotkeyMatch>> {
    keys.iter().map(|&k| c.on_key(k, true)).collect()
}

fn up(c: &mut HotkeyClassifier, keys: &[u32]) -> Vec<Option<HotkeyMatch>> {
    keys.iter().map(|&k| c.on_key(k, false)).collect()
}

fn assert_silent(results: &[Option<HotkeyMatch>]) {
    assert!(results.iter().all(Option::is_none), "{results:?}");
}

/// Matrix #25: `Ctrl+Shift+Alt+F12` hard-resets on the F12 key-down and consumes it.
#[test]
fn b25_ctrl_shift_alt_f12_hard_resets_on_key_down() {
    let mut c = defaults();
    assert_silent(&down(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]));
    assert_eq!(c.on_key(vk::F12, true), HARD_RESET);
    assert_eq!(c.on_key(vk::F12, false), None);
    assert_silent(&up(&mut c, &[vk::LMENU, vk::LSHIFT, vk::LCONTROL]));
}

/// Matrix #25: `Ctrl+Shift+Alt+Space` is the second hard-reset combination.
#[test]
fn b25_ctrl_shift_alt_space_hard_resets_with_generic_modifier_codes() {
    let mut c = defaults();
    assert_silent(&down(&mut c, &[vk::SHIFT, vk::MENU, vk::CONTROL]));
    assert_eq!(c.on_key(vk::SPACE, true), HARD_RESET);
    assert_eq!(c.on_key(vk::SPACE, false), None);
    assert_silent(&up(&mut c, &[vk::CONTROL, vk::MENU, vk::SHIFT]));
}

#[test]
fn b25_right_side_modifiers_count() {
    let mut c = defaults();
    down(&mut c, &[vk::RCONTROL, vk::RSHIFT, vk::RMENU]);
    assert_eq!(c.on_key(vk::F12, true), HARD_RESET);
}

/// Matrix #25: holding the hard-reset combination runs the action once, and the application sees
/// no part of the consumed key press: its auto-repeats and its key-up are swallowed as well.
#[test]
fn b25_auto_repeat_does_not_fire_twice_and_is_swallowed() {
    let mut c = defaults();
    assert_eq!(feed(&mut c, &CTRL_SHIFT_ALT_DOWN), [PASS; 3]);
    let held = feed(
        &mut c,
        &[
            (vk::F12, DOWN),
            (vk::F12, DOWN),
            (vk::F12, DOWN),
            (vk::F12, DOWN),
            (vk::F12, UP),
        ],
    );
    assert_eq!(
        held,
        [FIRE_HARD_RESET, SWALLOW, SWALLOW, SWALLOW, SWALLOW],
        "repeats and the key-up must be withheld"
    );
    assert_eq!(
        held.iter().filter_map(|v| v.hotkey()).count(),
        1,
        "the action runs once"
    );
    assert_eq!(
        c.classify(vk::F12, DOWN),
        FIRE_HARD_RESET,
        "a new press while the modifiers stay held fires again"
    );
    assert_eq!(c.classify(vk::F12, UP), SWALLOW);
}

/// The specified `on_key` reports only hotkey matches; swallowed events come back as `None`.
#[test]
fn on_key_reports_only_the_hotkey_match() {
    let mut c = defaults();
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]);
    assert_eq!(c.on_key(vk::F12, true), HARD_RESET);
    assert_eq!(c.on_key(vk::F12, true), None, "auto-repeat");
    assert_eq!(c.on_key(vk::F12, false), None, "key-up");
}

#[test]
fn a_consumed_key_stays_swallowed_after_the_modifiers_are_released() {
    let mut c = defaults();
    feed(&mut c, &CTRL_SHIFT_ALT_DOWN);
    assert_eq!(c.classify(vk::F12, DOWN), FIRE_HARD_RESET);
    assert_eq!(
        feed(
            &mut c,
            &[
                (vk::LMENU, UP),
                (vk::LSHIFT, UP),
                (vk::LCONTROL, UP),
                (vk::F12, DOWN),
                (vk::F12, UP),
            ]
        ),
        [PASS, PASS, PASS, SWALLOW, SWALLOW],
        "the key-up often arrives after the modifiers"
    );
    assert_eq!(
        feed(&mut c, &[(vk::F12, DOWN), (vk::F12, UP)]),
        [PASS, PASS],
        "the next press of the bare key is ordinary typing"
    );
}

#[test]
fn modifier_events_are_never_swallowed() {
    let mut c = defaults();
    feed(&mut c, &CTRL_SHIFT_ALT_DOWN);
    assert_eq!(c.classify(vk::F12, DOWN), FIRE_HARD_RESET);
    assert_eq!(
        feed(
            &mut c,
            &[
                (vk::LWIN, DOWN),
                (vk::LWIN, UP),
                (vk::LMENU, UP),
                (vk::LSHIFT, UP),
                (vk::LCONTROL, UP),
            ]
        ),
        [PASS; 5]
    );
    assert_eq!(c.classify(vk::F12, UP), SWALLOW);
}

#[test]
fn other_keys_pass_while_a_consumed_key_is_held() {
    let mut c = defaults();
    feed(&mut c, &CTRL_SHIFT_ALT_DOWN);
    assert_eq!(c.classify(vk::F12, DOWN), FIRE_HARD_RESET);
    assert_eq!(
        feed(&mut c, &[(A, DOWN), (A, DOWN), (A, UP), (vk::F12, UP)]),
        [PASS, PASS, PASS, SWALLOW]
    );
}

#[test]
fn two_consumed_keys_held_together_both_stay_swallowed() {
    let mut c = defaults();
    feed(&mut c, &CTRL_SHIFT_ALT_DOWN);
    assert_eq!(
        feed(
            &mut c,
            &[
                (vk::F12, DOWN),
                (vk::SPACE, DOWN),
                (vk::SPACE, DOWN),
                (vk::F12, UP),
                (vk::SPACE, UP),
            ]
        ),
        [FIRE_HARD_RESET, FIRE_HARD_RESET, SWALLOW, SWALLOW, SWALLOW]
    );
}

/// Consuming the auto-repeat of a key-down that already reached the application would also
/// withhold its key-up and leave the key stuck down in the host.
#[test]
fn a_hotkey_never_fires_on_the_auto_repeat_of_a_key_that_reached_the_application() {
    let mut c = defaults();
    feed(&mut c, &CTRL_SHIFT_ALT_DOWN);
    assert_eq!(
        feed(
            &mut c,
            &[
                (vk::LWIN, DOWN),
                (vk::F12, DOWN),
                (vk::LWIN, UP),
                (vk::F12, DOWN),
                (vk::F12, DOWN),
                (vk::F12, UP),
            ]
        ),
        [PASS; 6],
        "Win was extra when F12 went down"
    );
    assert_eq!(
        c.classify(vk::F12, DOWN),
        FIRE_HARD_RESET,
        "a new press with the exact modifiers fires"
    );
}

/// After a lost key-up, the next press of a consumed key that reaches the application must have
/// its key-up delivered too, or the key would stay down in the host.
#[test]
fn a_new_press_after_a_lost_key_up_reaches_the_application_whole() {
    let mut c = defaults();
    feed(&mut c, &CTRL_SHIFT_ALT_DOWN);
    assert_eq!(c.classify(vk::F12, DOWN), FIRE_HARD_RESET);
    // The F12 key-up is lost; the modifiers are released and another key is typed.
    feed(
        &mut c,
        &[
            (vk::LMENU, UP),
            (vk::LSHIFT, UP),
            (vk::LCONTROL, UP),
            (A, DOWN),
            (A, UP),
        ],
    );
    assert_eq!(
        feed(&mut c, &[(vk::F12, DOWN), (vk::F12, UP)]),
        [PASS, PASS]
    );
}

#[test]
fn reset_forgets_the_consumed_key() {
    let mut c = defaults();
    feed(&mut c, &CTRL_SHIFT_ALT_DOWN);
    assert_eq!(c.classify(vk::F12, DOWN), FIRE_HARD_RESET);
    c.reset();
    assert_eq!(
        c.classify(vk::F12, UP),
        PASS,
        "after a reset nothing is known about earlier presses"
    );

    feed(&mut c, &CTRL_SHIFT_ALT_DOWN);
    assert_eq!(c.classify(vk::F12, DOWN), FIRE_HARD_RESET);
    c.reset();
    assert_eq!(
        feed(&mut c, &[(vk::F12, DOWN), (vk::F12, UP)]),
        [PASS, PASS],
        "the modifiers are forgotten as well"
    );
}

/// The scenario of the review: a hotkey fires, its key-up is lost across a reset, and the
/// combination is pressed again.
#[test]
fn after_reset_a_lost_key_up_does_not_block_the_next_press() {
    let mut c = defaults();
    feed(&mut c, &CTRL_SHIFT_ALT_DOWN);
    assert_eq!(c.classify(vk::F12, DOWN), FIRE_HARD_RESET);
    c.reset();
    feed(&mut c, &CTRL_SHIFT_ALT_DOWN);
    assert_eq!(c.classify(vk::F12, DOWN), FIRE_HARD_RESET);
}

const FIRE_TOGGLE: KeyVerdict = KeyVerdict::Hotkey(HotkeyMatch {
    action: HotkeyAction::ToggleMode,
    consume: true,
});

/// Design §11 and matrix #22: on an AltGr layout an AltGr chord types a character (AltGr+Q is
/// `@` on a German layout) and must not run an `Alt+Q` hotkey. The adapter has dropped the
/// synthetic left Ctrl that Windows sends with AltGr.
#[test]
fn altgr_chords_do_not_match_alt_hotkeys_on_altgr_layouts() {
    let mut c = HotkeyClassifier::new(&config(vec![hotkey("Alt+Q")], Vec::new()));
    c.set_altgr_layout(true);
    assert!(c.altgr_layout());
    assert_eq!(
        feed(
            &mut c,
            &[
                (vk::RMENU, DOWN),
                (Q, DOWN),
                (Q, DOWN),
                (Q, UP),
                (vk::RMENU, UP)
            ]
        ),
        [PASS; 5],
        "the character reaches the application exactly once per press"
    );
    assert_eq!(
        feed(
            &mut c,
            &[(vk::LMENU, DOWN), (Q, DOWN), (Q, UP), (vk::LMENU, UP)]
        ),
        [PASS, FIRE_TOGGLE, SWALLOW, PASS],
        "left Alt is still Alt"
    );
}

#[test]
fn right_alt_is_alt_unless_the_layout_has_altgr() {
    let mut c = HotkeyClassifier::new(&config(vec![hotkey("Alt+Q")], Vec::new()));
    assert!(!c.altgr_layout(), "off by default");
    assert_eq!(
        feed(
            &mut c,
            &[(vk::RMENU, DOWN), (Q, DOWN), (Q, UP), (vk::RMENU, UP)]
        ),
        [PASS, FIRE_TOGGLE, SWALLOW, PASS]
    );
}

/// Without AltGr, right Alt is Alt for modifier-only hotkeys as well: it completes a `Ctrl+Alt`
/// tap whether it goes down first or last. Only an AltGr layout turns it into AltGr.
#[test]
fn right_alt_completes_modifier_only_hotkeys_unless_the_layout_has_altgr() {
    let mut c = HotkeyClassifier::new(&config(vec![hotkey("Ctrl+Alt")], Vec::new()));
    let toggle = KeyVerdict::Hotkey(TOGGLE.unwrap());
    let taps = [
        [
            (vk::LCONTROL, DOWN),
            (vk::RMENU, DOWN),
            (vk::RMENU, UP),
            (vk::LCONTROL, UP),
        ],
        [
            (vk::RMENU, DOWN),
            (vk::RCONTROL, DOWN),
            (vk::RCONTROL, UP),
            (vk::RMENU, UP),
        ],
    ];
    for tap in taps {
        assert_eq!(
            feed(&mut c, &tap),
            [PASS, PASS, PASS, toggle],
            "right Alt is Alt: {tap:x?}"
        );
    }
    c.set_altgr_layout(true);
    for tap in taps {
        assert_eq!(feed(&mut c, &tap), [PASS; 4], "AltGr is not Alt: {tap:x?}");
    }
    c.set_altgr_layout(false);
    for tap in taps {
        assert_eq!(
            feed(&mut c, &tap),
            [PASS, PASS, PASS, toggle],
            "right Alt is Alt again: {tap:x?}"
        );
    }
}

/// With AltGr held every key types an AltGr character, so no key hotkey matches, not even one
/// whose modifiers are the other held keys.
#[test]
fn no_key_hotkey_matches_while_altgr_is_held() {
    let mut c = HotkeyClassifier::new(&config(
        Vec::new(),
        vec![hotkey("Ctrl+Q"), hotkey("Ctrl+Alt+Q")],
    ));
    c.set_altgr_layout(true);
    assert_eq!(
        feed(
            &mut c,
            &[
                (vk::LCONTROL, DOWN),
                (vk::RMENU, DOWN),
                (Q, DOWN),
                (Q, UP),
                (vk::RMENU, UP),
            ]
        ),
        [PASS; 5],
        "Ctrl+AltGr+Q is neither Ctrl+Q nor Ctrl+Alt+Q"
    );
    assert_eq!(
        feed(&mut c, &[(Q, DOWN), (Q, UP)]),
        [FIRE_HARD_RESET, SWALLOW],
        "Ctrl alone is held again"
    );
    assert_eq!(
        feed(&mut c, &[(vk::LMENU, DOWN), (Q, DOWN), (Q, UP)]),
        [PASS, FIRE_HARD_RESET, SWALLOW],
        "left Ctrl+Alt is a shortcut"
    );
}

#[test]
fn altgr_spoils_modifier_only_hotkeys_on_altgr_layouts() {
    let mut c = HotkeyClassifier::new(&config(vec![hotkey("Ctrl+Alt")], Vec::new()));
    c.set_altgr_layout(true);
    assert_eq!(
        feed(
            &mut c,
            &[
                (vk::LCONTROL, DOWN),
                (vk::RMENU, DOWN),
                (vk::RMENU, UP),
                (vk::LCONTROL, UP),
            ]
        ),
        [PASS; 4],
        "Ctrl with an AltGr tap is not a Ctrl+Alt tap"
    );
    assert_eq!(
        feed(
            &mut c,
            &[
                (vk::LCONTROL, DOWN),
                (vk::RMENU, DOWN),
                (vk::RMENU, UP),
                (vk::LMENU, DOWN),
                (vk::LMENU, UP),
                (vk::LCONTROL, UP),
            ]
        ),
        [PASS; 6],
        "AltGr anywhere in the hold spoils it"
    );
    assert_eq!(
        feed(
            &mut c,
            &[
                (vk::LCONTROL, DOWN),
                (vk::LMENU, DOWN),
                (vk::LMENU, UP),
                (vk::LCONTROL, UP),
            ]
        ),
        [PASS, PASS, PASS, KeyVerdict::Hotkey(TOGGLE.unwrap())],
        "left Ctrl+Alt still toggles"
    );
}

/// The setting takes effect with the next transition: a right Alt that went down as Alt is AltGr
/// once the layout has AltGr, so adding Ctrl does not complete a Ctrl+Alt tap.
#[test]
fn a_right_alt_held_across_a_switch_to_an_altgr_layout_stops_counting_as_alt() {
    let mut c = HotkeyClassifier::new(&config(vec![hotkey("Ctrl+Alt")], Vec::new()));
    assert_eq!(c.classify(vk::RMENU, DOWN), PASS);
    c.set_altgr_layout(true);
    assert_eq!(
        feed(
            &mut c,
            &[(vk::LCONTROL, DOWN), (vk::LCONTROL, UP), (vk::RMENU, UP)]
        ),
        [PASS; 3]
    );
}

#[test]
fn the_altgr_layout_setting_survives_reset_and_can_be_turned_off() {
    let mut c = HotkeyClassifier::new(&config(vec![hotkey("Alt+Q")], Vec::new()));
    c.set_altgr_layout(true);
    c.reset();
    assert!(c.altgr_layout());
    assert_eq!(
        feed(
            &mut c,
            &[(vk::RMENU, DOWN), (Q, DOWN), (Q, UP), (vk::RMENU, UP)]
        ),
        [PASS; 4]
    );
    c.set_altgr_layout(false);
    assert!(!c.altgr_layout());
    assert_eq!(
        feed(
            &mut c,
            &[(vk::RMENU, DOWN), (Q, DOWN), (Q, UP), (vk::RMENU, UP)]
        ),
        [PASS, FIRE_TOGGLE, SWALLOW, PASS]
    );
}

#[test]
fn key_verdict_helpers() {
    let toggle = HotkeyMatch {
        action: HotkeyAction::ToggleMode,
        consume: false,
    };
    let reset = HotkeyMatch {
        action: HotkeyAction::HardReset,
        consume: true,
    };
    assert!(!PASS.consumes());
    assert_eq!(PASS.hotkey(), None);
    assert!(SWALLOW.consumes());
    assert_eq!(SWALLOW.hotkey(), None);
    assert!(KeyVerdict::Hotkey(reset).consumes());
    assert_eq!(KeyVerdict::Hotkey(reset).hotkey(), Some(reset));
    assert!(!KeyVerdict::Hotkey(toggle).consumes());
    assert_eq!(KeyVerdict::Hotkey(toggle).hotkey(), Some(toggle));
}

#[test]
fn key_hotkeys_need_exactly_the_configured_modifiers() {
    let mut c = defaults();
    assert_eq!(c.on_key(vk::F12, true), None, "no modifiers");
    c.on_key(vk::F12, false);

    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    assert_eq!(c.on_key(vk::F12, true), None, "Alt missing");
    c.on_key(vk::F12, false);

    down(&mut c, &[vk::LMENU, vk::LWIN]);
    assert_eq!(c.on_key(vk::F12, true), None, "Win is extra");
    c.on_key(vk::F12, false);
    assert_eq!(c.on_key(F11, true), None, "another key");
    c.on_key(F11, false);

    c.on_key(vk::LWIN, false);
    assert_eq!(c.on_key(vk::F12, true), HARD_RESET, "exact set again");
}

#[test]
fn released_modifiers_no_longer_count() {
    let mut c = defaults();
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]);
    c.on_key(vk::LMENU, false);
    assert_eq!(c.on_key(vk::F12, true), None);
}

#[test]
fn a_lost_key_up_does_not_block_the_next_press() {
    let mut c = defaults();
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]);
    assert_eq!(c.on_key(vk::F12, true), HARD_RESET);
    // The F12 key-up never arrives; the modifiers are released.
    up(&mut c, &[vk::LMENU, vk::LSHIFT, vk::LCONTROL]);
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]);
    assert_eq!(c.on_key(vk::F12, true), HARD_RESET);
}

#[test]
fn another_key_press_ends_the_repeat_suppression() {
    let mut c = defaults();
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]);
    assert_eq!(c.on_key(vk::F12, true), HARD_RESET);
    assert_eq!(c.on_key(vk::SPACE, true), HARD_RESET);
    assert_eq!(c.on_key(vk::F12, true), HARD_RESET);
}

#[test]
fn custom_key_hotkeys_fire_their_action() {
    let mut c = HotkeyClassifier::new(&config(vec![hotkey("Alt+Z")], vec![hotkey("Ctrl+F11")]));
    c.on_key(vk::LMENU, true);
    assert_eq!(
        c.on_key(Z, true),
        Some(HotkeyMatch {
            action: HotkeyAction::ToggleMode,
            consume: true,
        })
    );
    up(&mut c, &[Z, vk::LMENU]);
    c.on_key(vk::RCONTROL, true);
    assert_eq!(c.on_key(F11, true), HARD_RESET);
    assert_eq!(c.on_key(vk::F12, true), None, "defaults are replaced");
}

#[test]
fn the_first_configured_action_wins_for_a_duplicate_combination() {
    let combo = hotkey("Ctrl+Shift+Alt+F12");
    let mut c = HotkeyClassifier::new(&config(vec![combo], vec![combo]));
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]);
    assert_eq!(
        c.on_key(vk::F12, true),
        Some(HotkeyMatch {
            action: HotkeyAction::ToggleMode,
            consume: true,
        })
    );
}

#[test]
fn hotkeys_the_parser_would_reject_are_ignored() {
    let bare = Hotkey {
        ctrl: false,
        shift: false,
        alt: false,
        win: false,
        key: HotkeyKey::VirtualKey(A),
    };
    let modifier_as_key = Hotkey {
        ctrl: true,
        shift: false,
        alt: false,
        win: false,
        key: HotkeyKey::VirtualKey(vk::LSHIFT),
    };
    let out_of_range = Hotkey {
        key: HotkeyKey::VirtualKey(0x1_0041),
        ..modifier_as_key
    };
    let zero = Hotkey {
        key: HotkeyKey::VirtualKey(0),
        ..modifier_as_key
    };
    let mut c = HotkeyClassifier::new(&config(
        vec![bare, modifier_as_key, out_of_range, zero],
        Vec::new(),
    ));
    assert_eq!(c.on_key(A, true), None, "a bare key never steals typing");
    c.on_key(A, false);
    c.on_key(vk::LCONTROL, true);
    assert_eq!(c.on_key(vk::LSHIFT, true), None);
    assert_eq!(c.on_key(0x1_0041, true), None);
    assert_eq!(c.on_key(0, true), None);
}

#[test]
fn reset_forgets_held_modifiers() {
    let mut c = defaults();
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]);
    c.reset();
    assert_eq!(c.on_key(vk::F12, true), None);
    c.on_key(vk::F12, false);
    up(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]);
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]);
    assert_eq!(c.on_key(vk::F12, true), HARD_RESET);
}

const TOGGLE: Option<HotkeyMatch> = Some(HotkeyMatch {
    action: HotkeyAction::ToggleMode,
    consume: false,
});

/// Matrix #24: a `Ctrl+Shift` tap toggles the mode on the release of the last modifier, and the
/// release still reaches the application.
#[test]
fn b24_ctrl_shift_tap_toggles_on_the_last_release() {
    let mut c = defaults();
    assert_silent(&down(&mut c, &[vk::LCONTROL, vk::LSHIFT]));
    assert_eq!(c.on_key(vk::LSHIFT, false), None);
    assert_eq!(c.on_key(vk::LCONTROL, false), TOGGLE);
}

#[test]
fn b24_toggle_accepts_any_order_and_generic_or_side_codes() {
    let pairs = [
        (vk::LCONTROL, vk::LSHIFT),
        (vk::LSHIFT, vk::LCONTROL),
        (vk::CONTROL, vk::SHIFT),
        (vk::SHIFT, vk::CONTROL),
        (vk::RCONTROL, vk::RSHIFT),
        (vk::LCONTROL, vk::RSHIFT),
    ];
    for (first, second) in pairs {
        for first_up_first in [true, false] {
            let mut c = defaults();
            assert_silent(&down(&mut c, &[first, second]));
            let order = if first_up_first {
                [first, second]
            } else {
                [second, first]
            };
            assert_eq!(c.on_key(order[0], false), None);
            assert_eq!(c.on_key(order[1], false), TOGGLE, "{first:#x} {second:#x}");
        }
    }
}

#[test]
fn b24_modifier_auto_repeat_toggles_once() {
    let mut c = defaults();
    assert_silent(&down(
        &mut c,
        &[
            vk::LCONTROL,
            vk::LCONTROL,
            vk::LSHIFT,
            vk::LSHIFT,
            vk::LSHIFT,
        ],
    ));
    assert_eq!(c.on_key(vk::LSHIFT, false), None);
    assert_eq!(c.on_key(vk::LCONTROL, false), TOGGLE);
    assert_eq!(c.on_key(vk::LCONTROL, false), None, "stray release");
}

#[test]
fn b24_every_tap_toggles() {
    let mut c = defaults();
    for _ in 0..3 {
        down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
        assert_eq!(up(&mut c, &[vk::LCONTROL, vk::LSHIFT]), [None, TOGGLE]);
    }
}

#[test]
fn b24_no_toggle_when_another_key_goes_down_during_the_hold() {
    let mut c = defaults();
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT, A]);
    assert_silent(&up(&mut c, &[A, vk::LSHIFT, vk::LCONTROL]));

    down(&mut c, &[vk::LCONTROL, A]);
    up(&mut c, &[A]);
    down(&mut c, &[vk::LSHIFT]);
    assert_silent(&up(&mut c, &[vk::LSHIFT, vk::LCONTROL]));

    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    assert_eq!(up(&mut c, &[vk::LSHIFT, vk::LCONTROL]), [None, TOGGLE]);
}

#[test]
fn b24_no_toggle_when_an_extra_modifier_was_held() {
    for extra in [vk::LMENU, vk::RMENU, vk::MENU, vk::LWIN, vk::RWIN] {
        let mut c = defaults();
        down(&mut c, &[vk::LCONTROL, vk::LSHIFT, extra]);
        assert_silent(&up(&mut c, &[extra, vk::LSHIFT, vk::LCONTROL]));

        down(&mut c, &[vk::LCONTROL, extra]);
        c.on_key(extra, false);
        c.on_key(vk::LSHIFT, true);
        assert_silent(&up(&mut c, &[vk::LSHIFT, vk::LCONTROL]));
    }
}

#[test]
fn b24_single_modifier_taps_and_separate_taps_do_nothing() {
    let mut c = defaults();
    for key in [vk::LCONTROL, vk::LSHIFT, vk::CONTROL, vk::SHIFT] {
        c.on_key(key, true);
        assert_eq!(c.on_key(key, false), None);
    }
    c.on_key(vk::LCONTROL, true);
    c.on_key(vk::LCONTROL, false);
    c.on_key(vk::LSHIFT, true);
    assert_eq!(c.on_key(vk::LSHIFT, false), None);
}

#[test]
fn modifier_only_hotkeys_need_all_modifiers_held_together() {
    let mut c = HotkeyClassifier::new(&config(vec![hotkey("Ctrl+Shift+Alt")], Vec::new()));
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    c.on_key(vk::LCONTROL, false);
    c.on_key(vk::LMENU, true);
    assert_silent(&up(&mut c, &[vk::LSHIFT, vk::LMENU]));

    down(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]);
    assert_eq!(
        up(&mut c, &[vk::LCONTROL, vk::LMENU, vk::LSHIFT]),
        [None, None, TOGGLE]
    );
}

#[test]
fn a_key_released_during_the_hold_does_not_cancel_the_toggle() {
    let mut c = defaults();
    c.on_key(A, true);
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    c.on_key(A, false);
    assert_eq!(up(&mut c, &[vk::LSHIFT, vk::LCONTROL]), [None, TOGGLE]);
}

#[test]
fn reset_during_the_hold_cancels_the_toggle() {
    let mut c = defaults();
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    c.reset();
    assert_silent(&up(&mut c, &[vk::LSHIFT, vk::LCONTROL]));
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    assert_eq!(up(&mut c, &[vk::LSHIFT, vk::LCONTROL]), [None, TOGGLE]);
}

#[test]
fn interrupt_cancels_the_toggle_but_keeps_tracking_modifiers() {
    let mut c = defaults();
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    c.interrupt();
    assert_silent(&up(&mut c, &[vk::LSHIFT, vk::LCONTROL]));

    down(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]);
    c.interrupt();
    assert_eq!(c.on_key(vk::F12, true), HARD_RESET);
    up(&mut c, &[vk::F12, vk::LMENU, vk::LSHIFT, vk::LCONTROL]);

    c.interrupt();
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    assert_eq!(
        up(&mut c, &[vk::LSHIFT, vk::LCONTROL]),
        [None, TOGGLE],
        "an interrupt with nothing held does not affect the next hold"
    );
}

#[test]
fn a_key_hotkey_inside_the_hold_suppresses_the_modifier_only_hotkey() {
    let mut c = HotkeyClassifier::new(&config(
        vec![hotkey("Ctrl+Shift")],
        vec![hotkey("Ctrl+Shift+F12")],
    ));
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    assert_eq!(c.on_key(vk::F12, true), HARD_RESET);
    assert_silent(&up(&mut c, &[vk::F12, vk::LSHIFT, vk::LCONTROL]));
}

#[test]
fn modifier_only_hotkeys_can_be_reconfigured() {
    let mut c = HotkeyClassifier::new(&config(vec![hotkey("Ctrl+Alt")], vec![hotkey("Shift+Win")]));
    down(&mut c, &[vk::LCONTROL, vk::LMENU]);
    assert_eq!(up(&mut c, &[vk::LMENU, vk::LCONTROL]), [None, TOGGLE]);
    down(&mut c, &[vk::RWIN, vk::RSHIFT]);
    assert_eq!(
        up(&mut c, &[vk::RSHIFT, vk::RWIN]),
        [
            None,
            Some(HotkeyMatch {
                action: HotkeyAction::HardReset,
                consume: false,
            })
        ]
    );
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    assert_silent(&up(&mut c, &[vk::LSHIFT, vk::LCONTROL]));
}

#[test]
fn stray_key_ups_are_ignored() {
    let mut c = defaults();
    assert_silent(&up(
        &mut c,
        &[vk::LCONTROL, vk::SHIFT, vk::F12, A, vk::RWIN],
    ));
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    assert_eq!(
        c.on_key(vk::RSHIFT, false),
        None,
        "the other side was not down"
    );
    assert_eq!(up(&mut c, &[vk::LSHIFT, vk::LCONTROL]), [None, TOGGLE]);
}

#[test]
fn mixed_generic_and_side_codes_do_not_leave_a_modifier_stuck() {
    let mut c = defaults();
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    assert_eq!(up(&mut c, &[vk::CONTROL, vk::LSHIFT]), [None, TOGGLE]);

    down(&mut c, &[vk::CONTROL, vk::SHIFT]);
    assert_eq!(up(&mut c, &[vk::LCONTROL, vk::RSHIFT]), [None, TOGGLE]);

    c.on_key(vk::LMENU, true);
    assert_eq!(c.on_key(vk::F12, true), None, "no Ctrl or Shift left held");
}

#[test]
fn default_classifier_uses_the_default_hotkeys() {
    let mut c = HotkeyClassifier::default();
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT]);
    assert_eq!(up(&mut c, &[vk::LSHIFT, vk::LCONTROL]), [None, TOGGLE]);
    down(&mut c, &[vk::LCONTROL, vk::LSHIFT, vk::LMENU]);
    assert_eq!(c.on_key(vk::SPACE, true), HARD_RESET);
}
