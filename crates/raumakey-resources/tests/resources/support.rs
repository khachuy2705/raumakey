//! Small independent readers for the binary formats under test.
//!
//! These readers are deliberately written against the published Windows formats rather than
//! against the crate's own code, so a writer bug cannot hide behind a matching reader bug.

/// The leading null entry every 32-bit `.res` file starts with.
pub const NULL_ENTRY: [u8; 32] = [
    0, 0, 0, 0, // DataSize
    0x20, 0, 0, 0, // HeaderSize
    0xFF, 0xFF, 0, 0, // TYPE = ordinal 0
    0xFF, 0xFF, 0, 0, // NAME = ordinal 0
    0, 0, 0, 0, // DataVersion
    0, 0, // MemoryFlags
    0, 0, // LanguageId
    0, 0, 0, 0, // Version
    0, 0, 0, 0, // Characteristics
];

/// A resource type or name as stored in a `.res` header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Id {
    /// `0xFFFF` followed by a 16-bit ordinal.
    Ordinal(u16),
    /// NUL-terminated UTF-16 string.
    Name(String),
}

/// One `.res` entry with every header field.
#[derive(Clone, Debug)]
pub struct Entry {
    pub offset: usize,
    pub data_size: u32,
    pub header_size: u32,
    pub kind: Id,
    pub name: Id,
    pub data_version: u32,
    pub memory_flags: u16,
    pub language: u16,
    pub version: u32,
    pub characteristics: u32,
    pub data: Vec<u8>,
}

pub fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

pub fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

pub fn i32_at(bytes: &[u8], offset: usize) -> i32 {
    u32_at(bytes, offset) as i32
}

/// Reads a NUL-terminated UTF-16 string, returning it and the offset just past the NUL.
pub fn utf16z_at(bytes: &[u8], offset: usize) -> (String, usize) {
    let mut units = Vec::new();
    let mut pos = offset;
    loop {
        let unit = u16_at(bytes, pos);
        pos += 2;
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    (String::from_utf16(&units).expect("valid UTF-16"), pos)
}

fn read_id(bytes: &[u8], pos: &mut usize) -> Id {
    if u16_at(bytes, *pos) == 0xFFFF {
        let id = u16_at(bytes, *pos + 2);
        *pos += 4;
        Id::Ordinal(id)
    } else {
        let (name, next) = utf16z_at(bytes, *pos);
        *pos = next;
        Id::Name(name)
    }
}

fn skip_zero_padding(bytes: &[u8], pos: &mut usize) {
    while !pos.is_multiple_of(4) {
        assert_eq!(bytes[*pos], 0, "padding byte at {pos} must be zero");
        *pos += 1;
    }
}

/// Parses a complete `.res` file and asserts the container invariants: the leading null entry,
/// DWORD-aligned entries, `HeaderSize` covering the whole header, and zero padding.
pub fn parse_res(bytes: &[u8]) -> Vec<Entry> {
    assert!(
        bytes.len() >= 32,
        "a .res file starts with a 32-byte null entry"
    );
    assert_eq!(bytes[..32], NULL_ENTRY, "leading null entry");
    assert!(
        bytes.len().is_multiple_of(4),
        "file length is DWORD aligned"
    );
    let mut entries = Vec::new();
    let mut pos = 32;
    while pos < bytes.len() {
        assert!(
            pos.is_multiple_of(4),
            "entry at {pos} must start on a DWORD boundary"
        );
        let offset = pos;
        let data_size = u32_at(bytes, pos);
        let header_size = u32_at(bytes, pos + 4);
        pos += 8;
        let kind = read_id(bytes, &mut pos);
        let name = read_id(bytes, &mut pos);
        skip_zero_padding(bytes, &mut pos);
        let data_version = u32_at(bytes, pos);
        let memory_flags = u16_at(bytes, pos + 4);
        let language = u16_at(bytes, pos + 6);
        let version = u32_at(bytes, pos + 8);
        let characteristics = u32_at(bytes, pos + 12);
        pos += 16;
        assert_eq!(
            pos - offset,
            header_size as usize,
            "HeaderSize must cover the whole header including padding"
        );
        let end = pos + data_size as usize;
        assert!(end <= bytes.len(), "DataSize runs past the end of the file");
        let data = bytes[pos..end].to_vec();
        pos = end;
        skip_zero_padding(bytes, &mut pos);
        entries.push(Entry {
            offset,
            data_size,
            header_size,
            kind,
            name,
            data_version,
            memory_flags,
            language,
            version,
            characteristics,
            data,
        });
    }
    assert_eq!(pos, bytes.len());
    entries
}

/// One node of a `VS_VERSIONINFO` tree.
#[derive(Clone, Debug)]
pub struct VersionNode {
    /// Offset of the node from the start of the resource data.
    pub offset: usize,
    pub length: u16,
    pub value_length: u16,
    pub kind: u16,
    pub key: String,
    /// Raw value bytes: `wValueLength` bytes for binary nodes, `2 * wValueLength` for text nodes.
    pub value: Vec<u8>,
    pub children: Vec<VersionNode>,
}

impl VersionNode {
    /// Returns the single child with this key.
    pub fn child(&self, key: &str) -> &VersionNode {
        let matches: Vec<&VersionNode> = self
            .children
            .iter()
            .filter(|child| child.key == key)
            .collect();
        assert_eq!(matches.len(), 1, "expected one child {key:?}");
        matches[0]
    }

    /// Decodes a text value and checks its NUL terminator.
    pub fn text(&self) -> String {
        assert_eq!(self.kind, 1, "text node");
        let (pairs, rest) = self.value.as_chunks::<2>();
        assert!(rest.is_empty(), "text value has whole UTF-16 units");
        let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
        assert_eq!(units.last(), Some(&0), "text value is NUL-terminated");
        String::from_utf16(&units[..units.len() - 1]).expect("valid UTF-16")
    }
}

/// Parses one version node and its children, asserting DWORD alignment of every node and of the
/// value, zero padding, and that `wLength` ends exactly at the end of the last child or value.
pub fn parse_version_node(bytes: &[u8], offset: usize) -> VersionNode {
    assert!(
        offset.is_multiple_of(4),
        "node at {offset} must be DWORD aligned"
    );
    let length = u16_at(bytes, offset);
    let value_length = u16_at(bytes, offset + 2);
    let kind = u16_at(bytes, offset + 4);
    assert!(kind <= 1, "wType is 0 (binary) or 1 (text)");
    let end = offset + usize::from(length);
    assert!(end <= bytes.len(), "wLength runs past the data");
    let (key, mut pos) = utf16z_at(bytes, offset + 6);
    skip_zero_padding(bytes, &mut pos);
    let value_bytes = if kind == 1 {
        usize::from(value_length) * 2
    } else {
        usize::from(value_length)
    };
    let value = bytes[pos..pos + value_bytes].to_vec();
    pos += value_bytes;
    let mut children = Vec::new();
    loop {
        while !pos.is_multiple_of(4) && pos < end {
            assert_eq!(bytes[pos], 0, "padding byte at {pos} must be zero");
            pos += 1;
        }
        if pos >= end {
            break;
        }
        let child = parse_version_node(bytes, pos);
        pos += usize::from(child.length);
        children.push(child);
    }
    assert_eq!(pos, end, "node {key:?} must end exactly at its wLength");
    VersionNode {
        offset,
        length,
        value_length,
        kind,
        key,
        value,
        children,
    }
}

/// An XML element read by [`parse_xml`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlElement {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    /// Concatenated, unescaped, trimmed character data.
    pub text: String,
    pub children: Vec<XmlElement>,
}

impl XmlElement {
    /// Returns an attribute value.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Returns every descendant reached by following child names.
    pub fn find_all(&self, path: &[&str]) -> Vec<&XmlElement> {
        let mut current = vec![self];
        for name in path {
            current = current
                .into_iter()
                .flat_map(|element| element.children.iter())
                .filter(|child| child.name == *name)
                .collect();
        }
        current
    }

    /// Returns the single descendant reached by following child names.
    pub fn find(&self, path: &[&str]) -> &XmlElement {
        let found = self.find_all(path);
        assert_eq!(found.len(), 1, "expected one element at {path:?}");
        found[0]
    }
}

struct XmlParser<'a> {
    text: &'a str,
    pos: usize,
}

impl XmlParser<'_> {
    fn rest(&self) -> &str {
        &self.text[self.pos..]
    }

    fn expect(&mut self, token: &str) {
        assert!(
            self.rest().starts_with(token),
            "expected {token:?} at byte {}",
            self.pos
        );
        self.pos += token.len();
    }

    fn skip_whitespace(&mut self) {
        let trimmed = self.rest().trim_start().len();
        self.pos = self.text.len() - trimmed;
    }

    fn skip_misc(&mut self) {
        loop {
            self.skip_whitespace();
            if self.rest().starts_with("<!--") {
                let end = self.rest().find("-->").expect("unterminated comment");
                self.pos += end + 3;
            } else {
                break;
            }
        }
    }

    fn name(&mut self) -> String {
        let end = self
            .rest()
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, ':' | '.' | '-' | '_')))
            .unwrap_or(self.rest().len());
        assert!(end > 0, "expected a name at byte {}", self.pos);
        let name = self.rest()[..end].to_owned();
        self.pos += end;
        name
    }

    fn element(&mut self) -> XmlElement {
        self.expect("<");
        let name = self.name();
        let mut attributes: Vec<(String, String)> = Vec::new();
        loop {
            self.skip_whitespace();
            if self.rest().starts_with("/>") {
                self.pos += 2;
                return XmlElement {
                    name,
                    attributes,
                    text: String::new(),
                    children: Vec::new(),
                };
            }
            if self.rest().starts_with('>') {
                self.pos += 1;
                break;
            }
            let key = self.name();
            self.skip_whitespace();
            self.expect("=");
            self.skip_whitespace();
            let quote = self.rest().chars().next().expect("attribute value");
            assert!(quote == '"' || quote == '\'', "attribute values are quoted");
            self.pos += 1;
            let end = self.rest().find(quote).expect("unterminated attribute");
            let raw = &self.rest()[..end];
            assert!(!raw.contains('<'), "'<' must be escaped in attributes");
            let value = unescape_xml(raw);
            self.pos += end + 1;
            assert!(
                attributes.iter().all(|(existing, _)| *existing != key),
                "duplicate attribute {key}"
            );
            attributes.push((key, value));
        }
        let mut text = String::new();
        let mut children = Vec::new();
        loop {
            if self.rest().starts_with("</") {
                self.pos += 2;
                let end_name = self.name();
                assert_eq!(end_name, name, "mismatched end tag");
                self.skip_whitespace();
                self.expect(">");
                break;
            }
            if self.rest().starts_with("<!--") {
                let end = self.rest().find("-->").expect("unterminated comment");
                self.pos += end + 3;
                continue;
            }
            if self.rest().starts_with('<') {
                children.push(self.element());
                continue;
            }
            let end = self.rest().find('<').expect("unterminated element");
            text.push_str(&unescape_xml(&self.rest()[..end]));
            self.pos += end;
        }
        XmlElement {
            name,
            attributes,
            text: text.trim().to_owned(),
            children,
        }
    }
}

/// Decodes the five predefined entities; any other `&` sequence is a failure.
fn unescape_xml(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        rest = &rest[index..];
        let end = rest.find(';').expect("unterminated entity");
        out.push(match &rest[..=end] {
            "&amp;" => '&',
            "&lt;" => '<',
            "&gt;" => '>',
            "&quot;" => '"',
            "&apos;" => '\'',
            other => panic!("unknown entity {other}"),
        });
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Parses a UTF-8 XML document with a declaration and one root element, asserting
/// well-formedness (balanced tags, quoted unique attributes, escaped `<` and `&`).
pub fn parse_xml(text: &str) -> XmlElement {
    let mut parser = XmlParser { text, pos: 0 };
    parser.expect("<?xml ");
    let end = parser.rest().find("?>").expect("unterminated declaration");
    let declaration = &parser.rest()[..end];
    assert!(
        declaration.contains("version=\"1.0\""),
        "XML 1.0 declaration"
    );
    assert!(
        declaration.contains("encoding=\"UTF-8\""),
        "UTF-8 declaration"
    );
    parser.pos += end + 2;
    parser.skip_misc();
    let root = parser.element();
    parser.skip_misc();
    assert_eq!(parser.pos, text.len(), "content after the root element");
    root
}

/// Returns the entries whose type is the given ordinal.
pub fn of_type(entries: &[Entry], kind: u16) -> Vec<&Entry> {
    entries
        .iter()
        .filter(|entry| entry.kind == Id::Ordinal(kind))
        .collect()
}

/// Returns the single entry with the given ordinal type and name.
pub fn find(entries: &[Entry], kind: u16, name: u16) -> &Entry {
    let matches: Vec<&Entry> = entries
        .iter()
        .filter(|entry| entry.kind == Id::Ordinal(kind) && entry.name == Id::Ordinal(name))
        .collect();
    assert_eq!(matches.len(), 1, "expected one resource {kind}/{name}");
    matches[0]
}

/// Small deterministic generator (xorshift64*) for seeded randomized tests.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform value in `0..bound` (`bound > 0`).
    pub fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }

    /// Random text of `0..max_chars` characters mixing ASCII, Vietnamese letters, and a
    /// character outside the BMP, never containing NUL.
    pub fn text(&mut self, max_chars: u64) -> String {
        const ALPHABET: [char; 12] = [
            'a',
            'Z',
            '0',
            '.',
            ' ',
            'ă',
            'ơ',
            'Đ',
            'ệ',
            'ữ',
            '\u{1D519}',
            '-',
        ];
        let len = self.below(max_chars);
        (0..len)
            .map(|_| ALPHABET[self.below(ALPHABET.len() as u64) as usize])
            .collect()
    }
}
