//! `clipboard`: snapshot/restore and text encoding logic over a fake clipboard. The real
//! clipboard is only touched by `#[ignore]`d manual tests.

use raumakey_converter::ClipboardError;
use raumakey_windows::clipboard::{
    ClipboardAccess, ClipboardSnapshot, FormatStorage, RestoreOutcome, SnapshotError,
    classify_format, decode_unicode_units, encode_unicode_units, formats, plan_snapshot,
    read_unicode_units, write_unicode_units,
};

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn text_bytes(text: &str) -> Vec<u8> {
    encode_unicode_units(&units(text))
}

/// In-memory clipboard that bumps its sequence number like Windows does.
#[derive(Default)]
struct FakeClipboard {
    sequence: u32,
    data: Vec<(u32, Vec<u8>)>,
    fail_reads: bool,
}

impl FakeClipboard {
    fn with(data: Vec<(u32, Vec<u8>)>) -> Self {
        Self {
            sequence: 10,
            data,
            fail_reads: false,
        }
    }
}

impl ClipboardAccess for FakeClipboard {
    fn sequence_number(&self) -> u32 {
        self.sequence
    }

    fn available_formats(&mut self) -> Result<Vec<u32>, ClipboardError> {
        Ok(self.data.iter().map(|(format, _)| *format).collect())
    }

    fn read_format(&mut self, format: u32) -> Result<Option<Vec<u8>>, ClipboardError> {
        if self.fail_reads {
            return Err(ClipboardError::ReadFailed);
        }
        Ok(self
            .data
            .iter()
            .find(|(candidate, _)| *candidate == format)
            .map(|(_, bytes)| bytes.clone()))
    }

    fn clear(&mut self) -> Result<(), ClipboardError> {
        self.data.clear();
        self.sequence += 1;
        Ok(())
    }

    fn write_format(&mut self, format: u32, data: &[u8]) -> Result<(), ClipboardError> {
        self.data.retain(|(candidate, _)| *candidate != format);
        self.data.push((format, data.to_vec()));
        self.sequence += 1;
        Ok(())
    }
}

#[test]
fn formats_are_classified_by_how_their_data_is_stored() {
    for global in [
        formats::CF_TEXT,
        formats::CF_OEMTEXT,
        formats::CF_UNICODETEXT,
        formats::CF_DIB,
        formats::CF_DIBV5,
        formats::CF_HDROP,
        formats::CF_LOCALE,
        0x0081,
        0xC000,
        0xFFFF,
    ] {
        assert_eq!(
            classify_format(global),
            FormatStorage::Global,
            "{global:#x}"
        );
    }
    for handle in [
        formats::CF_BITMAP,
        formats::CF_METAFILEPICT,
        formats::CF_PALETTE,
        formats::CF_ENHMETAFILE,
        0x0082,
        0x0083,
        0x008E,
        0x0300,
        0x03FF,
    ] {
        assert_eq!(
            classify_format(handle),
            FormatStorage::Handle,
            "{handle:#x}"
        );
    }
    for unsupported in [0, 0x0080, 0x0200, 0x02FF] {
        assert_eq!(
            classify_format(unsupported),
            FormatStorage::Unsupported,
            "{unsupported:#x}"
        );
    }
}

#[test]
fn snapshot_plans_skip_synthesized_handles_and_refuse_real_ones() {
    let text = [
        formats::CF_UNICODETEXT,
        formats::CF_LOCALE,
        formats::CF_TEXT,
        formats::CF_OEMTEXT,
    ];
    assert_eq!(plan_snapshot(&text).unwrap(), text);
    assert_eq!(
        plan_snapshot(&[
            formats::CF_DIB,
            formats::CF_BITMAP,
            formats::CF_PALETTE,
            formats::CF_DIBV5
        ])
        .unwrap(),
        [formats::CF_DIB, formats::CF_DIBV5]
    );
    assert_eq!(
        plan_snapshot(&[formats::CF_BITMAP]),
        Err(SnapshotError::NonGlobalFormat(formats::CF_BITMAP))
    );
    assert_eq!(
        plan_snapshot(&[formats::CF_ENHMETAFILE, formats::CF_METAFILEPICT]),
        Err(SnapshotError::NonGlobalFormat(formats::CF_ENHMETAFILE))
    );
    assert_eq!(
        plan_snapshot(&[formats::CF_UNICODETEXT, 0x0200]),
        Err(SnapshotError::NonGlobalFormat(0x0200))
    );
    assert!(plan_snapshot(&[]).unwrap().is_empty());
}

#[test]
fn a_snapshot_captures_bytes_and_the_sequence() {
    let mut clipboard = FakeClipboard::with(vec![
        (formats::CF_UNICODETEXT, text_bytes("xin chào")),
        (0xC0FE, vec![1, 2, 3]),
    ]);
    let snapshot = ClipboardSnapshot::capture(&mut clipboard).unwrap();
    assert_eq!(snapshot.sequence(), 10);
    assert_eq!(snapshot.format_count(), 2);
    assert_eq!(snapshot.total_bytes(), text_bytes("xin chào").len() + 3);
    // Debug never shows the captured bytes.
    let debug = format!("{snapshot:?}");
    assert!(!debug.contains("[1, 2, 3]"), "{debug}");
    assert!(!debug.contains("120"), "{debug}");
}

#[test]
fn snapshots_are_refused_for_handles_size_and_read_errors() {
    let mut images = FakeClipboard::with(vec![(formats::CF_BITMAP, Vec::new())]);
    assert_eq!(
        ClipboardSnapshot::capture(&mut images).unwrap_err(),
        SnapshotError::NonGlobalFormat(formats::CF_BITMAP)
    );
    let mut big = FakeClipboard::with(vec![(0xC001, vec![0; 64]), (0xC002, vec![0; 64])]);
    assert_eq!(
        ClipboardSnapshot::capture_with_limit(&mut big, 100).unwrap_err(),
        SnapshotError::TooLarge
    );
    let mut failing = FakeClipboard::with(vec![(formats::CF_UNICODETEXT, vec![0, 0])]);
    failing.fail_reads = true;
    assert_eq!(
        ClipboardSnapshot::capture(&mut failing).unwrap_err(),
        SnapshotError::Clipboard(ClipboardError::ReadFailed)
    );
}

#[test]
fn restore_happens_only_while_raumakey_owns_the_latest_update() {
    let original = vec![
        (formats::CF_UNICODETEXT, text_bytes("original")),
        (0xC0FE, vec![9]),
    ];
    let mut clipboard = FakeClipboard::with(original.clone());
    let snapshot = ClipboardSnapshot::capture(&mut clipboard).unwrap();
    // Raumakey writes its transient text; the sequence after that write proves ownership.
    write_unicode_units(&mut clipboard, &units("ế"), &[]).unwrap();
    let ours = clipboard.sequence_number();
    assert_eq!(
        snapshot.restore_if_owner(&mut clipboard, ours).unwrap(),
        RestoreOutcome::Restored
    );
    assert_eq!(clipboard.data, original);

    // Another application copied something after Raumakey's write: never restore over it.
    write_unicode_units(&mut clipboard, &units("ế"), &[]).unwrap();
    let ours = clipboard.sequence_number();
    write_unicode_units(&mut clipboard, &units("newer"), &[]).unwrap();
    let before = clipboard.data.clone();
    assert_eq!(
        snapshot.restore_if_owner(&mut clipboard, ours).unwrap(),
        RestoreOutcome::SkippedNewerContent
    );
    assert_eq!(clipboard.data, before);
}

#[test]
fn restoring_an_empty_snapshot_empties_the_clipboard() {
    let mut clipboard = FakeClipboard::with(Vec::new());
    let snapshot = ClipboardSnapshot::capture(&mut clipboard).unwrap();
    write_unicode_units(&mut clipboard, &units("x"), &[(0xC010, vec![0, 0, 0, 0])]).unwrap();
    assert_eq!(clipboard.data.len(), 2, "text plus the extra format");
    let ours = clipboard.sequence_number();
    snapshot.restore_if_owner(&mut clipboard, ours).unwrap();
    assert!(clipboard.data.is_empty());
}

#[test]
fn unicode_units_encode_with_a_terminator_and_decode_up_to_it() {
    assert_eq!(encode_unicode_units(&units("ab")), [0x61, 0, 0x62, 0, 0, 0]);
    assert_eq!(
        decode_unicode_units(&[0x61, 0, 0x62, 0, 0, 0, 0x63, 0]).unwrap(),
        units("ab")
    );
    assert_eq!(decode_unicode_units(&[0x61, 0]).unwrap(), units("a"));
    assert!(decode_unicode_units(&[]).unwrap().is_empty());
    // Units are passed on unvalidated; the converter validates them.
    assert_eq!(decode_unicode_units(&[0x00, 0xD8, 0, 0]).unwrap(), [0xD800]);
    assert_eq!(
        decode_unicode_units(&[0x61, 0, 0x62]),
        Err(ClipboardError::ReadFailed)
    );
    let mut clipboard =
        FakeClipboard::with(vec![(formats::CF_UNICODETEXT, text_bytes("Tiếng Việt 😀"))]);
    assert_eq!(
        read_unicode_units(&mut clipboard).unwrap(),
        Some(units("Tiếng Việt 😀"))
    );
    let mut empty = FakeClipboard::default();
    assert_eq!(read_unicode_units(&mut empty).unwrap(), None);
}

#[cfg(windows)]
mod win {
    use raumakey_converter::ClipboardPort;
    use raumakey_windows::clipboard::WinClipboard;

    #[test]
    fn reading_the_sequence_number_does_not_touch_content() {
        let clipboard = WinClipboard::new();
        let first = clipboard.sequence_number();
        let second = clipboard.sequence_number();
        assert!(second >= first);
    }

    /// Replaces the user's clipboard content. Manual test only.
    #[test]
    #[ignore = "manual: overwrites the real clipboard"]
    fn manual_clipboard_text_round_trip_and_snapshot_restore() {
        let mut clipboard = WinClipboard::new();
        let snapshot = clipboard.capture_snapshot().unwrap();
        let text: Vec<u16> = "Raumakey ế".encode_utf16().collect();
        let sequence = clipboard.write_transient_units(&text).unwrap();
        let read = clipboard.read_text().unwrap().unwrap();
        assert_eq!(read.units, text);
        assert_eq!(read.sequence, sequence);
        let replacement: Vec<u16> = "Raumakey ệ".encode_utf16().collect();
        assert!(
            clipboard
                .write_text_if_unchanged(sequence, &replacement)
                .unwrap()
        );
        let ours = clipboard.sequence_number();
        clipboard.restore_if_owner(&snapshot, ours).unwrap();
    }
}
