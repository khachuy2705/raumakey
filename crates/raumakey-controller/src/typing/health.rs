//! Whether Windows removed the typing hook.
//!
//! Windows calls a low-level hook synchronously and removes it, without telling its thread,
//! after it exceeded `LowLevelHooksTimeout` (repeatedly, on current systems). The hook thread
//! then keeps running and simply never sees another event, so typing turns plain. The run
//! mode notices it by comparing the time stamp of the last event the hook saw
//! (`raumakey_windows::hooks::Heartbeat`, beaten for every keyboard and mouse event, moves
//! included) with the last input of the session (`GetLastInputInfo`), and restarts the hook
//! (`TypingHook::ensure_alive`).

/// How long after the hook's last event the session may see input before the hook counts as
/// removed, in milliseconds. It covers the moment between the system taking an event and the
/// hook being called for it.
pub const HOOK_SILENCE_LIMIT_MS: u32 = 1000;

/// Whether the hook is gone: the session saw input (`last_input`, the tick of
/// `GetLastInputInfo`) more than [`HOOK_SILENCE_LIMIT_MS`] after the later of the hook's last
/// event (`last_event`, `None` before the first) and `reference` (when the hook was started).
///
/// Input that Windows never shows the hook does not count: when the foreground is not
/// `foreground_typable` (no foreground window, as on the lock screen, or a window with more
/// privileges than `raumakey.exe`), the answer is false. Ticks are compared modulo 2³², so
/// they may wrap (every 49.7 days); input older than the reference is no sign.
pub fn hook_lost(
    last_event: Option<u32>,
    reference: u32,
    last_input: u32,
    foreground_typable: bool,
) -> bool {
    if !foreground_typable {
        return false;
    }
    let seen = match last_event {
        Some(event) if is_later(event, reference) => event,
        _ => reference,
    };
    is_later(last_input, seen) && last_input.wrapping_sub(seen) > HOOK_SILENCE_LIMIT_MS
}

/// Whether tick `a` is later than tick `b`, modulo 2³² (at most half the range apart).
fn is_later(a: u32, b: u32) -> bool {
    let ahead = a.wrapping_sub(b);
    ahead != 0 && ahead < 1 << 31
}
