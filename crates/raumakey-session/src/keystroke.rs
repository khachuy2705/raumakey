//! Key strokes as the COM layer hands them over, and their engine events.

use core::fmt;

use raumakey_model::host::HostContext;
use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::{InputEvent, Modifiers, vk};

/// Num Lock.
const VK_NUMLOCK: u32 = 0x90;
/// Scroll Lock.
const VK_SCROLL: u32 = 0x91;

/// One key transition reported by `ITfKeyEventSink`, already translated by the COM layer.
///
/// The same type describes a key-down and a key-up; the core method called says which.
/// `Debug` shows only whether the key was injected and its document: the key and its text
/// are what the user typed.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyStroke {
    /// Virtual-key code (`wParam`).
    pub virtual_key: u32,
    /// Hardware scan code (bits 16–23 of `lParam`).
    pub scan_code: u32,
    /// Extended-key flag (bit 24 of `lParam`); tells right Alt and right Ctrl from left.
    pub extended: bool,
    /// Auto-repeat: the key was already down before this key-down (bit 30 of `lParam`).
    /// Set on every key-up as well. The engine event of a key-down carries it as
    /// `InputEvent::repeat`.
    pub repeat: bool,
    /// Modifier state at the time of the key. `alt_gr` is set when the layout has AltGr and
    /// the character comes from right Alt (or Ctrl+Alt) rather than a shortcut.
    pub modifiers: Modifiers,
    /// UTF-16 text the key produces with the current layout and modifiers, as `ToUnicodeEx`
    /// reports it without changing the keyboard state; empty for keys that type nothing and
    /// for dead keys.
    pub text: Vec<u16>,
    /// The message extra info (`GetMessageExtraInfo`) as the zero-extended `ULONG_PTR` that
    /// `SendInput` stored: `lparam.0 as usize as u64`. A sign-extending `lparam.0 as u64`
    /// would turn `0xEBCA1234` into `0xFFFF_FFFF_EBCA_1234` on 32-bit Windows. Input that
    /// Raumakey injected carries [`RAUMAKEY_INJECTED_TAG`].
    pub extra_info: u64,
    /// The document that has the focus, as the COM layer identifies it.
    pub host: HostContext,
}

impl KeyStroke {
    /// True when Raumakey injected this key (matrix #39): it bypasses the core entirely.
    pub fn is_injected(&self) -> bool {
        self.extra_info == RAUMAKEY_INJECTED_TAG
    }

    /// The engine event of this key-down, or `None` for keys the engine never sees:
    /// modifiers and lock keys, which neither type text nor move the caret.
    ///
    /// Backspace, Space, Enter, and Tab are identified by their virtual key whatever text
    /// the layout reports; caret and editing keys are navigation; any other key that types
    /// text is a character; a key that types nothing (a function key, a dead key, a shortcut
    /// without text) is treated as navigation, which ends the word.
    pub(crate) fn input_event(&self) -> Option<InputEvent> {
        let virtual_key = self.virtual_key;
        if vk::is_modifier(virtual_key)
            || matches!(virtual_key, vk::CAPITAL | VK_NUMLOCK | VK_SCROLL)
        {
            return None;
        }
        let host = self.host.clone();
        let mut event = match virtual_key {
            vk::BACK => InputEvent::backspace(host),
            vk::SPACE => InputEvent::separator(' ', vk::SPACE, host),
            vk::RETURN => InputEvent::separator('\r', vk::RETURN, host),
            vk::TAB => InputEvent::separator('\t', vk::TAB, host),
            key if vk::is_navigation(key) || self.text.is_empty() => {
                InputEvent::navigation(key, host)
            }
            _ => InputEvent::text(self.text.clone(), host),
        };
        event.virtual_key = Some(virtual_key);
        event.scan_code = Some(self.scan_code);
        event.modifiers = self.modifiers;
        event.repeat = self.repeat;
        event.injected_tag = (self.extra_info != 0).then_some(self.extra_info);
        Some(event)
    }
}

impl fmt::Debug for KeyStroke {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyStroke")
            .field("injected", &self.is_injected())
            .field("document", &self.host.document_id)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use raumakey_model::host::{ExeName, HostContext, SessionId};
    use raumakey_model::input::{Modifiers, vk};

    use super::KeyStroke;

    fn stroke(virtual_key: u32, text: &[u16], repeat: bool) -> KeyStroke {
        KeyStroke {
            virtual_key,
            scan_code: 0x1E,
            extended: false,
            repeat,
            modifiers: Modifiers::default(),
            text: text.to_vec(),
            extra_info: 0,
            host: HostContext::new(
                SessionId::default(),
                ExeName::new("notepad.exe").expect("valid name"),
            ),
        }
    }

    #[test]
    fn the_event_of_an_auto_repeat_is_marked_as_a_repeat() {
        for virtual_key in [0x41, vk::BACK, vk::SPACE, vk::LEFT] {
            let repeated = stroke(virtual_key, &[0x61], true).input_event();
            assert!(
                repeated.is_some_and(|event| event.repeat),
                "{virtual_key:#x}"
            );
            let first = stroke(virtual_key, &[0x61], false).input_event();
            assert!(first.is_some_and(|event| !event.repeat), "{virtual_key:#x}");
        }
    }
}
