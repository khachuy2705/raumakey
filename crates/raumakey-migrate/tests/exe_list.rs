//! The heuristic that recognises values listing executable names (plan D18, matrix #28).

use raumakey_migrate::exe_list;
use raumakey_model::host::ExeName;

fn names(list: &[&str]) -> Vec<ExeName> {
    list.iter()
        .map(|name| ExeName::new(name).unwrap())
        .collect()
}

#[test]
fn every_separator_splits_items() {
    assert_eq!(
        exe_list("a.exe,b.exe;c.exe|d.exe e.exe\tf.exe\r\ng.exe\nh.exe\u{a0}i.exe"),
        Some(names(&[
            "a.exe", "b.exe", "c.exe", "d.exe", "e.exe", "f.exe", "g.exe", "h.exe", "i.exe"
        ]))
    );
}

#[test]
fn empty_items_are_ignored() {
    assert_eq!(exe_list(" ;; a.exe ,, | "), Some(names(&["a.exe"])));
}

#[test]
fn names_are_lowercased_and_deduplicated_in_order() {
    assert_eq!(
        exe_list("Game.EXE;Other.exe;game.exe"),
        Some(names(&["game.exe", "other.exe"]))
    );
}

#[test]
fn a_single_name_is_a_list() {
    assert_eq!(exe_list("notepad.exe"), Some(names(&["notepad.exe"])));
}

#[test]
fn unicode_basenames_are_accepted() {
    assert_eq!(
        exe_list("trình-duyệt.exe;app_2.0.exe"),
        Some(names(&["trình-duyệt.exe", "app_2.0.exe"]))
    );
}

#[test]
fn every_item_must_be_an_exe_basename() {
    let long = format!("{}.exe", "a".repeat(252));
    let rejected = [
        "",
        "   ",
        ";,|",
        "a.exe;readme.txt",
        "a.exe;b",
        "C:\\Tools\\a.exe",
        "tools/a.exe",
        "drive:a.exe",
        "*.exe",
        "a?.exe",
        "\"a.exe\"",
        "<a>.exe",
        ".exe",
        "...exe",
        "a.exe;.exe",
        "a.exe\u{0}b.exe",
        "a.exe\u{7}",
        "1",
        "true",
        "exe",
        "a.exe.bak",
        long.as_str(),
    ];
    for text in rejected {
        assert_eq!(exe_list(text), None, "{text:?}");
    }
    let longest = format!("{}.exe", "a".repeat(251));
    assert!(exe_list(&longest).is_some());
}
