//! Fakes and drivers shared by the typing tests.
//!
//! [`FakePlatform`] stands in for Windows: the foreground window, a US-like keyboard layout
//! (with an optional dead key and AltGr), the asynchronous key state, the Caps Lock toggle of
//! the desktop, and `SendInput`, which only queues what it accepts.
//!
//! [`Sim`] plays the desktop around the hook, in the order Windows does it:
//!
//! 1. the hook's handler gets the event;
//! 2. an event that the hook let through reaches the system: the asynchronous key state
//!    and the Caps Lock toggle change, and the foreground application handles the key; a
//!    blocked event changes nothing;
//! 3. only then do the events that the handler sent arrive (`SendInput` appends to the input
//!    stream), each through the hook again, tagged, and on to the application.
//!
//! Each window has its own text with the caret at its end; the application types a key with
//! the real key state, not with the hook's view of it, so a hook that tracks the keys wrongly
//! shows up as a wrong text. A window may complete text inline, as a browser's address bar
//! does ([`Sim::complete_inline`]). Nothing here touches the desktop of the machine running
//! the tests.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use raumakey_controller::typing::{Foreground, LayoutInfo, Mailbox, TypingHandler, TypingPlatform};
use raumakey_model::host::ExeName;
use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::vk;
use raumakey_windows::hooks::{HookDecision, KeyboardEvent, MouseButton, MouseEvent};
use raumakey_windows::input::{SendReport, SyntheticKey};
use raumakey_windows::keyboard::{KeyboardSnapshot, Translation};

/// Notepad's main window.
pub const NOTEPAD: isize = 0x0001_0010;
/// WordPad's main window.
pub const WORDPAD: isize = 0x0002_0020;
/// An elevated command prompt.
pub const ADMIN: isize = 0x0003_0030;
/// `VK_PACKET`: how a low-level hook reports a `KEYEVENTF_UNICODE` event.
pub const VK_PACKET: u32 = 0xE7;
/// The scan code of the synthetic left Ctrl that Windows sends with AltGr.
pub const FAKE_LCTRL_SCAN: u32 = 0x21D;
/// `VK_OEM_7` (quote), a dead key when [`FakePlatform::dead_quote`] is set.
pub const VK_QUOTE: u32 = 0xDE;
/// `VK_OEM_PERIOD`.
pub const VK_PERIOD: u32 = 0xBE;
/// The layout of every fake window (US English).
pub const LAYOUT: isize = 0x0409_0409;

/// A foreground window `hwnd` of `exe`, focused on its child `hwnd + 1`, not elevated.
pub fn window(hwnd: isize, exe: &str) -> Foreground {
    let id = u32::try_from(hwnd >> 16).expect("small");
    Foreground {
        window: hwnd,
        focus: hwnd + 1,
        process_id: 100 + id,
        thread_id: 200 + id,
        executable: Some(ExeName::new(exe).expect("valid name")),
        window_class: Some("Edit".to_owned()),
        secure_field: false,
        elevated: Some(false),
    }
}

/// Notepad in the foreground.
pub fn notepad() -> Foreground {
    window(NOTEPAD, "notepad.exe")
}

/// WordPad in the foreground.
pub fn wordpad() -> Foreground {
    window(WORDPAD, "wordpad.exe")
}

/// An elevated command prompt in the foreground.
pub fn admin_prompt() -> Foreground {
    Foreground {
        elevated: Some(true),
        ..window(ADMIN, "cmd.exe")
    }
}

/// The virtual key and Shift of a character in these tests.
pub fn key_of(c: char) -> (u32, bool) {
    match c {
        ' ' => (vk::SPACE, false),
        '\r' => (vk::RETURN, false),
        '\t' => (vk::TAB, false),
        '.' => (VK_PERIOD, false),
        '\'' => (VK_QUOTE, false),
        c if c.is_ascii_uppercase() => (u32::from(c), true),
        c if c.is_ascii_alphanumeric() => (u32::from(c.to_ascii_uppercase()), false),
        _ => panic!("no key types {c:?} in these tests"),
    }
}

/// What a US layout types for `virtual_key` with `state`. With `dead_quote` the quote key is
/// a dead key (as on US-International). Right Alt is AltGr when `state.alt_gr` is set: AltGr+E
/// types the euro sign, AltGr+Q types `@`.
pub fn us_text(virtual_key: u32, state: &KeyboardSnapshot, dead_quote: bool) -> Translation {
    let text = |units: &str| Translation {
        units: units.encode_utf16().collect(),
        dead_key: false,
    };
    if state.alt_gr {
        return match virtual_key {
            0x45 => text("€"),
            0x51 => text("@"),
            _ => text(""),
        };
    }
    if state.ctrl {
        return match virtual_key {
            0x41..=0x5A => Translation {
                units: vec![u16::try_from(virtual_key - 0x40).expect("small")],
                dead_key: false,
            },
            _ => text(""),
        };
    }
    let upper = state.shift != state.caps_lock;
    match virtual_key {
        0x41..=0x5A => {
            let c = char::from_u32(virtual_key).expect("letter");
            text(&if upper { c } else { c.to_ascii_lowercase() }.to_string())
        }
        0x30..=0x39 if state.shift => {
            let index = usize::try_from(virtual_key - 0x30).expect("digit");
            text(&")!@#$%^&*(".chars().nth(index).expect("digit").to_string())
        }
        0x30..=0x39 => text(&char::from_u32(virtual_key).expect("digit").to_string()),
        vk::SPACE => text(" "),
        vk::RETURN => text("\r"),
        vk::TAB => text("\t"),
        vk::BACK => text("\u{8}"),
        vk::ESCAPE => text("\u{1b}"),
        VK_PERIOD => text(if state.shift { ">" } else { "." }),
        VK_QUOTE if dead_quote => Translation {
            units: vec![0xB4],
            dead_key: true,
        },
        VK_QUOTE => text(if state.shift { "\"" } else { "'" }),
        _ => text(""),
    }
}

/// Windows as the typing handler sees it.
#[derive(Debug, Default)]
pub struct FakePlatform {
    /// `raumakey.exe` runs elevated.
    pub elevated: bool,
    /// The foreground window.
    pub foreground: Option<Foreground>,
    /// The asynchronous key state: keys down as far as the system knows.
    pub async_down: BTreeSet<u32>,
    /// The Caps Lock toggle of the desktop.
    pub caps_lock: bool,
    /// The layout has AltGr.
    pub altgr_layout: bool,
    /// The quote key is a dead key.
    pub dead_quote: bool,
    /// `SendInput` accepts at most this many events per call.
    pub send_limit: Option<usize>,
    /// The keys of every `send` call, as requested.
    pub sends: Vec<Vec<SyntheticKey>>,
    /// The keys `send` accepted that the system has not delivered yet.
    pub queued: Vec<SyntheticKey>,
    /// How often the foreground was read.
    pub foreground_reads: usize,
    /// How often a layout was described.
    pub layout_descriptions: usize,
    /// How often the Caps Lock toggle of the desktop was read.
    pub caps_reads: usize,
    /// The keyboard layout of the foreground thread; 0 stands for [`LAYOUT`].
    pub layout: isize,
}

impl FakePlatform {
    /// Notepad in the foreground, nothing held, Caps Lock off.
    pub fn notepad() -> Self {
        Self {
            foreground: Some(notepad()),
            ..Self::default()
        }
    }

    /// The layout of the foreground thread.
    pub fn layout(&self) -> isize {
        if self.layout == 0 {
            LAYOUT
        } else {
            self.layout
        }
    }

    /// The key state the application translates keys with: the real one.
    pub fn app_state(&self) -> KeyboardSnapshot {
        let down = |keys: &[u32]| keys.iter().any(|key| self.async_down.contains(key));
        let right_alt = down(&[vk::RMENU]);
        KeyboardSnapshot {
            shift: down(&[vk::LSHIFT, vk::RSHIFT, vk::SHIFT]),
            ctrl: down(&[vk::LCONTROL, vk::RCONTROL, vk::CONTROL])
                && !(right_alt && self.altgr_layout),
            alt: down(&[vk::LMENU, vk::MENU]) || (right_alt && !self.altgr_layout),
            alt_gr: right_alt && self.altgr_layout,
            win: down(&[vk::LWIN, vk::RWIN]),
            caps_lock: self.caps_lock,
        }
    }
}

impl TypingPlatform for FakePlatform {
    fn process_is_elevated(&mut self) -> bool {
        self.elevated
    }

    fn foreground(&mut self) -> Option<Foreground> {
        self.foreground_reads += 1;
        self.foreground.clone()
    }

    fn keyboard_layout(&mut self, _thread_id: u32) -> isize {
        self.layout()
    }

    fn describe_layout(&mut self, layout: isize) -> LayoutInfo {
        self.layout_descriptions += 1;
        LayoutInfo {
            name: format!("{:08X}", (layout as usize as u32) & 0xFFFF),
            altgr: self.altgr_layout,
        }
    }

    fn translate(
        &mut self,
        virtual_key: u32,
        _scan_code: u32,
        state: &KeyboardSnapshot,
        layout: isize,
    ) -> Translation {
        assert_eq!(layout, LAYOUT, "only the US layout translates keys here");
        us_text(virtual_key, state, self.dead_quote)
    }

    fn key_is_down(&mut self, virtual_key: u32) -> bool {
        self.async_down.contains(&virtual_key)
    }

    fn caps_lock_on(&mut self) -> bool {
        self.caps_reads += 1;
        self.caps_lock
    }

    fn send(&mut self, keys: &[SyntheticKey]) -> SendReport {
        self.sends.push(keys.to_vec());
        let sent = self
            .send_limit
            .map_or(keys.len(), |limit| limit.min(keys.len()));
        self.queued.extend_from_slice(&keys[..sent]);
        SendReport {
            requested: keys.len(),
            sent,
        }
    }
}

/// The application of a window: its text, caret at the end, and a pending dead key.
///
/// An application that completes text inline (a browser's address bar, Excel) has
/// `completions`: after a typed character, when the text is the start of one of them, the
/// rest of the first such completion follows the caret, selected. Typing replaces the
/// selection; a Backspace deletes only the selection when there is one, and no completion
/// follows a deletion.
#[derive(Debug, Default)]
struct App {
    text: Vec<u16>,
    dead: Option<u16>,
    completions: Vec<Vec<u16>>,
    selected: Vec<u16>,
}

impl App {
    fn type_units(&mut self, units: &[u16]) {
        self.selected.clear();
        self.insert(units);
        self.complete();
    }

    fn insert(&mut self, units: &[u16]) {
        let Some(dead) = self.dead.take() else {
            self.text.extend_from_slice(units);
            return;
        };
        let composed = match units {
            [0x61] => Some(0xE1),
            [0x65] => Some(0xE9),
            _ => None,
        };
        match composed {
            Some(unit) => self.text.push(unit),
            None => {
                self.text.push(dead);
                self.text.extend_from_slice(units);
            }
        }
    }

    /// Selects the rest of the first completion that starts with the text.
    fn complete(&mut self) {
        let text = &self.text;
        if let Some(completion) = self
            .completions
            .iter()
            .find(|completion| completion.len() > text.len() && completion.starts_with(text))
        {
            self.selected = completion[text.len()..].to_vec();
        }
    }

    fn backspace(&mut self) {
        if !self.selected.is_empty() {
            self.selected.clear();
            return;
        }
        if self.dead.take().is_some() {
            return;
        }
        let len = raumakey_model::utf16::last_scalar_len(&self.text);
        self.text.truncate(self.text.len() - len);
    }
}

/// The desktop around a [`TypingHandler`] (see the module documentation).
pub struct Sim {
    /// The handler under test.
    pub handler: TypingHandler<FakePlatform>,
    /// Its mailbox.
    pub mailbox: Arc<Mailbox>,
    modes: Arc<Mutex<Vec<bool>>>,
    apps: BTreeMap<isize, App>,
    time: u32,
    /// The workstation is locked: the secure desktop takes the input, the hook is not
    /// called, and no application types.
    locked: bool,
    /// Windows calls the hook for the keys of an elevated window too (by default it does
    /// not, as for a hook of a process that is not elevated).
    pub hook_sees_elevated: bool,
}

impl Sim {
    /// A handler over `platform`, with a mailbox and an observer that records the modes.
    pub fn new(platform: FakePlatform) -> Self {
        Self::with_mailbox(platform, Arc::new(Mailbox::new()))
    }

    /// A handler over `platform` with `mailbox`, whose posts it applies when it is built.
    pub fn with_mailbox(platform: FakePlatform, mailbox: Arc<Mailbox>) -> Self {
        let modes = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&modes);
        let handler = TypingHandler::new(platform, Arc::clone(&mailbox), move |mode| {
            seen.lock().expect("not poisoned").push(mode);
        });
        Self {
            handler,
            mailbox,
            modes,
            apps: BTreeMap::new(),
            time: 1000,
            locked: false,
            hook_sees_elevated: false,
        }
    }

    /// Whether Windows calls the hook now: not while the workstation is locked (or the
    /// secure desktop has the input), and not for an elevated foreground window when
    /// `raumakey.exe` is not elevated (unless [`hook_sees_elevated`](Self::hook_sees_elevated)).
    pub fn hook_called(&mut self) -> bool {
        let (locked, sees_elevated) = (self.locked, self.hook_sees_elevated);
        let platform = self.platform();
        let elevated_foreground = platform
            .foreground
            .as_ref()
            .is_some_and(|foreground| foreground.elevated == Some(true));
        !locked && (sees_elevated || platform.elevated || !elevated_foreground)
    }

    /// Locks the workstation (Win+L): from now on the hook is not called.
    pub fn lock(&mut self) {
        self.locked = true;
    }

    /// Unlocks the workstation.
    pub fn unlock(&mut self) {
        self.locked = false;
    }

    /// Lets `milliseconds` pass without any input.
    pub fn wait(&mut self, milliseconds: u32) {
        self.time += milliseconds;
    }

    /// A key transition that the system sees but the hook does not (it happened on the
    /// secure desktop, or in a window whose input the hook does not get).
    pub fn unseen(&mut self, virtual_key: u32, down: bool) {
        let event = self.physical(virtual_key, scan_of(virtual_key), down);
        self.reach_system(&event);
    }

    /// Notepad in the foreground.
    pub fn notepad() -> Self {
        Self::new(FakePlatform::notepad())
    }

    /// The fake Windows.
    pub fn platform(&mut self) -> &mut FakePlatform {
        self.handler.platform_mut()
    }

    /// The text that window `hwnd` shows, a selected completion included.
    pub fn text(&self, hwnd: isize) -> String {
        self.apps
            .get(&hwnd)
            .map(|app| String::from_utf16_lossy(&[&app.text[..], &app.selected[..]].concat()))
            .unwrap_or_default()
    }

    /// Window `hwnd` completes text inline with `completions`, as a browser's address bar
    /// completes the addresses of its history.
    pub fn complete_inline(&mut self, hwnd: isize, completions: &[&str]) {
        self.apps.entry(hwnd).or_default().completions = completions
            .iter()
            .map(|completion| completion.encode_utf16().collect())
            .collect();
    }

    /// The modes the observer was told, in order.
    pub fn modes(&self) -> Vec<bool> {
        self.modes.lock().expect("not poisoned").clone()
    }

    /// Makes `foreground` the foreground window (no event reaches the hook).
    pub fn focus(&mut self, foreground: Option<Foreground>) {
        self.platform().foreground = foreground;
    }

    /// A physical key event, as `WH_KEYBOARD_LL` reports it.
    pub fn physical(&mut self, virtual_key: u32, scan: u32, down: bool) -> KeyboardEvent {
        self.time += 10;
        KeyboardEvent {
            vk: virtual_key,
            scan,
            extended: matches!(virtual_key, vk::RCONTROL | vk::RMENU | 0x21..=0x2E),
            injected: false,
            injected_lower_il: false,
            alt_down: false,
            key_up: !down,
            time: self.time,
            extra_info: 0,
        }
    }

    /// Runs `event` through the hook and the desktop; returns the hook's decision. When the
    /// hook is not called ([`hook_called`](Self::hook_called)), the event goes straight to
    /// the system and the decision is `PassThrough`.
    pub fn event(&mut self, event: KeyboardEvent) -> HookDecision {
        if !self.hook_called() {
            self.reach_system(&event);
            return HookDecision::PassThrough;
        }
        let decision = self.handler.on_key(&event);
        if decision == HookDecision::PassThrough {
            self.reach_system(&event);
        }
        let injected = std::mem::take(&mut self.platform().queued);
        for key in injected {
            assert_eq!(
                key.extra_info as u64, RAUMAKEY_INJECTED_TAG,
                "every key is tagged"
            );
            self.time += 1;
            let event = KeyboardEvent {
                vk: if key.unicode {
                    VK_PACKET
                } else {
                    u32::from(key.virtual_key)
                },
                scan: u32::from(key.scan_code),
                extended: key.extended,
                injected: true,
                injected_lower_il: false,
                alt_down: false,
                key_up: key.key_up,
                time: self.time,
                extra_info: key.extra_info,
            };
            assert_eq!(
                self.handler.on_key(&event),
                HookDecision::PassThrough,
                "Raumakey's own keys pass the hook"
            );
            self.reach_system(&event);
        }
        decision
    }

    /// The system handles an event that passed the hook: the key state, then the
    /// foreground application.
    fn reach_system(&mut self, event: &KeyboardEvent) {
        let platform = self.handler.platform_mut();
        if event.key_up {
            platform.async_down.remove(&event.vk);
            return;
        }
        if event.vk != VK_PACKET {
            let first = platform.async_down.insert(event.vk);
            if event.vk == vk::CAPITAL && first {
                platform.caps_lock = !platform.caps_lock;
            }
        }
        if self.locked {
            return;
        }
        let platform = self.handler.platform_mut();
        let Some(hwnd) = platform
            .foreground
            .as_ref()
            .map(|foreground| foreground.window)
        else {
            return;
        };
        let state = platform.app_state();
        let dead_quote = platform.dead_quote;
        let app = self.apps.entry(hwnd).or_default();
        if event.vk == VK_PACKET {
            app.type_units(&[u16::try_from(event.scan).expect("a code unit")]);
            return;
        }
        if vk::is_modifier(event.vk) || (state.ctrl || state.alt || state.win) {
            return;
        }
        if event.vk == vk::BACK {
            app.backspace();
            return;
        }
        let translation = us_text(event.vk, &state, dead_quote);
        if translation.dead_key {
            app.dead = translation.units.first().copied();
        } else if !translation.units.is_empty() {
            app.type_units(&translation.units);
        }
    }

    /// A key-down of `virtual_key`; returns the hook's decision.
    pub fn down(&mut self, virtual_key: u32) -> HookDecision {
        let event = self.physical(virtual_key, scan_of(virtual_key), true);
        self.event(event)
    }

    /// A key-up of `virtual_key`; returns the hook's decision.
    pub fn up(&mut self, virtual_key: u32) -> HookDecision {
        let event = self.physical(virtual_key, scan_of(virtual_key), false);
        self.event(event)
    }

    /// Presses and releases `virtual_key`; returns the decision of the key-down.
    pub fn press(&mut self, virtual_key: u32) -> HookDecision {
        let decision = self.down(virtual_key);
        self.up(virtual_key);
        decision
    }

    /// Types `c`, holding left Shift for an uppercase letter.
    pub fn type_char(&mut self, c: char) -> HookDecision {
        let (virtual_key, shift) = key_of(c);
        if shift {
            self.down(vk::LSHIFT);
        }
        let decision = self.press(virtual_key);
        if shift {
            self.up(vk::LSHIFT);
        }
        decision
    }

    /// Types every character of `text`.
    pub fn type_text(&mut self, text: &str) -> Vec<HookDecision> {
        text.chars().map(|c| self.type_char(c)).collect()
    }

    /// Holds `modifiers` while pressing `virtual_key`, then releases them in reverse order.
    /// Returns the decisions of the key-down and the key-up of `virtual_key`.
    pub fn chord(&mut self, modifiers: &[u32], virtual_key: u32) -> (HookDecision, HookDecision) {
        for &modifier in modifiers {
            assert_eq!(self.down(modifier), HookDecision::PassThrough);
        }
        let down = self.down(virtual_key);
        let up = self.up(virtual_key);
        for &modifier in modifiers.iter().rev() {
            assert_eq!(self.up(modifier), HookDecision::PassThrough);
        }
        (down, up)
    }

    /// Taps Ctrl+Shift with the left keys: Ctrl down, Shift down, Shift up, Ctrl up.
    pub fn tap_ctrl_shift(&mut self) {
        for (key, down) in [
            (vk::LCONTROL, true),
            (vk::LSHIFT, true),
            (vk::LSHIFT, false),
            (vk::LCONTROL, false),
        ] {
            let decision = if down { self.down(key) } else { self.up(key) };
            assert_eq!(decision, HookDecision::PassThrough, "modifiers always pass");
        }
    }

    /// A left click in the foreground window.
    pub fn click(&mut self) {
        self.time += 10;
        let event = MouseEvent {
            button: MouseButton::Left,
            injected: false,
            injected_lower_il: false,
            time: self.time,
            extra_info: 0,
        };
        if self.hook_called() {
            self.handler.on_mouse(&event);
        }
    }

    /// `virtual_key` goes up while the hook does not see it (the secure desktop or an
    /// elevated window had the input).
    pub fn lose_release(&mut self, virtual_key: u32) {
        self.platform().async_down.remove(&virtual_key);
    }

    /// Every key sent so far, in order.
    pub fn sent(&mut self) -> Vec<SyntheticKey> {
        self.platform().sends.concat()
    }
}

/// The scan code of a key in these tests (set 1 for the keys that have one here).
pub fn scan_of(virtual_key: u32) -> u32 {
    match virtual_key {
        vk::BACK => 0x0E,
        vk::TAB => 0x0F,
        vk::RETURN => 0x1C,
        vk::LCONTROL | vk::RCONTROL => 0x1D,
        vk::LSHIFT => 0x2A,
        vk::RSHIFT => 0x36,
        vk::LMENU | vk::RMENU => 0x38,
        vk::SPACE => 0x39,
        vk::CAPITAL => 0x3A,
        vk::F12 => 0x58,
        _ => virtual_key & 0x7F,
    }
}

/// The down and up events of Backspace, as the handler sends them.
pub fn backspace_keys() -> [SyntheticKey; 2] {
    let down = SyntheticKey {
        virtual_key: 0x08,
        scan_code: 0x0E,
        key_up: false,
        unicode: false,
        extended: false,
        extra_info: RAUMAKEY_INJECTED_TAG as usize,
    };
    [
        down,
        SyntheticKey {
            key_up: true,
            ..down
        },
    ]
}

/// The down and up events that type `unit`.
pub fn unicode_keys(unit: u16) -> [SyntheticKey; 2] {
    let down = SyntheticKey {
        virtual_key: 0,
        scan_code: unit,
        key_up: false,
        unicode: true,
        extended: false,
        extra_info: RAUMAKEY_INJECTED_TAG as usize,
    };
    [
        down,
        SyntheticKey {
            key_up: true,
            ..down
        },
    ]
}
