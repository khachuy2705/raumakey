//! Structure-aware decoding of fuzz input: every byte string decodes to some bounded input.
//!
//! Each decision reads whole bytes in order. An exhausted reader yields zeros, so decoding never
//! fails; loops that decode a variable number of items stop at a count read from the input or
//! when [`Reader::is_empty`] reports that the bytes ran out.

/// Cursor over fuzz input bytes.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    /// A reader at the start of `data`.
    pub(crate) const fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    /// True when every byte has been read.
    pub(crate) fn is_empty(&self) -> bool {
        self.position >= self.data.len()
    }

    /// The next byte, or 0 when exhausted.
    pub(crate) fn byte(&mut self) -> u8 {
        let byte = self.data.get(self.position).copied().unwrap_or(0);
        self.position = self.position.saturating_add(1);
        byte
    }

    /// The low bit of the next byte.
    pub(crate) fn flag(&mut self) -> bool {
        self.byte() & 1 == 1
    }

    /// True with a chance of `threshold / 256`: when the next byte is one of the `threshold`
    /// largest values. Zero bytes, and so an exhausted reader, never switch a rare choice on.
    pub(crate) fn chance(&mut self, threshold: u8) -> bool {
        self.byte() > u8::MAX - threshold
    }

    /// The next two bytes, little-endian.
    pub(crate) fn u16(&mut self) -> u16 {
        u16::from_le_bytes([self.byte(), self.byte()])
    }

    /// The next four bytes, little-endian.
    pub(crate) fn u32(&mut self) -> u32 {
        u32::from_le_bytes([self.byte(), self.byte(), self.byte(), self.byte()])
    }

    /// A value in `0..bound` (0 when `bound` is 0): one byte for bounds up to 256, two bytes
    /// otherwise.
    pub(crate) fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        let raw = if bound <= 256 {
            usize::from(self.byte())
        } else {
            usize::from(self.u16())
        };
        raw % bound
    }

    /// One element of `items`, which must not be empty.
    pub(crate) fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        let index = self.below(items.len());
        items[index]
    }

    /// Up to `len` of the remaining bytes.
    pub(crate) fn take(&mut self, len: usize) -> &'a [u8] {
        let start = self.position.min(self.data.len());
        let end = start.saturating_add(len).min(self.data.len());
        self.position = end;
        &self.data[start..end]
    }
}

#[cfg(test)]
mod tests {
    use super::Reader;

    #[test]
    fn exhausted_input_reads_as_zeros() {
        let mut reader = Reader::new(&[7]);
        assert!(!reader.is_empty());
        assert_eq!(reader.byte(), 7);
        assert!(reader.is_empty());
        assert_eq!(reader.byte(), 0);
        assert_eq!(reader.u32(), 0);
        assert!(!reader.flag());
        assert_eq!(reader.below(10), 0);
        assert!(reader.take(5).is_empty());
        assert!(reader.take(usize::MAX).is_empty());
    }

    #[test]
    fn numbers_are_little_endian_and_bounded() {
        let mut reader = Reader::new(&[0x34, 0x12, 1, 0, 0, 0, 9, 0xFF, 0xFF, 3]);
        assert_eq!(reader.u16(), 0x1234);
        assert_eq!(reader.u32(), 1);
        assert_eq!(reader.below(4), 1);
        assert_eq!(reader.below(1000), 0xFFFF % 1000);
        assert_eq!(reader.pick(&['a', 'b']), 'b');
        assert_eq!(reader.below(0), 0);
    }

    #[test]
    fn take_stops_at_the_end() {
        let mut reader = Reader::new(&[1, 2, 0xFC, 4]);
        assert_eq!(reader.take(2), [1, 2]);
        assert!(reader.chance(4));
        assert_eq!(reader.take(usize::MAX), [4]);
        assert!(reader.take(usize::MAX).is_empty());
    }

    #[test]
    fn rare_choices_are_off_for_zero_bytes() {
        // Exhausted input reads as zeros; it must not switch on rare (expensive) choices.
        let mut reader = Reader::new(&[0, 251, 252, 255]);
        assert!(!reader.chance(4));
        assert!(!reader.chance(4));
        assert!(reader.chance(4));
        assert!(!reader.chance(0));
        assert!(!reader.chance(255));
    }
}
