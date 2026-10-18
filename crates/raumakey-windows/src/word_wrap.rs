//! Word wrap that breaks Vietnamese text at spaces only.
//!
//! GDI's word break (`DrawTextW` with `DT_WORDBREAK`, which multi-line labels, multi-line
//! buttons, and `MessageBoxW` use) treats the boundary before a letter outside ASCII as a
//! break opportunity, as for CJK text: a Vietnamese line that does not fit is cut inside a
//! word ("…không phải ti" / "ếng Việt"). So Raumakey wraps such texts itself, at spaces only,
//! to lines that each fit, and Windows finds nothing left to wrap.
//!
//! [`wrap_words`] is pure and takes the measure as a closure. On Windows, [`wrap_for_control`]
//! measures with a control's own font and width ([`line_width`]), which
//! [`Dialog::set_text_wrapped`](crate::dialog_box::Dialog::set_text_wrapped) uses, and
//! [`wrap_for_message_box`] with the message box font, which
//! [`message_box`](crate::dialog::message_box) uses before it shows a box.

/// `text` with line breaks added so that each line is at most `max_width` wide as `measure`
/// measures it (in any unit, such as pixels).
///
/// - Existing line breaks (`\n`, `\r\n`, a lone `\r`) stay as they are; each added break is a
///   `\n`.
/// - A line breaks only at an ASCII space (U+0020), never inside a word: a no-break space
///   (U+00A0), a tab, and every other character belong to their word. The spaces at a break
///   go, however many; spaces inside a line stay.
/// - Each line takes as many words as fit; a word wider than `max_width` (a long path) stays
///   whole, alone on its line, where Windows may still cut it.
/// - Spaces at the start of a line stay with its first word; spaces at the end of a line go.
/// - `measure` gets whole candidate lines, so kerning and overhang count as the text is drawn.
///
/// Wrapping a wrapped text again at the same width changes nothing.
pub fn wrap_words(text: &str, max_width: i32, mut measure: impl FnMut(&str) -> i32) -> String {
    let mut wrapped = String::with_capacity(text.len() + text.len() / 16);
    let mut rest = text;
    loop {
        let Some(at) = rest.find(['\r', '\n']) else {
            wrap_line(rest, max_width, &mut measure, &mut wrapped);
            return wrapped;
        };
        let break_len = if rest[at..].starts_with("\r\n") { 2 } else { 1 };
        wrap_line(&rest[..at], max_width, &mut measure, &mut wrapped);
        wrapped.push_str(&rest[at..at + break_len]);
        rest = &rest[at + break_len..];
    }
}

/// The widest line of a message box text, in pixels at 96 DPI ([`message_text_width`]).
const MESSAGE_LINE_PIXELS: i64 = 300;
/// The most average characters of the message font a message box line takes.
const MESSAGE_LINE_CHARS: i32 = 50;
/// The fewest average characters a message box line may take on a small screen.
const MESSAGE_LINE_MIN_CHARS: i32 = 25;

/// The widest a line of a message box text may be, in pixels, for a box at `dpi` (0 counts
/// as 96) on a monitor whose work area is `work_width` pixels wide (0 or less when unknown),
/// with a message font whose average character is `average_char_width` pixels wide (0 or less
/// when unknown). The least of:
///
/// - 300 pixels at 96 DPI, scaled to `dpi`. `MessageBoxW` sizes its text to the longest line
///   up to a limit and wraps longer lines: on Windows 11 at 96 DPI (a 1,296-pixel work area)
///   a box with an icon showed lines of up to 318 pixels whole and wrapped those of 321, with
///   its box 426 pixels wide at most, the same box as on a 1,280-pixel work area; a box of
///   many lines only grows wider. Its text font is one pixel smaller than the message font
///   that measures the lines here (Segoe UI -11 against -12), so 300 pixels leave about a
///   sixth of room.
/// - 50 average characters, for readability (also where the box's own limit shrinks with a
///   small font).
/// - A quarter of the work area, but at least 25 average characters: on a small screen the
///   box may have less room.
///
/// The result is at least 1.
pub fn message_text_width(work_width: i32, dpi: u32, average_char_width: i32) -> i32 {
    let dpi = if dpi == 0 { 96 } else { i64::from(dpi) };
    let pixels = i32::try_from(MESSAGE_LINE_PIXELS * dpi / 96).unwrap_or(i32::MAX);
    let average = average_char_width.max(0);
    let readable = if average > 0 {
        average.saturating_mul(MESSAGE_LINE_CHARS)
    } else {
        i32::MAX
    };
    let screen = (work_width.max(0) / 4).max(average.saturating_mul(MESSAGE_LINE_MIN_CHARS));
    pixels.min(readable).min(screen).max(1)
}

/// Appends `line` (without line breaks) to `wrapped`, broken at spaces as [`wrap_words`] says.
fn wrap_line(
    line: &str,
    max_width: i32,
    measure: &mut impl FnMut(&str) -> i32,
    wrapped: &mut String,
) {
    let line = line.trim_end_matches(' ');
    let words = line.trim_start_matches(' ');
    // The indentation stays with the first word.
    let mut current = line[..line.len() - words.len()].to_owned();
    let mut started = false;
    // Spaces between the previous word and the next one, beyond the first.
    let mut extra_spaces = 0_usize;
    for word in words.split(' ') {
        if word.is_empty() {
            extra_spaces += 1;
            continue;
        }
        if started {
            let mut candidate = current.clone();
            candidate.extend(core::iter::repeat_n(' ', extra_spaces + 1));
            candidate.push_str(word);
            if measure(&candidate) <= max_width {
                current = candidate;
            } else {
                wrapped.push_str(&current);
                wrapped.push('\n');
                current = word.to_owned();
            }
        } else {
            current.push_str(word);
            started = true;
        }
        extra_spaces = 0;
    }
    if started {
        wrapped.push_str(&current);
    }
}

#[cfg(windows)]
pub use win::{line_width, wrap_for_control, wrap_for_message_box};

#[cfg(windows)]
mod win {
    use core::ffi::c_void;
    use std::sync::OnceLock;

    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        CreateFontIndirectW, DT_CALCRECT, DT_EXPANDTABS, DT_NOPREFIX, DT_SINGLELINE, DeleteObject,
        DrawTextW, EnumDisplayMonitors, GetDC, GetDeviceCaps, GetMonitorInfoW, GetTextMetricsW,
        HDC, HFONT, HGDIOBJ, HMONITOR, LOGPIXELSX, MONITORINFO, ReleaseDC, SelectObject,
        TEXTMETRICW,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::{
        BS_3STATE, BS_AUTO3STATE, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_CHECKBOX, BS_PUSHLIKE,
        BS_RADIOBUTTON, BS_TYPEMASK, GWL_STYLE, GetClassNameW, GetClientRect, GetSystemMetrics,
        GetWindowLongW, NONCLIENTMETRICSW, SM_CXMENUCHECK, SPI_GETNONCLIENTMETRICS,
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SendMessageW, SystemParametersInfoW, WM_GETFONT,
    };
    use windows::core::{BOOL, s};

    use super::{message_text_width, wrap_words};
    use crate::error::{E_INVALIDARG, ResultExt, WinError, WinResult};
    use crate::icon::{system_metric_for_dpi, user32_export, window_dpi};
    use crate::panic::guard_bool;

    /// The gap between a check box's box and its text that [`line_width`] keeps, at 96 DPI,
    /// besides the width of the menu check mark.
    const CHECK_BOX_GAP: i32 = 6;

    /// The width that [`wrap_for_message_box`] compares every line with: each line's width on
    /// each monitor, relative to that monitor's limit.
    const RELATIVE_LIMIT: i32 = 1 << 16;

    /// What kind of text a control shows, for [`line_width`].
    enum Kind {
        /// A label (`Static`): its text takes the client width.
        Label,
        /// A check box or radio button: its box comes first.
        CheckBox,
    }

    /// The kind of `control`; fails with `E_INVALIDARG` for anything but a label, a check
    /// box, or a radio button (a push button, a group box, an edit, a list).
    fn kind(control: HWND) -> WinResult<Kind> {
        let mut class = [0_u16; 16];
        // SAFETY: the buffer is writable for its whole length; USER validates `control`.
        let length = unsafe { GetClassNameW(control, &mut class) };
        let length = usize::try_from(length).unwrap_or(0).min(class.len());
        let class = String::from_utf16_lossy(&class[..length]);
        // SAFETY: only reads the style of a window; USER validates the handle.
        let style = unsafe { GetWindowLongW(control, GWL_STYLE) };
        let check_box = matches!(
            style & BS_TYPEMASK,
            BS_CHECKBOX
                | BS_AUTOCHECKBOX
                | BS_RADIOBUTTON
                | BS_3STATE
                | BS_AUTO3STATE
                | BS_AUTORADIOBUTTON
        );
        if class.eq_ignore_ascii_case("Static") {
            Ok(Kind::Label)
        } else if class.eq_ignore_ascii_case("Button") && check_box && style & BS_PUSHLIKE == 0 {
            Ok(Kind::CheckBox)
        } else {
            Err(WinError::new(E_INVALIDARG, "word wrap control"))
        }
    }

    /// How wide one line of the text of `control` may be, in pixels, so that the control
    /// draws it on one line: the client width of a label; for a check box or radio button the
    /// client width less the room of its box, which is the width of the menu check mark
    /// (`SM_CXMENUCHECK`, 15 pixels at 96 DPI) and 6 pixels more, at the control's DPI. That
    /// is more than the controls keep: drawn into a bitmap at 96 DPI, a check box of Common
    /// Controls 6 starts its text 16 pixels in and one of version 5 at 18 pixels, ending it 2
    /// pixels short of its right edge (20 pixels in all). Fails with `E_INVALIDARG` for any
    /// other control.
    pub fn line_width(control: HWND) -> WinResult<i32> {
        let kind = kind(control)?;
        let mut client = RECT::default();
        // SAFETY: `client` is a valid out buffer; USER validates `control`.
        unsafe { GetClientRect(control, &mut client) }.context("GetClientRect")?;
        let width = client.right - client.left;
        Ok(match kind {
            Kind::Label => width,
            Kind::CheckBox => width - check_box_room(control),
        })
    }

    /// The room a check box keeps for its box ([`line_width`]), at the DPI of `control`.
    fn check_box_room(control: HWND) -> i32 {
        let dpi = window_dpi(control);
        let check_mark = dpi
            .and_then(|dpi| system_metric_for_dpi(SM_CXMENUCHECK, dpi))
            // SAFETY: no preconditions.
            .unwrap_or_else(|| unsafe { GetSystemMetrics(SM_CXMENUCHECK) });
        let dpi = dpi.and_then(|dpi| i32::try_from(dpi).ok()).unwrap_or(96);
        check_mark + CHECK_BOX_GAP * dpi / 96
    }

    /// `text` wrapped at spaces ([`wrap_words`]) to the [`line_width`] of `control`, measured
    /// with the control's own font (`WM_GETFONT`) as `DrawTextW` draws a single line, so that
    /// the control draws each line as it is and cuts no word. Fails with `E_INVALIDARG` for a
    /// control other than a label, a check box, or a radio button, and when the control
    /// cannot be measured. The text is not set.
    pub fn wrap_for_control(control: HWND, text: &str) -> WinResult<String> {
        let width = line_width(control)?;
        // SAFETY: a message without pointers; USER validates `control`. 0 means the system
        // font, which a device context of the control starts with.
        let font = unsafe { SendMessageW(control, WM_GETFONT, None, None) }.0;
        let measure = Measure::new(Some(control), HFONT(font as *mut c_void), None)?;
        Ok(wrap_words(text, width, |line| measure.width(line)))
    }

    /// `text` wrapped at spaces ([`wrap_words`]) for `MessageBoxW`, so that the box shows each
    /// line as it is and cuts no word: every line fits [`message_text_width`] on every monitor
    /// (the work area and DPI of each, as the calling thread sees them), measured with the
    /// message font at that monitor's DPI (`SystemParametersInfoForDpi`,
    /// `NONCLIENTMETRICSW::lfMessageFont`; before Windows 10 1607 the font of the system DPI).
    /// So the box may open on any monitor. `text` comes back as it is when nothing can be
    /// measured.
    pub fn wrap_for_message_box(text: &str) -> String {
        let limits = message_limits();
        if limits.is_empty() {
            return text.to_owned();
        }
        wrap_words(text, RELATIVE_LIMIT, |line| {
            limits
                .iter()
                .map(|(measure, limit)| relative(measure.width(line), *limit))
                .max()
                .unwrap_or(0)
        })
    }

    /// `width` relative to `limit`, as a part of [`RELATIVE_LIMIT`] rounded up: at most
    /// [`RELATIVE_LIMIT`] exactly when `width` is at most `limit`.
    fn relative(width: i32, limit: i32) -> i32 {
        let scaled = i64::from(width) * i64::from(RELATIVE_LIMIT);
        let limit = i64::from(limit.max(1));
        i32::try_from((scaled + limit - 1) / limit).unwrap_or(i32::MAX)
    }

    /// A measure of the message font and the line limit for each DPI of the monitors (with
    /// the narrowest work area of that DPI); for the system DPI alone when the monitors cannot
    /// be listed. Empty when no font could be made.
    fn message_limits() -> Vec<(Measure, i32)> {
        let mut screens: Vec<(u32, i32)> = Vec::new();
        for (dpi, work_width) in monitors() {
            match screens.iter_mut().find(|(known, _)| *known == dpi) {
                Some(screen) => screen.1 = screen.1.min(work_width),
                None => screens.push((dpi, work_width)),
            }
        }
        if screens.is_empty() {
            screens.push((screen_dpi(), 0));
        }
        screens
            .into_iter()
            .filter_map(|(dpi, work_width)| {
                let font = message_font(dpi)?;
                let measure = Measure::new(None, font.0, Some(font)).ok()?;
                let limit = message_text_width(work_width, dpi, measure.average_char_width());
                Some((measure, limit))
            })
            .collect()
    }

    /// The DPI and the work area's width of each monitor, as the calling thread sees them
    /// (`EnumDisplayMonitors`, `GetMonitorInfoW`, `GetDpiForMonitor`); a monitor whose DPI
    /// cannot be read counts with the system DPI.
    fn monitors() -> Vec<(u32, i32)> {
        let mut handles: Vec<HMONITOR> = Vec::new();
        // SAFETY: the callback runs during the call only and receives a pointer to `handles`,
        // which outlives the call; it is never kept.
        let _ = unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(each_monitor),
                LPARAM(&raw mut handles as isize),
            )
        };
        handles
            .into_iter()
            .filter_map(|monitor| {
                let mut info = MONITORINFO {
                    cbSize: size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                // SAFETY: `info` is a valid out buffer with its size set; the handle came
                // from the enumeration (a stale one makes the call fail).
                if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
                    return None;
                }
                let (mut dpi, mut unused) = (0_u32, 0_u32);
                // SAFETY: valid out pointers; the call validates the handle.
                let read =
                    unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused) };
                let dpi = if read.is_ok() && dpi != 0 {
                    dpi
                } else {
                    screen_dpi()
                };
                Some((dpi, info.rcWork.right - info.rcWork.left))
            })
            .collect()
    }

    /// The callback of [`monitors`]: collects each monitor.
    unsafe extern "system" fn each_monitor(
        monitor: HMONITOR,
        _dc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        guard_bool(|| {
            // SAFETY: `lparam` is the vector of the enumeration that calls this.
            let handles = unsafe { &mut *(lparam.0 as *mut Vec<HMONITOR>) };
            handles.push(monitor);
            true
        })
    }

    /// The DPI of the screen as the calling thread sees it (`LOGPIXELSX`); 96 when it cannot
    /// be read.
    fn screen_dpi() -> u32 {
        // SAFETY: the screen's device context, released below.
        unsafe {
            let dc = GetDC(None);
            if dc.is_invalid() {
                return 96;
            }
            let dpi = GetDeviceCaps(Some(dc), LOGPIXELSX);
            ReleaseDC(None, dc);
            u32::try_from(dpi)
                .ok()
                .filter(|&dpi| dpi != 0)
                .unwrap_or(96)
        }
    }

    /// `BOOL WINAPI SystemParametersInfoForDpi(UINT, UINT, PVOID, UINT, UINT)`.
    type ParametersForDpi = unsafe extern "system" fn(u32, u32, *mut c_void, u32, u32) -> BOOL;

    /// `SystemParametersInfoForDpi` of user32, looked up once; `None` before Windows 10 1607.
    fn parameters_for_dpi() -> Option<ParametersForDpi> {
        static FUNCTION: OnceLock<Option<ParametersForDpi>> = OnceLock::new();
        *FUNCTION.get_or_init(|| {
            let address = user32_export(s!("SystemParametersInfoForDpi"))?;
            // SAFETY: the export has the documented signature `ParametersForDpi` describes
            // (four `UINT` and a pointer parameter, a `BOOL` result, WINAPI calling
            // convention).
            Some(unsafe {
                core::mem::transmute::<unsafe extern "system" fn() -> isize, ParametersForDpi>(
                    address,
                )
            })
        })
    }

    /// The message font (`lfMessageFont`) at `dpi`, made for this process; `None` when the
    /// metrics cannot be read or the font cannot be made.
    fn message_font(dpi: u32) -> Option<OwnedFont> {
        let mut metrics = NONCLIENTMETRICSW {
            cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
            ..Default::default()
        };
        let read = match parameters_for_dpi() {
            // SAFETY: `metrics` is a valid out buffer of the size it announces; the function
            // is user32's SystemParametersInfoForDpi (see `parameters_for_dpi`).
            Some(for_dpi) => unsafe {
                for_dpi(
                    SPI_GETNONCLIENTMETRICS.0,
                    metrics.cbSize,
                    (&raw mut metrics).cast(),
                    0,
                    dpi,
                )
            }
            .as_bool(),
            // SAFETY: as above, for the system DPI.
            None => unsafe {
                SystemParametersInfoW(
                    SPI_GETNONCLIENTMETRICS,
                    metrics.cbSize,
                    Some((&raw mut metrics).cast()),
                    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                )
            }
            .is_ok(),
        };
        if !read {
            return None;
        }
        // SAFETY: a complete LOGFONTW read from the system.
        let font = unsafe { CreateFontIndirectW(&metrics.lfMessageFont) };
        (!font.is_invalid()).then_some(OwnedFont(font))
    }

    /// A font made for this process, deleted on drop.
    struct OwnedFont(HFONT);

    impl Drop for OwnedFont {
        fn drop(&mut self) {
            // SAFETY: the font was made by `message_font` and is deselected from every device
            // context before it goes (`Measure` drops its font after releasing its context).
            let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
        }
    }

    /// A device context of a window (or the screen) with a font selected, which measures
    /// single lines as `DrawTextW` draws them. Dropping it puts the context's font back,
    /// releases the context, and then deletes the font it owns.
    struct Measure {
        window: Option<HWND>,
        dc: HDC,
        /// The font the context had before, when another one was selected.
        previous: Option<HGDIOBJ>,
        /// Dropped after the context was released (fields drop after `Drop::drop`).
        _font: Option<OwnedFont>,
    }

    impl Measure {
        /// A device context of `window` (the screen for `None`) with `font` selected; the
        /// context's own font for a null `font`. `owned` is deleted when the measure goes.
        fn new(window: Option<HWND>, font: HFONT, owned: Option<OwnedFont>) -> WinResult<Self> {
            // SAFETY: USER validates the window handle; the context is released on drop.
            let dc = unsafe { GetDC(window) };
            if dc.is_invalid() {
                return Err(WinError::last("GetDC"));
            }
            let previous = (!font.is_invalid())
                // SAFETY: a valid context and a font that outlives the selection (the
                // control's own, or `owned`, which drops after the context is released).
                .then(|| unsafe { SelectObject(dc, HGDIOBJ(font.0)) });
            Ok(Self {
                window,
                dc,
                previous,
                _font: owned,
            })
        }

        /// The width of `line` drawn on one line (`DrawTextW` with `DT_SINGLELINE`,
        /// `DT_EXPANDTABS`, and `DT_NOPREFIX`, which measures an `&` too and so never less
        /// than a control that hides it).
        fn width(&self, line: &str) -> i32 {
            if line.is_empty() {
                // An empty slice lends a dangling pointer, which DrawTextW would read.
                return 0;
            }
            let mut units: Vec<u16> = line.encode_utf16().collect();
            let mut rect = RECT::default();
            // SAFETY: a valid context; without DT_MODIFYSTRING the call only reads the
            // text; `rect` is a valid in/out rectangle.
            unsafe {
                DrawTextW(
                    self.dc,
                    &mut units,
                    &mut rect,
                    DT_CALCRECT | DT_SINGLELINE | DT_EXPANDTABS | DT_NOPREFIX,
                )
            };
            rect.right - rect.left
        }

        /// The average character width of the selected font (`tmAveCharWidth`); 0 when it
        /// cannot be read.
        fn average_char_width(&self) -> i32 {
            let mut metrics = TEXTMETRICW::default();
            // SAFETY: a valid context and a valid out buffer.
            if unsafe { GetTextMetricsW(self.dc, &mut metrics) }.as_bool() {
                metrics.tmAveCharWidth
            } else {
                0
            }
        }
    }

    impl Drop for Measure {
        fn drop(&mut self) {
            // SAFETY: puts back the font the context had, then releases the context that
            // `new` got for the same window.
            unsafe {
                if let Some(previous) = self.previous {
                    SelectObject(self.dc, previous);
                }
                ReleaseDC(self.window, self.dc);
            }
        }
    }
}
