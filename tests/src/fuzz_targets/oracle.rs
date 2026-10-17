//! Reference computations built on `std` alone, so that the targets never use the code under
//! test as its own oracle.

/// True for a high (leading) surrogate.
pub(crate) const fn is_high(unit: u16) -> bool {
    matches!(unit, 0xD800..=0xDBFF)
}

/// True for a low (trailing) surrogate.
pub(crate) const fn is_low(unit: u16) -> bool {
    matches!(unit, 0xDC00..=0xDFFF)
}

/// True when `units` is well-formed UTF-16 according to `std`.
pub(crate) fn well_formed(units: &[u16]) -> bool {
    char::decode_utf16(units.iter().copied()).all(|item| item.is_ok())
}

/// True when a cut at `index` separates a high surrogate from the low surrogate after it.
pub(crate) fn splits_pair(units: &[u16], index: usize) -> bool {
    index > 0 && index < units.len() && is_high(units[index - 1]) && is_low(units[index])
}

/// One code point of a UTF-16 sequence as `std` decodes it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Item {
    /// Index of the first unit.
    pub(crate) start: usize,
    /// Units used: 2 for a pair, 1 otherwise.
    pub(crate) len: usize,
    /// The scalar, or `None` for an unpaired surrogate.
    pub(crate) scalar: Option<char>,
}

/// Splits `units` into code points; each unpaired surrogate is an item of its own.
pub(crate) fn decode_items(units: &[u16]) -> Vec<Item> {
    let mut items = Vec::new();
    let mut start = 0;
    for decoded in char::decode_utf16(units.iter().copied()) {
        let (len, scalar) = match decoded {
            Ok(c) => (c.len_utf16(), Some(c)),
            Err(_) => (1, None),
        };
        items.push(Item { start, len, scalar });
        start += len;
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_cover_every_unit() {
        let units = [0x61, 0xD83D, 0xDE00, 0xDC00, 0xD800];
        let items = decode_items(&units);
        let shape: Vec<(usize, usize, bool)> = items
            .iter()
            .map(|item| (item.start, item.len, item.scalar.is_some()))
            .collect();
        assert_eq!(
            shape,
            [(0, 1, true), (1, 2, true), (3, 1, false), (4, 1, false)]
        );
        assert!(!well_formed(&units));
        assert!(well_formed(&units[..3]));
        assert!(is_high(0xDBFF) && !is_high(0xDC00) && is_low(0xDFFF) && !is_low(0xD800));
        assert!(splits_pair(&units, 2) && !splits_pair(&units, 1) && !splits_pair(&units, 4));
        assert!(!splits_pair(&units, 0) && !splits_pair(&units, 5) && !splits_pair(&units, 9));
    }
}
