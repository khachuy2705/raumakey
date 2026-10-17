//! Surrogate- and combining-mark-safe suffix diff (plan §3.7).

use raumakey_model::utf16;
use unicode_normalization::char::is_combining_mark;

/// The edit that turns `old` into `new`: delete `delete` units before the caret, then insert.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Diff {
    /// UTF-16 units to delete from the end of `old`.
    pub(crate) delete: usize,
    /// UTF-16 units to insert.
    pub(crate) insert: Vec<u16>,
}

/// Computes the minimal suffix edit from `old` to `new`.
///
/// The common prefix is backed off so that the cut never splits a surrogate pair and never
/// separates a base character from the combining marks that follow it.
pub(crate) fn diff(old: &[u16], new: &[u16]) -> Diff {
    let mut prefix = old
        .iter()
        .zip(new)
        .take_while(|(left, right)| left == right)
        .count();
    while prefix > 0 {
        if !utf16::is_scalar_boundary(old, prefix) || !utf16::is_scalar_boundary(new, prefix) {
            prefix -= 1;
        } else if starts_with_combining_mark(old.get(prefix..).unwrap_or_default())
            || starts_with_combining_mark(new.get(prefix..).unwrap_or_default())
        {
            let previous = utf16::last_scalar_len(old.get(..prefix).unwrap_or_default());
            prefix = prefix.saturating_sub(previous.max(1));
        } else {
            break;
        }
    }
    Diff {
        delete: old.len().saturating_sub(prefix),
        insert: new.get(prefix..).unwrap_or_default().to_vec(),
    }
}

/// True when `units` begins with a combining mark (General Category M).
fn starts_with_combining_mark(units: &[u16]) -> bool {
    char::decode_utf16(units.iter().copied())
        .next()
        .and_then(Result::ok)
        .is_some_and(is_combining_mark)
}

#[cfg(test)]
mod tests {
    use raumakey_model::utf16;

    use super::*;

    fn check(old: &str, new: &str, delete: usize, insert: &str) {
        let d = diff(&utf16::encode(old), &utf16::encode(new));
        assert_eq!(
            d,
            Diff {
                delete,
                insert: utf16::encode(insert)
            },
            "{old:?} -> {new:?}"
        );
    }

    #[test]
    fn common_prefix_is_kept() {
        check("a", "â", 1, "â");
        check("tiê", "tiế", 1, "ế");
        check("hòa", "hoàn", 2, "oàn");
        check("a", "ab", 0, "b");
        check("abc", "abc", 0, "");
        check("abc", "a", 2, "");
        check("", "", 0, "");
        check("", "xy", 0, "xy");
    }

    #[test]
    fn never_splits_surrogate_pairs() {
        check("😀a", "😁", 3, "😁");
        check("a😀", "a😁", 2, "😁");
        check("😀", "😀b", 0, "b");
    }

    #[test]
    fn never_separates_base_from_combining_marks() {
        check("e\u{302}", "e\u{302}\u{301}", 2, "e\u{302}\u{301}");
        check("te\u{302}", "te\u{302}\u{301}", 2, "e\u{302}\u{301}");
        check("a", "a\u{302}", 1, "a\u{302}");
        check("a\u{302}", "a", 2, "a");
        check("ba\u{302}n", "ba\u{302}", 1, "");
    }
}
