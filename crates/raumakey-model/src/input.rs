//! Normalized input events (clean-room spec §3.1).

use crate::error::EngineError;
use crate::host::HostContext;
use crate::identity::RAUMAKEY_INJECTED_TAG;
use crate::utf16;

/// Maximum UTF-16 units carried by one event.
pub const MAX_EVENT_TEXT_UNITS: usize = 16;

/// Kind of a normalized input event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputKind {
    /// A key that produces text.
    Character,
    /// Backspace.
    Backspace,
    /// Space, tab, or enter.
    Separator,
    /// Caret movement or editing key that invalidates the context.
    Navigation,
    /// Mouse click in the document.
    MouseReset,
    /// Focus change.
    FocusReset,
    /// Vietnamese/English toggle.
    ModeToggle,
    /// Configuration changed.
    ConfigChanged,
    /// Component shutdown.
    Shutdown,
}

/// Modifier state at the time of the event. Shift and Caps Lock are reported separately.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    /// Shift held.
    pub shift: bool,
    /// Caps Lock toggled on.
    pub caps_lock: bool,
    /// Ctrl held.
    pub ctrl: bool,
    /// Alt held.
    pub alt: bool,
    /// The character is produced by AltGr (Ctrl+Alt on AltGr layouts) and is not a shortcut.
    pub alt_gr: bool,
    /// Windows key held.
    pub win: bool,
}

/// A normalized input event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputEvent {
    /// Event kind.
    pub kind: InputKind,
    /// Produced text; empty except for `Character` and `Separator`.
    pub text_utf16: Vec<u16>,
    /// Virtual key code, when the event came from a key.
    pub virtual_key: Option<u32>,
    /// Hardware scan code, when known.
    pub scan_code: Option<u32>,
    /// Modifier state.
    pub modifiers: Modifiers,
    /// Auto-repeat.
    pub repeat: bool,
    /// Extra-info tag of injected input.
    pub injected_tag: Option<u64>,
    /// Monotonic timestamp in milliseconds.
    pub monotonic_time_ms: u64,
    /// Host that produced the event.
    pub host: HostContext,
}

impl InputEvent {
    /// An event of `kind` with no text and no key.
    pub fn simple(kind: InputKind, host: HostContext) -> Self {
        Self {
            kind,
            text_utf16: Vec::new(),
            virtual_key: None,
            scan_code: None,
            modifiers: Modifiers::default(),
            repeat: false,
            injected_tag: None,
            monotonic_time_ms: 0,
            host,
        }
    }

    /// A `Character` event for one scalar.
    pub fn character(c: char, host: HostContext) -> Self {
        let mut buffer = [0u16; 2];
        Self::text(c.encode_utf16(&mut buffer).to_vec(), host)
    }

    /// A `Character` event with arbitrary (possibly invalid) UTF-16, for adapters and tests.
    pub fn text(units: Vec<u16>, host: HostContext) -> Self {
        let mut event = Self::simple(InputKind::Character, host);
        event.text_utf16 = units;
        event
    }

    /// A `Separator` event (space, tab, enter) with its virtual key.
    pub fn separator(c: char, virtual_key: u32, host: HostContext) -> Self {
        let mut event = Self::character(c, host);
        event.kind = InputKind::Separator;
        event.virtual_key = Some(virtual_key);
        event
    }

    /// A `Backspace` event.
    pub fn backspace(host: HostContext) -> Self {
        let mut event = Self::simple(InputKind::Backspace, host);
        event.virtual_key = Some(vk::BACK);
        event
    }

    /// A `Navigation` event for `virtual_key`.
    pub fn navigation(virtual_key: u32, host: HostContext) -> Self {
        let mut event = Self::simple(InputKind::Navigation, host);
        event.virtual_key = Some(virtual_key);
        event
    }

    /// Replaces the modifier state.
    pub fn with_modifiers(mut self, modifiers: Modifiers) -> Self {
        self.modifiers = modifiers;
        self
    }

    /// Sets the injected extra-info tag.
    pub fn with_injected_tag(mut self, tag: u64) -> Self {
        self.injected_tag = Some(tag);
        self
    }

    /// Validates the event shape.
    pub fn validate(&self) -> Result<(), EngineError> {
        let carries_text = matches!(self.kind, InputKind::Character | InputKind::Separator);
        if !carries_text && !self.text_utf16.is_empty() {
            return Err(EngineError::InternalInvariant);
        }
        if self.text_utf16.len() > MAX_EVENT_TEXT_UNITS {
            return Err(EngineError::CapacityExceeded);
        }
        utf16::validate(&self.text_utf16).map_err(|_| EngineError::InvalidUtf16)
    }

    /// True for Ctrl/Alt/Win combinations that are shortcuts rather than text (AltGr excluded).
    pub fn is_shortcut(&self) -> bool {
        let m = self.modifiers;
        (m.ctrl || m.alt || m.win) && !m.alt_gr
    }

    /// True when the event was injected by Raumakey itself.
    pub fn is_raumakey_injected(&self) -> bool {
        self.injected_tag == Some(RAUMAKEY_INJECTED_TAG)
    }
}

/// Modifiers to hold while replaying a key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ModifierSet {
    /// Shift.
    pub shift: bool,
    /// Ctrl.
    pub ctrl: bool,
    /// Alt.
    pub alt: bool,
    /// Windows key.
    pub win: bool,
}

/// A key that the adapter must replay after applying a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReplayKey {
    /// Virtual key code.
    pub virtual_key: u32,
    /// Scan code, when known.
    pub scan_code: Option<u32>,
    /// Modifiers held during replay.
    pub modifiers: ModifierSet,
}

/// Virtual-key numbers used by platform-independent code. These are plain numbers, not API calls.
pub mod vk {
    /// Backspace.
    pub const BACK: u32 = 0x08;
    /// Tab.
    pub const TAB: u32 = 0x09;
    /// Enter.
    pub const RETURN: u32 = 0x0D;
    /// Shift.
    pub const SHIFT: u32 = 0x10;
    /// Ctrl.
    pub const CONTROL: u32 = 0x11;
    /// Alt.
    pub const MENU: u32 = 0x12;
    /// Pause.
    pub const PAUSE: u32 = 0x13;
    /// Caps Lock.
    pub const CAPITAL: u32 = 0x14;
    /// Escape.
    pub const ESCAPE: u32 = 0x1B;
    /// Space.
    pub const SPACE: u32 = 0x20;
    /// Page Up.
    pub const PRIOR: u32 = 0x21;
    /// Page Down.
    pub const NEXT: u32 = 0x22;
    /// End.
    pub const END: u32 = 0x23;
    /// Home.
    pub const HOME: u32 = 0x24;
    /// Left arrow.
    pub const LEFT: u32 = 0x25;
    /// Up arrow.
    pub const UP: u32 = 0x26;
    /// Right arrow.
    pub const RIGHT: u32 = 0x27;
    /// Down arrow.
    pub const DOWN: u32 = 0x28;
    /// Insert.
    pub const INSERT: u32 = 0x2D;
    /// Delete.
    pub const DELETE: u32 = 0x2E;
    /// Left Windows key.
    pub const LWIN: u32 = 0x5B;
    /// Right Windows key.
    pub const RWIN: u32 = 0x5C;
    /// F1.
    pub const F1: u32 = 0x70;
    /// F12.
    pub const F12: u32 = 0x7B;
    /// F24.
    pub const F24: u32 = 0x87;
    /// Left Shift.
    pub const LSHIFT: u32 = 0xA0;
    /// Right Shift.
    pub const RSHIFT: u32 = 0xA1;
    /// Left Ctrl.
    pub const LCONTROL: u32 = 0xA2;
    /// Right Ctrl.
    pub const RCONTROL: u32 = 0xA3;
    /// Left Alt.
    pub const LMENU: u32 = 0xA4;
    /// Right Alt (AltGr on AltGr layouts).
    pub const RMENU: u32 = 0xA5;

    /// Keys that move the caret or edit outside the current word.
    pub const fn is_navigation(vk: u32) -> bool {
        matches!(vk, PRIOR..=DOWN | INSERT | DELETE | ESCAPE)
    }

    /// Modifier keys (generic and side-specific).
    pub const fn is_modifier(vk: u32) -> bool {
        matches!(
            vk,
            SHIFT
                | CONTROL
                | MENU
                | LWIN
                | RWIN
                | LSHIFT
                | RSHIFT
                | LCONTROL
                | RCONTROL
                | LMENU
                | RMENU
        )
    }
}
