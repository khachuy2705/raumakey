//! `RT_STRING` string tables.
//!
//! Windows stores string resources in blocks of sixteen. Block `n` (resource name `n`) holds the
//! strings with ids `(n - 1) * 16` to `(n - 1) * 16 + 15`; each string is a 16-bit count of
//! UTF-16 code units followed by the units, without a terminator. Missing strings have length 0.

use std::collections::BTreeMap;

use crate::error::ResourceError;

/// Strings per `RT_STRING` block.
pub const STRINGS_PER_BLOCK: u16 = 16;

/// String resources keyed by 16-bit id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringTable {
    strings: BTreeMap<u16, String>,
}

impl StringTable {
    /// Creates an empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a string. Each id may be used once; a string holds at most 65,535 UTF-16 units.
    pub fn insert(&mut self, id: u16, text: &str) -> Result<(), ResourceError> {
        if self.strings.contains_key(&id) {
            return Err(ResourceError::Duplicate);
        }
        if text.encode_utf16().count() > usize::from(u16::MAX) {
            return Err(ResourceError::TooLarge);
        }
        self.strings.insert(id, text.to_owned());
        Ok(())
    }

    /// Returns the string with this id.
    pub fn get(&self, id: u16) -> Option<&str> {
        self.strings.get(&id).map(String::as_str)
    }

    /// Number of strings.
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Whether the table has no strings.
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Encodes the table as `(block name, block data)` pairs in ascending block order.
    pub(crate) fn blocks(&self) -> Result<Vec<(u16, Vec<u8>)>, ResourceError> {
        let mut grouped: BTreeMap<u16, [Option<&str>; STRINGS_PER_BLOCK as usize]> =
            BTreeMap::new();
        for (&id, text) in &self.strings {
            // id / 16 is at most 4095, so the block name is at most 4096.
            let slots = grouped
                .entry(id / STRINGS_PER_BLOCK + 1)
                .or_insert([None; STRINGS_PER_BLOCK as usize]);
            if let Some(slot) = slots.get_mut(usize::from(id % STRINGS_PER_BLOCK)) {
                *slot = Some(text);
            }
        }
        grouped
            .into_iter()
            .map(|(block, slots)| {
                let mut data = Vec::new();
                for text in slots {
                    let units: Vec<u16> = text.unwrap_or_default().encode_utf16().collect();
                    let count = u16::try_from(units.len()).map_err(|_| ResourceError::TooLarge)?;
                    data.extend_from_slice(&count.to_le_bytes());
                    for unit in units {
                        data.extend_from_slice(&unit.to_le_bytes());
                    }
                }
                Ok((block, data))
            })
            .collect()
    }
}
