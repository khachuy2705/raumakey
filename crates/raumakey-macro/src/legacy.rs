//! Legacy `trigger:replacement` macro text import and export (plan §4).
//!
//! The legacy format is one macro per line, `trigger:replacement`, split at the first `:`.
//! Import decodes UTF-8 (with or without a byte order mark) or UTF-16 LE/BE with a byte order
//! mark; input without a byte order mark is read as UTF-8. Further U+FEFF characters at the very
//! start of the text are ignored as repeated byte order marks. Lines end with CR LF, LF, or CR and
//! are numbered from 1, counting every physical line.
//!
//! A line is skipped silently when, after trimming whitespace, it is empty or starts with `#`,
//! `;`, or `//`, or when it contains `DO NOT DELETE` anywhere (legacy header). Otherwise the line
//! is split at its first `:`. Whitespace around the trigger and around the replacement is
//! trimmed for readability (`ko : không` imports `không`), except that a replacement consisting
//! only of whitespace (a tab, a no-break space) is kept as written instead of becoming empty.
//! Both halves are then validated and normalized to NFC exactly like
//! [`normalize_entry`](crate::validate::normalize_entry); nothing else is changed. Each line is
//! decoded and parsed independently, so one bad line never hides the others.
//!
//! Lines that cannot be imported are reported as [`LegacyIssue`]s that carry only the line number
//! and a reason code, never the line content. Input over [`MAX_LEGACY_TEXT_BYTES`] bytes or
//! [`MAX_LEGACY_MACRO_LINES`] macro lines is rejected as a whole
//! ([`LegacyIssueReason::TooLarge`]).
//!
//! Export writes the same format; [`export_text_checked`] also reports the entries the format
//! cannot hold (see [`is_legacy_exportable`]).

use core::fmt;
use std::borrow::Cow;

use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use serde::{Deserialize, Serialize};

use crate::error::EntryError;
use crate::validate::{normalize_replacement, normalize_trigger};

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const BOM_CHAR: char = '\u{FEFF}';
const HEADER_MARKER: &str = "DO NOT DELETE";
const LINE_END: &str = "\r\n";

/// Largest input, in bytes including any byte order mark, that [`parse_legacy_text`] parses.
///
/// 16 MiB is far more than any realistic macro file (one maximal replacement of 4,096 scalars
/// takes at most 16 KiB) and bounds the time and memory spent on a wrong or hostile file. Larger
/// input is rejected as a whole with [`LegacyIssueReason::TooLarge`].
pub const MAX_LEGACY_TEXT_BYTES: usize = 16 * 1024 * 1024;

/// Largest number of macro lines (lines that are not blank, comment, or header lines) that
/// [`parse_legacy_text`] accepts.
///
/// Each macro line becomes one entry or one issue, so this bounds both lists. Input with more
/// macro lines is rejected as a whole with [`LegacyIssueReason::TooLarge`].
pub const MAX_LEGACY_MACRO_LINES: usize = 100_000;

/// Text encoding of a legacy macro file, detected from its byte order mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LegacyEncoding {
    /// UTF-8 without a byte order mark (also the fallback for unmarked input).
    Utf8,
    /// UTF-8 with a byte order mark.
    Utf8Bom,
    /// UTF-16 little endian with a byte order mark.
    Utf16Le,
    /// UTF-16 big endian with a byte order mark.
    Utf16Be,
}

/// Why a legacy line was not imported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LegacyIssueReason {
    /// The line is not valid text in the detected encoding.
    InvalidEncoding,
    /// The line has no `:` separator.
    MissingSeparator,
    /// See [`EntryError::EmptyTrigger`].
    EmptyTrigger,
    /// See [`EntryError::TriggerTooLong`].
    TriggerTooLong,
    /// See [`EntryError::TriggerWhitespace`].
    TriggerWhitespace,
    /// See [`EntryError::TriggerControl`].
    TriggerControl,
    /// See [`EntryError::EmptyReplacement`].
    EmptyReplacement,
    /// See [`EntryError::ReplacementTooLong`].
    ReplacementTooLong,
    /// See [`EntryError::ReplacementNul`].
    ReplacementNul,
    /// The input has more than [`MAX_LEGACY_TEXT_BYTES`] bytes or more than
    /// [`MAX_LEGACY_MACRO_LINES`] macro lines, so nothing was imported. Reported once, with
    /// line 0.
    TooLarge,
}

impl LegacyIssueReason {
    /// Stable reason code (kebab-case), identical to the serialized form.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidEncoding => "invalid-encoding",
            Self::MissingSeparator => "missing-separator",
            Self::EmptyTrigger => EntryError::EmptyTrigger.code(),
            Self::TriggerTooLong => EntryError::TriggerTooLong.code(),
            Self::TriggerWhitespace => EntryError::TriggerWhitespace.code(),
            Self::TriggerControl => EntryError::TriggerControl.code(),
            Self::EmptyReplacement => EntryError::EmptyReplacement.code(),
            Self::ReplacementTooLong => EntryError::ReplacementTooLong.code(),
            Self::ReplacementNul => EntryError::ReplacementNul.code(),
            Self::TooLarge => "too-large",
        }
    }
}

impl From<EntryError> for LegacyIssueReason {
    fn from(error: EntryError) -> Self {
        match error {
            EntryError::EmptyTrigger => Self::EmptyTrigger,
            EntryError::TriggerTooLong => Self::TriggerTooLong,
            EntryError::TriggerWhitespace => Self::TriggerWhitespace,
            EntryError::TriggerControl => Self::TriggerControl,
            EntryError::EmptyReplacement => Self::EmptyReplacement,
            EntryError::ReplacementTooLong => Self::ReplacementTooLong,
            EntryError::ReplacementNul => Self::ReplacementNul,
        }
    }
}

/// One legacy line that was not imported. Carries no line content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LegacyIssue {
    /// One-based physical line number, or 0 for an issue about the whole input
    /// ([`LegacyIssueReason::TooLarge`]).
    pub line: usize,
    /// Reason code.
    pub reason: LegacyIssueReason,
}

/// Result of [`parse_legacy_text`].
///
/// The [`fmt::Debug`] output shows the entry count and the issues, never macro text.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct LegacyMacroImport {
    /// Imported entries in file order, with identifiers 1, 2, 3, … and default settings
    /// (enabled, [`BoundaryPolicy::Delimiter`], [`CasePolicy::Adapt`]). Duplicate triggers are
    /// kept; in a table the earlier line wins because it has the lower identifier.
    pub entries: Vec<MacroEntry>,
    /// Lines that were not imported, in line order.
    pub issues: Vec<LegacyIssue>,
}

/// Result of [`export_text_checked`].
///
/// The [`fmt::Debug`] output shows the text length and the skipped identifiers, never macro
/// text.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct LegacyExport {
    /// `trigger:replacement` lines for the written entries.
    pub text: String,
    /// Identifiers of the entries that were not written, in input order.
    pub skipped: Vec<MacroId>,
}

/// Detects the encoding of legacy macro text from its byte order mark.
pub fn detect_encoding(bytes: &[u8]) -> LegacyEncoding {
    match bytes {
        [0xEF, 0xBB, 0xBF, ..] => LegacyEncoding::Utf8Bom,
        [0xFF, 0xFE, ..] => LegacyEncoding::Utf16Le,
        [0xFE, 0xFF, ..] => LegacyEncoding::Utf16Be,
        _ => LegacyEncoding::Utf8,
    }
}

/// Parses legacy macro text (see the [module documentation](self) for the format).
///
/// Never fails and never panics: every line that cannot be imported becomes a [`LegacyIssue`].
/// Input with more than [`MAX_LEGACY_TEXT_BYTES`] bytes or more than [`MAX_LEGACY_MACRO_LINES`]
/// macro lines is not imported at all: the result has no entries and the single issue
/// `LegacyIssue { line: 0, reason: LegacyIssueReason::TooLarge }`. Lines are split and decoded
/// one at a time, so working memory beyond the result is a small multiple of the longest line.
///
/// ```
/// use raumakey_macro::{LegacyIssue, LegacyIssueReason, parse_legacy_text};
///
/// let import = parse_legacy_text("; header\nko:không\nbroken\n".as_bytes());
/// assert_eq!(import.entries.len(), 1);
/// assert_eq!(import.entries[0].replacement, "không");
/// assert_eq!(
///     import.issues,
///     [LegacyIssue { line: 3, reason: LegacyIssueReason::MissingSeparator }]
/// );
/// ```
pub fn parse_legacy_text(bytes: &[u8]) -> LegacyMacroImport {
    if bytes.len() > MAX_LEGACY_TEXT_BYTES {
        return LegacyMacroImport::too_large();
    }
    let mut import = LegacyMacroImport::default();
    let complete = match detect_encoding(bytes) {
        LegacyEncoding::Utf8 | LegacyEncoding::Utf8Bom => {
            let body = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
            Lines::new(body, b'\r', b'\n')
                .enumerate()
                .all(|(index, line)| {
                    let decoded = String::from_utf8_lossy(line);
                    let valid = matches!(decoded, Cow::Borrowed(_));
                    import.add_line(index, &decoded, valid)
                })
        }
        LegacyEncoding::Utf16Le => {
            parse_utf16(&mut import, bytes, u16::from_le_bytes, u16::to_le_bytes)
        }
        LegacyEncoding::Utf16Be => {
            parse_utf16(&mut import, bytes, u16::from_be_bytes, u16::to_be_bytes)
        }
    };
    if complete {
        import
    } else {
        LegacyMacroImport::too_large()
    }
}

/// Parses UTF-16 text that starts with a byte order mark, splitting the raw code units into
/// lines and decoding one line at a time. Returns false when the macro line limit is exceeded.
fn parse_utf16(
    import: &mut LegacyMacroImport,
    bytes: &[u8],
    decode: fn([u8; 2]) -> u16,
    encode: fn(u16) -> [u8; 2],
) -> bool {
    let body = bytes.get(2..).unwrap_or_default();
    let (units, remainder) = body.as_chunks::<2>();
    let mut lines = Lines::new(units, encode(0x000D), encode(0x000A))
        .enumerate()
        .peekable();
    while let Some((index, line)) = lines.next() {
        // A dangling odd byte is an incomplete code unit at the end of the last line.
        let truncated = !remainder.is_empty() && lines.peek().is_none();
        let mut valid = !truncated;
        let mut decoded = String::with_capacity(line.len());
        for scalar in char::decode_utf16(line.iter().map(|&unit| decode(unit))) {
            match scalar {
                Ok(c) => decoded.push(c),
                Err(_) => {
                    valid = false;
                    decoded.push(char::REPLACEMENT_CHARACTER);
                }
            }
        }
        if truncated {
            decoded.push(char::REPLACEMENT_CHARACTER);
        }
        if !import.add_line(index, &decoded, valid) {
            return false;
        }
    }
    true
}

/// Whether `entry` can be written by [`export_text`] and read back unchanged (after NFC
/// normalization) by [`parse_legacy_text`].
///
/// An entry is exportable when it is valid and its normalized trigger has no `:` and does not
/// start with `#`, `;`, `//`, or U+FEFF (which would read as a byte order mark on the first line
/// of a UTF-8 file), and its normalized replacement has no CR or LF, does not contain
/// `DO NOT DELETE`, and either consists only of whitespace or has no leading or trailing
/// whitespace (import trims such whitespace). Enabled flag, boundary policy, and case policy are
/// not part of the legacy format and do not affect exportability.
pub fn is_legacy_exportable(entry: &MacroEntry) -> bool {
    exportable_text(entry).is_some()
}

/// Writes legacy text for `entries` and reports the entries it could not write.
///
/// Every [exportable](is_legacy_exportable) entry becomes one `trigger:replacement` line (NFC
/// forms, CR LF line end), in the given order; the text has no byte order mark and no header.
/// Every other entry is left out and its identifier is listed in [`LegacyExport::skipped`], so
/// the caller can tell the user which macros the legacy format cannot hold (multi-line
/// replacements, for example). Disabled entries are written like enabled ones; filter them
/// beforehand to leave them out.
///
/// ```
/// use raumakey_macro::export_text_checked;
/// use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
///
/// let entry = |id, trigger: &str, replacement: &str| MacroEntry {
///     id: MacroId(id),
///     trigger: trigger.into(),
///     replacement: replacement.into(),
///     enabled: true,
///     boundary: BoundaryPolicy::Delimiter,
///     case_policy: CasePolicy::Adapt,
/// };
/// let export = export_text_checked(&[
///     entry(1, "dc", "dòng 1\r\ndòng 2"),
///     entry(2, "ko", "không"),
/// ]);
/// assert_eq!(export.text, "ko:không\r\n");
/// assert_eq!(export.skipped, [MacroId(1)]);
/// ```
pub fn export_text_checked(entries: &[MacroEntry]) -> LegacyExport {
    let mut export = LegacyExport::default();
    for entry in entries {
        match exportable_text(entry) {
            Some((trigger, replacement)) => {
                export.text.push_str(&trigger);
                export.text.push(':');
                export.text.push_str(&replacement);
                export.text.push_str(LINE_END);
            }
            None => export.skipped.push(entry.id),
        }
    }
    export
}

/// Writes legacy text for `entries`: the [`LegacyExport::text`] of [`export_text_checked`].
///
/// Entries that are not [exportable](is_legacy_exportable) are left out **without notice**. Use
/// this only for entries already checked with [`is_legacy_exportable`]; otherwise use
/// [`export_text_checked`], which reports them.
///
/// ```
/// use raumakey_macro::export_text;
/// use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
///
/// let entry = MacroEntry {
///     id: MacroId(1),
///     trigger: "ko".into(),
///     replacement: "không".into(),
///     enabled: true,
///     boundary: BoundaryPolicy::Delimiter,
///     case_policy: CasePolicy::Adapt,
/// };
/// assert_eq!(export_text(&[entry]), "ko:không\r\n");
/// ```
pub fn export_text(entries: &[MacroEntry]) -> String {
    export_text_checked(entries).text
}

impl LegacyMacroImport {
    /// The result for input over a limit: no entries and one whole-input issue.
    fn too_large() -> Self {
        Self {
            entries: Vec::new(),
            issues: vec![LegacyIssue {
                line: 0,
                reason: LegacyIssueReason::TooLarge,
            }],
        }
    }

    /// Parses one decoded line; `index` is zero-based and `valid` tells whether decoding was
    /// lossless. Comment and blank lines are recognized even when decoding was lossy.
    ///
    /// Returns false, without recording anything, when the line is a macro line beyond
    /// [`MAX_LEGACY_MACRO_LINES`].
    fn add_line(&mut self, index: usize, text: &str, valid: bool) -> bool {
        let line = index.saturating_add(1);
        // Byte order marks repeated after the first one (a common artifact of tools that prepend
        // a mark to text that already has one) are not content.
        let text = if index == 0 {
            text.trim_start_matches(BOM_CHAR)
        } else {
            text
        };
        if !is_macro_line(text) {
            return true;
        }
        if self.entries.len().saturating_add(self.issues.len()) >= MAX_LEGACY_MACRO_LINES {
            return false;
        }
        let parsed = if valid {
            parse_entry(text)
        } else {
            Err(LegacyIssueReason::InvalidEncoding)
        };
        match parsed {
            Ok((trigger, replacement)) => {
                let id = u64::try_from(self.entries.len())
                    .map_or(u64::MAX, |count| count.saturating_add(1));
                self.entries.push(MacroEntry {
                    id: MacroId(id),
                    trigger,
                    replacement,
                    enabled: true,
                    boundary: BoundaryPolicy::default(),
                    case_policy: CasePolicy::default(),
                });
            }
            Err(reason) => self.issues.push(LegacyIssue { line, reason }),
        }
        true
    }
}

/// Whether a decoded line defines a macro, i.e. is not a blank, comment, or header line.
fn is_macro_line(text: &str) -> bool {
    let trimmed = text.trim();
    !(trimmed.is_empty() || is_comment(trimmed) || text.contains(HEADER_MARKER))
}

fn is_comment(text: &str) -> bool {
    text.starts_with('#') || text.starts_with(';') || text.starts_with("//")
}

/// Splits a macro line at its first `:` and validates both halves.
fn parse_entry(line: &str) -> Result<(String, String), LegacyIssueReason> {
    let (trigger, replacement) = line
        .split_once(':')
        .ok_or(LegacyIssueReason::MissingSeparator)?;
    let trigger = normalize_trigger(trigger.trim())?;
    let replacement = normalize_replacement(legacy_replacement(replacement))?;
    Ok((trigger, replacement))
}

/// The replacement part of a macro line as imported: without surrounding whitespace, or as
/// written when it consists only of whitespace (a tab or a no-break space is a valid
/// replacement, not an empty one).
fn legacy_replacement(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.is_empty() { raw } else { trimmed }
}

/// Normalized trigger and replacement when the entry survives a legacy round trip.
fn exportable_text(entry: &MacroEntry) -> Option<(String, String)> {
    let trigger = normalize_trigger(&entry.trigger).ok()?;
    let replacement = normalize_replacement(&entry.replacement).ok()?;
    let representable = !trigger.contains(':')
        && !is_comment(&trigger)
        && !trigger.starts_with(BOM_CHAR)
        && !replacement.contains(['\r', '\n'])
        && legacy_replacement(&replacement) == replacement
        && !replacement.contains(HEADER_MARKER);
    representable.then_some((trigger, replacement))
}

impl fmt::Debug for LegacyMacroImport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LegacyMacroImport")
            .field("entries", &self.entries.len())
            .field("issues", &self.issues)
            .finish()
    }
}

impl fmt::Debug for LegacyExport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LegacyExport")
            .field("text_bytes", &self.text.len())
            .field("skipped", &self.skipped)
            .finish()
    }
}

/// Lines of `units` split at CR LF, LF, or CR, borrowed from the input without copying.
///
/// Generic over the code unit so that UTF-16 is split before it is decoded. Always yields at
/// least one (possibly empty) line; text that ends with a line break yields a final empty line.
struct Lines<'a, T> {
    /// Text after the last line break returned; `None` once the last line has been returned.
    rest: Option<&'a [T]>,
    cr: T,
    lf: T,
}

impl<'a, T> Lines<'a, T> {
    fn new(units: &'a [T], cr: T, lf: T) -> Self {
        Self {
            rest: Some(units),
            cr,
            lf,
        }
    }
}

impl<'a, T: Copy + PartialEq> Iterator for Lines<'a, T> {
    type Item = &'a [T];

    fn next(&mut self) -> Option<&'a [T]> {
        let rest = self.rest.take()?;
        let (cr, lf) = (self.cr, self.lf);
        let Some(position) = rest.iter().position(|&unit| unit == cr || unit == lf) else {
            return Some(rest);
        };
        let (line, line_break) = rest.split_at_checked(position)?;
        let break_len = if line_break.starts_with(&[cr, lf]) {
            2
        } else {
            1
        };
        self.rest = line_break.get(break_len..);
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::Lines;

    fn lines(units: &[u8]) -> Vec<&[u8]> {
        Lines::new(units, b'\r', b'\n').collect()
    }

    #[test]
    fn lines_handles_every_line_end() {
        let expected: [&[u8]; 5] = [b"a", b"b", b"c", b"", b""];
        assert_eq!(lines(b"a\r\nb\rc\n\r\n"), expected);
        let empty: [&[u8]; 1] = [b""];
        assert_eq!(lines(b""), empty);
        let lone: [&[u8]; 3] = [b"", b"", b""];
        assert_eq!(lines(b"\n\r"), lone);
        let unterminated: [&[u8]; 2] = [b"a", b"b"];
        assert_eq!(lines(b"a\nb"), unterminated);
    }

    #[test]
    fn lines_stays_exhausted() {
        let mut iter = Lines::new(&b"a\n"[..], b'\r', b'\n');
        assert_eq!(iter.next(), Some(&b"a"[..]));
        assert_eq!(iter.next(), Some(&b""[..]));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn lines_splits_utf16_code_units_without_decoding() {
        let units: Vec<[u8; 2]> = "a\r\nb\u{0D0A}c"
            .encode_utf16()
            .map(u16::to_le_bytes)
            .collect();
        let lines: Vec<&[[u8; 2]]> =
            Lines::new(&units[..], 0x000Du16.to_le_bytes(), 0x000Au16.to_le_bytes()).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], [[b'a', 0]]);
        // U+0D0A is a single code unit made of the bytes 0x0D and 0x0A; it is no line break.
        assert_eq!(lines[1].len(), 3);
    }
}
