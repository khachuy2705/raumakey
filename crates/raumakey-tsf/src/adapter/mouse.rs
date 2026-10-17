//! Mouse messages that press a button (matrix #19).
//!
//! The text service's `WH_MOUSE` hook sees the mouse messages that the host's thread
//! retrieves. A button press can move the caret before the host reports the move, or without
//! the host ever reporting it, so every press, in the client or the non-client area, is
//! reported to `ServiceCore::mouse_down`. Moves, releases, and wheel turns are not.

/// True when `message` presses a mouse button: a button-down or double-click message of any
/// button, in the client or the non-client area.
pub fn is_button_press(message: u32) -> bool {
    matches!(
        message,
        // WM_LBUTTONDOWN, WM_LBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONDBLCLK,
        // WM_MBUTTONDOWN, WM_MBUTTONDBLCLK, WM_XBUTTONDOWN, WM_XBUTTONDBLCLK.
        0x0201 | 0x0203 | 0x0204 | 0x0206 | 0x0207 | 0x0209 | 0x020B | 0x020D
        // The same in the non-client area: WM_NCLBUTTONDOWN to WM_NCXBUTTONDBLCLK.
        | 0x00A1 | 0x00A3 | 0x00A4 | 0x00A6 | 0x00A7 | 0x00A9 | 0x00AB | 0x00AD
    )
}
