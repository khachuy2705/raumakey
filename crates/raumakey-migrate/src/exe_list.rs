//! Recognition of setting values that list executable names (plan D18, matrix #28).

use std::collections::BTreeSet;

use raumakey_model::host::ExeName;

/// The executables listed by `text`, when it is a list of executable names.
///
/// `text` is split at `,`, `;`, `|`, and whitespace (including line breaks); empty items are
/// ignored. The value is a list only when there is at least one item and **every** item is an
/// executable basename: a valid [`ExeName`] that ends in `.exe` after lowercasing, has a stem
/// that is not only dots, and contains no character Windows forbids in file names (`< > : "
/// / \ | ? *` or a control character). Members are lowercased and deduplicated, keeping the
/// first occurrence. Paths, wildcards, and lists with any other item are not lists, so a value
/// is never imported partially.
///
/// ```
/// use raumakey_migrate::exe_list;
///
/// let members = exe_list("Game.exe; editor.EXE").unwrap();
/// assert_eq!(members[1].as_str(), "editor.exe");
/// assert_eq!(exe_list("game.exe;readme.txt"), None);
/// ```
pub fn exe_list(text: &str) -> Option<Vec<ExeName>> {
    let mut members = Vec::new();
    let mut seen = BTreeSet::new();
    let items = text
        .split(|c: char| matches!(c, ',' | ';' | '|') || c.is_whitespace())
        .filter(|item| !item.is_empty());
    for item in items {
        let name = exe_basename(item)?;
        if seen.insert(name.clone()) {
            members.push(name);
        }
    }
    (!members.is_empty()).then_some(members)
}

/// `item` as an executable basename, if it is one.
fn exe_basename(item: &str) -> Option<ExeName> {
    let forbidden = |c: char| matches!(c, '<' | '>' | '"' | '?' | '*') || c.is_control();
    if item.chars().any(forbidden) {
        return None;
    }
    let name = ExeName::new(item).ok()?;
    let stem = name.as_str().strip_suffix(".exe")?;
    if stem.chars().all(|c| c == '.') {
        return None;
    }
    Some(name)
}
