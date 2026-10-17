//! `convert` (plan §8, §10.2): files through `raumakey_converter::convert_file` in a temporary
//! directory, the clipboard through an in-memory `ClipboardPort`. Clipboard text never appears
//! in the output.

mod support;

use std::fs;
use std::path::Path;

use raumakey_controller::cli::{ConvertArgs, ConvertTarget, EXIT_FAILED, EXIT_OK};
use raumakey_controller::convert::convert;
use raumakey_converter::{Charset, ClipboardError};

use support::{Captured, FakeClipboard, capture};

/// "Tiếng Việt" in TCVN3: the bytes, and the Windows-1252 text form the clipboard holds.
const TCVN3_BYTES: &[u8] = b"Ti\xD5ng Vi\xD6t";
const TCVN3_TEXT: &str = "TiÕng ViÖt";
const UNICODE_TEXT: &str = "Tiếng Việt";

fn file_args(input: &Path, output: &Path, overwrite: bool) -> ConvertArgs {
    ConvertArgs {
        from: Charset::Tcvn3,
        to: Charset::Unicode,
        target: ConvertTarget::Files {
            input: input.to_path_buf(),
            output: output.to_path_buf(),
            overwrite,
        },
    }
}

fn clipboard_args(from: Charset, to: Charset) -> ConvertArgs {
    ConvertArgs {
        from,
        to,
        target: ConvertTarget::Clipboard,
    }
}

fn run(args: &ConvertArgs, clipboard: &mut FakeClipboard) -> Captured {
    capture(|output| convert(args, clipboard, output))
}

fn source(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("source.txt");
    fs::write(&path, TCVN3_BYTES).unwrap();
    path
}

#[test]
fn a_file_is_converted_into_a_new_file() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let output = dir.path().join("out.txt");
    let mut clipboard = FakeClipboard::with_text("untouched");
    let result = run(&file_args(&input, &output, false), &mut clipboard);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(fs::read_to_string(&output).unwrap(), UNICODE_TEXT);
    assert_eq!(fs::read(&input).unwrap(), TCVN3_BYTES);
    assert!(result.out.contains("tcvn3") && result.out.contains("unicode"));
    assert!(
        result.out.contains(&output.display().to_string()),
        "{}",
        result.out
    );
    assert!(!result.out.contains(UNICODE_TEXT), "no file content");
    assert!(result.err.is_empty(), "{}", result.err);
    assert_eq!(clipboard.writes, 0);
}

#[test]
fn an_existing_destination_is_replaced_only_with_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let output = dir.path().join("out.txt");
    fs::write(&output, "keep").unwrap();
    let mut clipboard = FakeClipboard::default();

    let result = run(&file_args(&input, &output, false), &mut clipboard);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(fs::read_to_string(&output).unwrap(), "keep");
    assert!(result.err.contains("already exists"), "{}", result.err);
    assert!(result.err.contains("--overwrite"), "{}", result.err);

    let result = run(&file_args(&input, &output, true), &mut clipboard);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(fs::read_to_string(&output).unwrap(), UNICODE_TEXT);
    assert!(result.out.contains("replaced"), "{}", result.out);
}

#[test]
fn the_source_is_never_the_destination() {
    let dir = tempfile::tempdir().unwrap();
    let input = source(dir.path());
    let result = run(
        &file_args(&input, &input, true),
        &mut FakeClipboard::default(),
    );
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(fs::read(&input).unwrap(), TCVN3_BYTES);
    assert!(result.err.contains("same file"), "{}", result.err);
}

#[test]
fn a_missing_source_fails() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("missing.txt");
    let output = dir.path().join("out.txt");
    let result = run(
        &file_args(&input, &output, false),
        &mut FakeClipboard::default(),
    );
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(!output.exists());
    assert!(
        result.err.contains(&input.display().to_string()),
        "{}",
        result.err
    );
}

#[test]
fn a_lossy_conversion_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("upper.txt");
    // TCVN3 has no uppercase letters with tone marks.
    fs::write(&input, "TIẾNG VIỆT").unwrap();
    let output = dir.path().join("out.txt");
    let args = ConvertArgs {
        from: Charset::Unicode,
        to: Charset::Tcvn3,
        target: ConvertTarget::Files {
            input: input.clone(),
            output: output.clone(),
            overwrite: false,
        },
    };
    let result = run(&args, &mut FakeClipboard::default());
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(output.exists());
    assert!(
        result.out.contains("could not be written exactly"),
        "{}",
        result.out
    );
}

#[test]
fn the_clipboard_text_is_converted_in_place_and_never_printed() {
    let mut clipboard = FakeClipboard::with_text(TCVN3_TEXT);
    let result = run(
        &clipboard_args(Charset::Tcvn3, Charset::Unicode),
        &mut clipboard,
    );
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(clipboard.text().as_deref(), Some(UNICODE_TEXT));
    for text in [TCVN3_TEXT, UNICODE_TEXT] {
        assert!(!result.out.contains(text) && !result.err.contains(text));
    }
    assert!(result.out.contains("clipboard"), "{}", result.out);
}

#[test]
fn clipboard_text_that_does_not_change_is_left_alone() {
    let mut clipboard = FakeClipboard::with_text("abc");
    let result = run(
        &clipboard_args(Charset::Unicode, Charset::Unicode),
        &mut clipboard,
    );
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(clipboard.writes, 0);
}

#[test]
fn a_clipboard_without_text_is_a_failure() {
    let mut clipboard = FakeClipboard::default();
    let result = run(
        &clipboard_args(Charset::Tcvn3, Charset::Unicode),
        &mut clipboard,
    );
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(result.err.contains("no text"), "{}", result.err);
}

#[test]
fn newer_clipboard_content_is_never_overwritten() {
    let mut clipboard = FakeClipboard::with_text(TCVN3_TEXT);
    clipboard.changed_by_another_application = true;
    let result = run(
        &clipboard_args(Charset::Tcvn3, Charset::Unicode),
        &mut clipboard,
    );
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(clipboard.writes, 0);
    assert_eq!(clipboard.text().as_deref(), Some(TCVN3_TEXT));
    assert!(result.err.contains("changed"), "{}", result.err);
    for text in [TCVN3_TEXT, UNICODE_TEXT] {
        assert!(!result.out.contains(text) && !result.err.contains(text));
    }
}

#[test]
fn an_unavailable_clipboard_is_a_failure() {
    let mut clipboard = FakeClipboard::with_text(TCVN3_TEXT);
    clipboard.read_error = Some(ClipboardError::Unavailable);
    let result = run(
        &clipboard_args(Charset::Tcvn3, Charset::Unicode),
        &mut clipboard,
    );
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(result.err.contains("unavailable"), "{}", result.err);
}
