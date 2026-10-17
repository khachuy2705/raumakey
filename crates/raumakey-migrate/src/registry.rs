//! Bounded, read-only walk of a registry key and its canonical encoding.

use crate::error::MigrateError;
use crate::ports::{
    LegacyRegistry, LegacyValue, REG_BINARY, REG_DWORD, REG_EXPAND_SZ, REG_MULTI_SZ, REG_QWORD,
    REG_SZ,
};

/// Deepest subkey level read below a source key (the key itself is level 0).
pub(crate) const MAX_DEPTH: usize = 8;

/// Most subkeys read below one source key.
pub(crate) const MAX_KEYS: usize = 1024;

/// First bytes of the canonical encoding (format version 1).
pub(crate) const CANONICAL_MAGIC: &[u8] = b"raumakey-registry-v1\0";

/// Why a subkey was listed but not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SkipReason {
    /// The name is empty or contains a backslash, so it cannot be addressed as a subkey.
    InvalidName,
    /// The depth or key-count limit was reached.
    LimitExceeded,
}

/// A registry key as read, in canonical order.
///
/// Values are sorted by name (UTF-8 byte order; ties, which only a misbehaving port produces,
/// by their encoding), subkeys and skipped subkeys by name.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct RegistryNode {
    pub(crate) values: Vec<(String, LegacyValue)>,
    pub(crate) subkeys: Vec<(String, RegistryNode)>,
    pub(crate) skipped: Vec<(String, SkipReason)>,
}

/// Reads the key at `path` and its subkeys, bounded by [`MAX_DEPTH`] and [`MAX_KEYS`].
///
/// `Ok(None)` when the key does not exist. A subkey that disappears between the listing and
/// the read is left out.
pub(crate) fn read_tree<R: LegacyRegistry + ?Sized>(
    registry: &R,
    path: &str,
) -> Result<Option<RegistryNode>, MigrateError> {
    let Some(values) = registry.values(path)? else {
        return Ok(None);
    };
    let mut keys_left = MAX_KEYS;
    read_node(registry, path, values, 0, &mut keys_left).map(Some)
}

fn read_node<R: LegacyRegistry + ?Sized>(
    registry: &R,
    path: &str,
    values: Vec<(String, LegacyValue)>,
    depth: usize,
    keys_left: &mut usize,
) -> Result<RegistryNode, MigrateError> {
    let mut node = RegistryNode {
        values: canonical_values(values),
        ..RegistryNode::default()
    };
    let mut names = registry.subkeys(path)?.unwrap_or_default();
    names.sort();
    names.dedup();
    for name in names {
        if name.is_empty() || name.contains('\\') {
            node.skipped.push((name, SkipReason::InvalidName));
            continue;
        }
        if depth >= MAX_DEPTH || *keys_left == 0 {
            node.skipped.push((name, SkipReason::LimitExceeded));
            continue;
        }
        let child_path = format!("{path}\\{name}");
        let Some(child_values) = registry.values(&child_path)? else {
            continue;
        };
        *keys_left = keys_left.saturating_sub(1);
        let child = read_node(
            registry,
            &child_path,
            child_values,
            depth.saturating_add(1),
            keys_left,
        )?;
        node.subkeys.push((name, child));
    }
    Ok(node)
}

/// Sorts values by name, then by encoding.
fn canonical_values(values: Vec<(String, LegacyValue)>) -> Vec<(String, LegacyValue)> {
    let mut keyed: Vec<(Vec<u8>, (String, LegacyValue))> = values
        .into_iter()
        .map(|entry| {
            let mut encoded = Vec::new();
            encode_value(&mut encoded, &entry.1);
            (encoded, entry)
        })
        .collect();
    keyed.sort_by(|(left_bytes, left), (right_bytes, right)| {
        left.0
            .cmp(&right.0)
            .then_with(|| left_bytes.cmp(right_bytes))
    });
    keyed.into_iter().map(|(_, entry)| entry).collect()
}

/// The canonical encoding of a key, which [`discover`](crate::discover) fingerprints.
pub(crate) fn canonical_bytes(node: &RegistryNode) -> Vec<u8> {
    let mut out = CANONICAL_MAGIC.to_vec();
    encode_node(&mut out, node);
    out
}

fn encode_node(out: &mut Vec<u8>, node: &RegistryNode) {
    put_len(out, node.values.len());
    for (name, value) in &node.values {
        put_bytes(out, name.as_bytes());
        encode_value(out, value);
    }
    put_len(out, node.subkeys.len());
    for (name, child) in &node.subkeys {
        put_bytes(out, name.as_bytes());
        encode_node(out, child);
    }
    put_len(out, node.skipped.len());
    for (name, _) in &node.skipped {
        put_bytes(out, name.as_bytes());
    }
}

fn encode_value(out: &mut Vec<u8>, value: &LegacyValue) {
    match value {
        LegacyValue::String(text) => {
            put_header(out, 1, REG_SZ);
            put_bytes(out, text.as_bytes());
        }
        LegacyValue::ExpandString(text) => {
            put_header(out, 2, REG_EXPAND_SZ);
            put_bytes(out, text.as_bytes());
        }
        LegacyValue::MultiString(items) => {
            put_header(out, 3, REG_MULTI_SZ);
            put_len(out, items.len());
            for item in items {
                put_bytes(out, item.as_bytes());
            }
        }
        LegacyValue::Dword(number) => {
            put_header(out, 4, REG_DWORD);
            out.extend_from_slice(&number.to_le_bytes());
        }
        LegacyValue::Qword(number) => {
            put_header(out, 5, REG_QWORD);
            out.extend_from_slice(&number.to_le_bytes());
        }
        LegacyValue::Binary(bytes) => {
            put_header(out, 6, REG_BINARY);
            put_bytes(out, bytes);
        }
        LegacyValue::Other { kind, bytes } => {
            put_header(out, 7, *kind);
            put_bytes(out, bytes);
        }
    }
}

/// Variant tag (one byte) and registry type code (u32 LE).
fn put_header(out: &mut Vec<u8>, tag: u8, type_code: u32) {
    out.push(tag);
    out.extend_from_slice(&type_code.to_le_bytes());
}

/// Length as u64 LE.
fn put_len(out: &mut Vec<u8>, len: usize) {
    let len = u64::try_from(len).unwrap_or(u64::MAX);
    out.extend_from_slice(&len.to_le_bytes());
}

/// Length-prefixed bytes.
fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    put_len(out, bytes.len());
    out.extend_from_slice(bytes);
}
