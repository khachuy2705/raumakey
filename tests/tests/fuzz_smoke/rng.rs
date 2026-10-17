//! A small deterministic generator for the smoke inputs (no external crate).
//!
//! [`SplitMix64`] is Steele, Lea, and Flood's splitmix64: a 64-bit state advanced by the golden
//! ratio increment and mixed by two xor-shift-multiply rounds. It is fast, has no bad seeds, and
//! produces the same sequence on every host, so a seed and an input number identify an input.

/// The splitmix64 generator.
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// A generator whose first output is the splitmix64 output for `seed`.
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..bound` (0 when `bound` is 0). The modulo bias is irrelevant here.
    pub fn below(&mut self, bound: usize) -> usize {
        let bound = u64::try_from(bound).unwrap_or(u64::MAX);
        if bound == 0 {
            return 0;
        }
        usize::try_from(self.next_u64() % bound).unwrap_or(0)
    }

    /// A value in `low..=high`.
    pub fn range(&mut self, low: usize, high: usize) -> usize {
        low + self.below(high.saturating_sub(low).saturating_add(1))
    }

    /// True with probability `percent` / 100.
    pub fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    /// One element of a non-empty slice.
    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }

    /// One random byte.
    pub fn byte(&mut self) -> u8 {
        self.next_u64().to_le_bytes()[0]
    }

    /// `len` random bytes.
    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.byte()).collect()
    }
}

#[test]
fn splitmix64_matches_the_reference_sequence() {
    // Reference values of splitmix64 for seed 0 and seed 1234567.
    let mut zero = SplitMix64::new(0);
    assert_eq!(zero.next_u64(), 0xE220_A839_7B1D_CDAF);
    assert_eq!(zero.next_u64(), 0x6E78_9E6A_A1B9_65F4);
    let mut seeded = SplitMix64::new(1_234_567);
    assert_eq!(seeded.next_u64(), 6_457_827_717_110_365_317);
    assert_eq!(seeded.next_u64(), 3_203_168_211_198_807_973);
}

#[test]
fn helpers_stay_in_range() {
    let mut rng = SplitMix64::new(7);
    for _ in 0..1000 {
        assert!(rng.below(10) < 10);
        assert_eq!(rng.below(0), 0);
        let value = rng.range(3, 5);
        assert!((3..=5).contains(&value));
    }
    assert_eq!(rng.bytes(17).len(), 17);
}
