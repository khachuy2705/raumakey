//! The keyboard open/close compartment (`GUID_COMPARTMENT_KEYBOARD_OPENCLOSE`) and the
//! Vietnamese mode of the core, kept in step.
//!
//! The core decides the mode: defaults v1 at activation, a local hotkey toggle while
//! disconnected (D24), later the controller's snapshots. The compartment shows it to TSF, the
//! language bar, and the input indicator, which can also change it: such a change sets the
//! core's mode. Open is Vietnamese, closed is English.

/// What the compartment holds, as far as the text service knows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModeMirror {
    /// The mode the compartment shows; `None` before the first write and after a failed
    /// write or a cleared compartment.
    shown: Option<bool>,
}

impl ModeMirror {
    /// After activation and after every key call: the value to write to the compartment so
    /// that it shows `mode`, the mode of the core, or `None` when it shows it already.
    pub fn sync(&mut self, mode: bool) -> Option<bool> {
        if self.shown == Some(mode) {
            None
        } else {
            self.shown = Some(mode);
            Some(mode)
        }
    }

    /// The write that [`sync`](Self::sync) asked for failed; the next `sync` asks again.
    pub fn write_failed(&mut self) {
        self.shown = None;
    }

    /// The compartment changed (`ITfCompartmentEventSink::OnChange`) and now holds `value`
    /// ([`mode_of`]): returns the mode the core must take, if any.
    pub fn changed(&mut self, value: Option<bool>) -> Option<bool> {
        self.shown = value;
        value
    }
}

/// The compartment value (`VT_I4`) that shows `mode`.
pub fn open_close_value(mode: bool) -> i32 {
    i32::from(mode)
}

/// The mode a compartment value shows: `value` is its `VT_I4` number, `None` when the
/// compartment is empty or holds another type.
pub fn mode_of(value: Option<i32>) -> Option<bool> {
    value.map(|value| value != 0)
}
