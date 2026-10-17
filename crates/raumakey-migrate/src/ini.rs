//! Tolerant parser for `setting.ini` (plan §9).
//!
//! The parser never fails and never panics. It reads what it can and reports every line it
//! could not use by line number and kind only, never by content:
//!
//! - **Encoding:** UTF-8 with or without a byte order mark, or UTF-16 LE/BE with a byte order
//!   mark; input without a mark is read as UTF-8. Further U+FEFF characters at the very start
//!   are ignored as repeated marks. Each line is decoded on its own, so an undecodable line
//!   ([`IniIssueKind::Undecodable`]) never hides the others; a dangling odd byte of UTF-16 input
//!   makes the last line undecodable.
//! - **Lines** end with CR LF, LF, or CR and are numbered from 1, counting every physical line.
//!   Blank lines and lines whose first non-blank character is `;` or `#` are skipped.
//! - **Sections:** `[name]`, optionally followed by a `;` or `#` comment. Names are trimmed and
//!   compare ignoring case (a repeated section continues the first one and keeps its spelling).
//!   Keys before the first header belong to the root section, whose name is empty. A header
//!   that is not closed, has an empty name, or is followed by other text is
//!   [`IniIssueKind::Malformed`], and so is every key below it until the next valid header (a
//!   key is never attributed to the wrong section).
//! - **Entries:** `key=value`, split at the first `=`; key and value are trimmed, and the value
//!   is otherwise kept raw (quotes, inner `=`, and a `;` after the value are part of it, as in
//!   Windows profile files). A line without `=` or with an empty key is malformed.
//! - **Duplicates:** keys compare ignoring case within a section. The last occurrence is kept;
//!   each earlier one is dropped and reported as [`IniIssueKind::DuplicateKey`] on its own line.
//! - **Limit:** at most [`MAX_INI_ITEMS`] entries and issues are kept; parsing stops at the line
//!   that would exceed the limit, reported once as [`IniIssueKind::LimitExceeded`].

use core::fmt;
use std::collections::BTreeMap;

/// Most entries plus issues that [`parse_ini`] keeps before it stops.
pub const MAX_INI_ITEMS: usize = 65_536;

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const BOM_CHAR: char = '\u{FEFF}';

/// Text encoding of an INI file, detected from its byte order mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IniEncoding {
    /// UTF-8 without a byte order mark (also the fallback for unmarked input).
    Utf8,
    /// UTF-8 with a byte order mark.
    Utf8Bom,
    /// UTF-16 little endian with a byte order mark.
    Utf16Le,
    /// UTF-16 big endian with a byte order mark.
    Utf16Be,
}

impl IniEncoding {
    /// Stable name, recorded with the preserved values.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf8Bom => "utf-8-bom",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
        }
    }

    fn detect(bytes: &[u8]) -> Self {
        match bytes {
            [0xEF, 0xBB, 0xBF, ..] => Self::Utf8Bom,
            [0xFF, 0xFE, ..] => Self::Utf16Le,
            [0xFE, 0xFF, ..] => Self::Utf16Be,
            _ => Self::Utf8,
        }
    }
}

/// One `key=value` entry.
///
/// The [`fmt::Debug`] output shows the line, section, key, and the value's length, never the
/// value.
#[derive(Clone, PartialEq, Eq)]
pub struct IniEntry {
    /// One-based line number.
    pub line: usize,
    /// Section name as first spelled; empty for the root section.
    pub section: String,
    /// Key as spelled on this line.
    pub key: String,
    /// Raw value, trimmed.
    pub value: String,
}

impl fmt::Debug for IniEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IniEntry")
            .field("line", &self.line)
            .field("section", &self.section)
            .field("key", &self.key)
            .field("value_bytes", &self.value.len())
            .finish()
    }
}

/// Why a line was not used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IniIssueKind {
    /// The line is not valid text in the detected encoding.
    Undecodable,
    /// The line is not a comment, a valid section header, or a `key=value` entry of a valid
    /// section.
    Malformed,
    /// A later line of the same section sets the same key; this line was dropped.
    DuplicateKey,
    /// The item limit was reached at this line; it and the rest of the file were not read.
    LimitExceeded,
}

/// A line that was not used. Carries no content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IniIssue {
    /// One-based line number.
    pub line: usize,
    /// Why the line was not used.
    pub kind: IniIssueKind,
}

/// Result of [`parse_ini`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IniDocument {
    /// Detected encoding.
    pub encoding: IniEncoding,
    /// Entries that were kept, in line order.
    pub entries: Vec<IniEntry>,
    /// Lines that were not used, in line order.
    pub issues: Vec<IniIssue>,
}

/// Parses INI text (see the [module documentation](self)).
pub fn parse_ini(bytes: &[u8]) -> IniDocument {
    let encoding = IniEncoding::detect(bytes);
    let mut parser = Parser::new();
    match encoding {
        IniEncoding::Utf8 | IniEncoding::Utf8Bom => {
            let body = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
            for (index, line) in Lines::new(body, b'\r', b'\n').enumerate() {
                if !parser.line(index, core::str::from_utf8(line).ok()) {
                    break;
                }
            }
        }
        IniEncoding::Utf16Le => {
            parse_utf16(&mut parser, bytes, u16::from_le_bytes, u16::to_le_bytes)
        }
        IniEncoding::Utf16Be => {
            parse_utf16(&mut parser, bytes, u16::from_be_bytes, u16::to_be_bytes)
        }
    }
    parser.finish(encoding)
}

/// Splits UTF-16 code units into lines and decodes one line at a time.
fn parse_utf16(
    parser: &mut Parser,
    bytes: &[u8],
    decode: fn([u8; 2]) -> u16,
    encode: fn(u16) -> [u8; 2],
) {
    let body = bytes.get(2..).unwrap_or_default();
    let (units, remainder) = body.as_chunks::<2>();
    let mut lines = Lines::new(units, encode(0x000D), encode(0x000A))
        .enumerate()
        .peekable();
    while let Some((index, line)) = lines.next() {
        let truncated = !remainder.is_empty() && lines.peek().is_none();
        let decoded = if truncated {
            None
        } else {
            char::decode_utf16(line.iter().map(|&unit| decode(unit)))
                .collect::<Result<String, _>>()
                .ok()
        };
        if !parser.line(index, decoded.as_deref()) {
            return;
        }
    }
}

/// Line-by-line parser state.
struct Parser {
    /// Kept entries; a dropped duplicate leaves `None`.
    entries: Vec<Option<IniEntry>>,
    /// Number of `Some` elements of `entries`.
    live: usize,
    issues: Vec<IniIssue>,
    /// Index into `entries` by lowercase section and key.
    keys: BTreeMap<(String, String), usize>,
    /// First spelling by lowercase section name.
    sections: BTreeMap<String, String>,
    /// Current section (spelling, lowercase); `None` below a malformed header.
    current: Option<(String, String)>,
}

impl Parser {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            live: 0,
            issues: Vec::new(),
            keys: BTreeMap::new(),
            sections: BTreeMap::new(),
            current: Some((String::new(), String::new())),
        }
    }

    /// Handles the line with zero-based `index`; `text` is `None` when it could not be decoded.
    /// Returns false when parsing must stop.
    fn line(&mut self, index: usize, text: Option<&str>) -> bool {
        let line = index.saturating_add(1);
        let Some(text) = text else {
            return self.issue(line, IniIssueKind::Undecodable);
        };
        let text = if index == 0 {
            text.trim_start_matches(BOM_CHAR)
        } else {
            text
        };
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
            return true;
        }
        if let Some(rest) = trimmed.strip_prefix('[') {
            return self.header(line, rest);
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            return self.issue(line, IniIssueKind::Malformed);
        };
        let key = key.trim();
        let Some((section, section_lc)) = self.current.clone() else {
            return self.issue(line, IniIssueKind::Malformed);
        };
        if key.is_empty() {
            return self.issue(line, IniIssueKind::Malformed);
        }
        self.entry(
            IniEntry {
                line,
                section,
                key: key.to_owned(),
                value: value.trim().to_owned(),
            },
            section_lc,
        )
    }

    /// Handles a line that starts with `[`; `rest` follows the bracket.
    fn header(&mut self, line: usize, rest: &str) -> bool {
        let name = rest.split_once(']').and_then(|(name, after)| {
            let name = name.trim();
            let after = after.trim();
            let tail_ok = after.is_empty() || after.starts_with(';') || after.starts_with('#');
            (!name.is_empty() && tail_ok).then_some(name)
        });
        match name {
            Some(name) => {
                let lower = name.to_lowercase();
                let spelling = self
                    .sections
                    .entry(lower.clone())
                    .or_insert_with(|| name.to_owned())
                    .clone();
                self.current = Some((spelling, lower));
                true
            }
            None => {
                self.current = None;
                self.issue(line, IniIssueKind::Malformed)
            }
        }
    }

    fn entry(&mut self, entry: IniEntry, section_lc: String) -> bool {
        if !self.has_room() {
            return self.stop(entry.line);
        }
        let id = (section_lc, entry.key.to_lowercase());
        if let Some(previous) = self
            .keys
            .get(&id)
            .and_then(|&index| self.entries.get_mut(index))
            .and_then(Option::take)
        {
            self.live = self.live.saturating_sub(1);
            self.issues.push(IniIssue {
                line: previous.line,
                kind: IniIssueKind::DuplicateKey,
            });
        }
        self.keys.insert(id, self.entries.len());
        self.entries.push(Some(entry));
        self.live = self.live.saturating_add(1);
        true
    }

    fn issue(&mut self, line: usize, kind: IniIssueKind) -> bool {
        if !self.has_room() {
            return self.stop(line);
        }
        self.issues.push(IniIssue { line, kind });
        true
    }

    fn has_room(&self) -> bool {
        self.live.saturating_add(self.issues.len()) < MAX_INI_ITEMS
    }

    fn stop(&mut self, line: usize) -> bool {
        self.issues.push(IniIssue {
            line,
            kind: IniIssueKind::LimitExceeded,
        });
        false
    }

    fn finish(mut self, encoding: IniEncoding) -> IniDocument {
        self.issues.sort_by_key(|issue| issue.line);
        IniDocument {
            encoding,
            entries: self.entries.into_iter().flatten().collect(),
            issues: self.issues,
        }
    }
}

/// Lines of `units` split at CR LF, LF, or CR, borrowed from the input.
///
/// Generic over the code unit so that UTF-16 is split before it is decoded. Always yields at
/// least one (possibly empty) line.
struct Lines<'a, T> {
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
