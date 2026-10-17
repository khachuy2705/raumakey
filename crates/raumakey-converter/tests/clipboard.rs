//! Clipboard conversion through a fake port: secure-field refusal, sequence-number ownership,
//! and never writing over newer clipboard content.

use raumakey_converter::{
    CaseTransform, Charset, ClipboardError, ClipboardOutcome, ClipboardPort, ClipboardText,
    ConvertError, ConvertOptions, MAX_INPUT_BYTES, convert_clipboard,
};

fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// In-memory clipboard. Every content change advances the sequence number, as on Windows, and
/// both port methods are atomic, as an adapter that keeps the clipboard open makes them.
#[derive(Default)]
struct FakeClipboard {
    sequence: u32,
    units: Option<Vec<u16>>,
    reads: usize,
    /// Calls of the conditional write, whether or not it wrote.
    write_calls: usize,
    /// Times the content was replaced.
    writes: usize,
    read_error: Option<ClipboardError>,
    write_error: Option<ClipboardError>,
    /// Another application replaces the clipboard right after Raumakey reads it.
    racing_writer: Option<String>,
    /// The owner uses delayed rendering: it renders the text when Raumakey asks for it, which
    /// advances the sequence number during the read without changing the content.
    renders_on_read: bool,
}

impl FakeClipboard {
    fn with_text(text: &str) -> Self {
        Self {
            sequence: 41,
            units: Some(utf16(text)),
            ..Self::default()
        }
    }

    fn text(&self) -> Option<String> {
        self.units
            .as_deref()
            .map(|units| String::from_utf16(units).unwrap())
    }
}

impl ClipboardPort for FakeClipboard {
    fn read_text(&mut self) -> Result<Option<ClipboardText>, ClipboardError> {
        self.reads += 1;
        if let Some(error) = self.read_error {
            return Err(error);
        }
        if std::mem::take(&mut self.renders_on_read) {
            self.sequence += 1;
        }
        let read = self.units.clone().map(|units| ClipboardText {
            units,
            sequence: self.sequence,
        });
        if let Some(newer) = self.racing_writer.take() {
            self.units = Some(utf16(&newer));
            self.sequence += 1;
        }
        Ok(read)
    }

    fn write_text_if_unchanged(
        &mut self,
        expected_sequence: u32,
        units: &[u16],
    ) -> Result<bool, ClipboardError> {
        self.write_calls += 1;
        if let Some(error) = self.write_error {
            return Err(error);
        }
        if self.sequence != expected_sequence {
            return Ok(false);
        }
        self.units = Some(units.to_vec());
        self.sequence += 1;
        self.writes += 1;
        Ok(true)
    }
}

fn run(
    port: &mut FakeClipboard,
    from: Charset,
    to: Charset,
) -> Result<ClipboardOutcome, ConvertError> {
    convert_clipboard(port, from, to, ConvertOptions::default(), false)
}

#[test]
fn secure_fields_are_refused_without_touching_the_clipboard() {
    let mut port = FakeClipboard::with_text("TiÕng ViÖt");
    let result = convert_clipboard(
        &mut port,
        Charset::Tcvn3,
        Charset::Unicode,
        ConvertOptions::default(),
        true,
    );
    assert_eq!(result, Err(ConvertError::SecureField));
    assert_eq!((port.reads, port.write_calls), (0, 0));
    assert_eq!(port.text().as_deref(), Some("TiÕng ViÖt"));
}

#[test]
fn converted_text_replaces_the_clipboard() {
    let mut port = FakeClipboard::with_text("TiÕng ViÖt");
    let outcome = run(&mut port, Charset::Tcvn3, Charset::Unicode).unwrap();
    assert_eq!(outcome, ClipboardOutcome::Converted { lossy: false });
    assert_eq!(port.text().as_deref(), Some("Tiếng Việt"));
    assert_eq!((port.writes, port.sequence), (1, 42));
}

#[test]
fn loss_is_reported() {
    let mut port = FakeClipboard::with_text("TIẾNG VIỆT");
    let outcome = run(&mut port, Charset::Unicode, Charset::Tcvn3).unwrap();
    assert_eq!(outcome, ClipboardOutcome::Converted { lossy: true });
    assert_eq!(port.text().as_deref(), Some("TIÕNG VIÖT"));
}

#[test]
fn options_are_applied() {
    let mut port = FakeClipboard::with_text("Tiếng Việt");
    let options = ConvertOptions {
        case: CaseTransform::Lower,
        remove_tones: true,
    };
    let outcome = convert_clipboard(
        &mut port,
        Charset::Unicode,
        Charset::Unicode,
        options,
        false,
    )
    .unwrap();
    assert_eq!(outcome, ClipboardOutcome::Converted { lossy: false });
    assert_eq!(port.text().as_deref(), Some("tieng viet"));
}

#[test]
fn text_outside_the_basic_multilingual_plane_survives() {
    let mut port = FakeClipboard::with_text("TiÕng ViÖt 😀");
    let outcome = run(&mut port, Charset::Tcvn3, Charset::Unicode).unwrap();
    assert_eq!(outcome, ClipboardOutcome::Converted { lossy: false });
    assert_eq!(port.text().as_deref(), Some("Tiếng Việt 😀"));
}

#[test]
fn an_empty_clipboard_is_left_alone() {
    let mut port = FakeClipboard::default();
    assert_eq!(
        run(&mut port, Charset::Tcvn3, Charset::Unicode),
        Ok(ClipboardOutcome::NoText)
    );
    assert_eq!(port.write_calls, 0);
}

#[test]
fn text_that_does_not_change_is_not_written_back() {
    let mut port = FakeClipboard::with_text("plain ASCII text");
    assert_eq!(
        run(&mut port, Charset::Tcvn3, Charset::Unicode),
        Ok(ClipboardOutcome::Unchanged)
    );
    assert_eq!((port.write_calls, port.sequence), (0, 41));
}

#[test]
fn newer_clipboard_content_is_never_overwritten() {
    let mut port = FakeClipboard::with_text("TiÕng ViÖt");
    port.racing_writer = Some("newer content from another application".to_owned());
    assert_eq!(
        run(&mut port, Charset::Tcvn3, Charset::Unicode),
        Err(ConvertError::OwnershipConflict)
    );
    // The conditional write was asked once and refused; nothing was written or restored.
    assert_eq!((port.write_calls, port.writes), (1, 0));
    assert_eq!(
        port.text().as_deref(),
        Some("newer content from another application")
    );
}

#[test]
fn delayed_rendering_during_the_read_is_not_an_ownership_conflict() {
    let mut port = FakeClipboard::with_text("TiÕng ViÖt");
    port.renders_on_read = true;
    let outcome = run(&mut port, Charset::Tcvn3, Charset::Unicode);
    assert_eq!(outcome, Ok(ClipboardOutcome::Converted { lossy: false }));
    assert_eq!(port.text().as_deref(), Some("Tiếng Việt"));
    assert_eq!(port.writes, 1);
}

#[test]
fn port_failures_are_reported_and_nothing_is_restored() {
    let mut port = FakeClipboard::with_text("TiÕng ViÖt");
    port.read_error = Some(ClipboardError::Unavailable);
    assert_eq!(
        run(&mut port, Charset::Tcvn3, Charset::Unicode),
        Err(ConvertError::Clipboard(ClipboardError::Unavailable))
    );
    assert_eq!(port.write_calls, 0);

    let mut port = FakeClipboard::with_text("TiÕng ViÖt");
    port.write_error = Some(ClipboardError::WriteFailed);
    assert_eq!(
        run(&mut port, Charset::Tcvn3, Charset::Unicode),
        Err(ConvertError::Clipboard(ClipboardError::WriteFailed))
    );
    // One attempt, no retry and no second write to "restore" anything.
    assert_eq!(port.write_calls, 1);
    assert_eq!(port.text().as_deref(), Some("TiÕng ViÖt"));
}

#[test]
fn ill_formed_utf16_is_refused_and_nothing_is_written() {
    // An unpaired high surrogate at the end, an unpaired low surrogate in the middle, and a
    // reversed pair. Decoding them lossily would write U+FFFD back over the user's text.
    for broken in [
        [utf16("TiÕng ViÖt"), vec![0xD83D]].concat(),
        [utf16("TiÕng"), vec![0xDE00], utf16(" ViÖt")].concat(),
        [utf16("TiÕng "), vec![0xDE00, 0xD83D], utf16("ViÖt")].concat(),
    ] {
        let mut port = FakeClipboard {
            sequence: 41,
            units: Some(broken.clone()),
            ..FakeClipboard::default()
        };
        assert_eq!(
            run(&mut port, Charset::Tcvn3, Charset::Unicode),
            Err(ConvertError::MalformedInput)
        );
        assert_eq!(port.write_calls, 0);
        assert_eq!(port.units, Some(broken));
    }
}

#[test]
fn oversized_clipboard_text_is_refused() {
    let mut port = FakeClipboard::with_text(&"a".repeat(MAX_INPUT_BYTES + 1));
    assert_eq!(
        run(&mut port, Charset::Tcvn3, Charset::Unicode),
        Err(ConvertError::InputTooLarge)
    );
    assert_eq!(port.write_calls, 0);

    // Within the limit in UTF-16 units but not in UTF-8 bytes.
    let mut port = FakeClipboard::with_text(&"ế".repeat(MAX_INPUT_BYTES / 3 + 1));
    assert_eq!(
        run(&mut port, Charset::Unicode, Charset::Tcvn3),
        Err(ConvertError::InputTooLarge)
    );
    assert_eq!(port.write_calls, 0);
}

/// A port that records the conditional writes it is asked for.
struct RecordingPort {
    sequence: u32,
    text: String,
    conditional_writes: Vec<(u32, String)>,
    owner_changed: bool,
}

impl ClipboardPort for RecordingPort {
    fn read_text(&mut self) -> Result<Option<ClipboardText>, ClipboardError> {
        Ok(Some(ClipboardText {
            units: utf16(&self.text),
            sequence: self.sequence,
        }))
    }

    fn write_text_if_unchanged(
        &mut self,
        expected_sequence: u32,
        units: &[u16],
    ) -> Result<bool, ClipboardError> {
        let text = String::from_utf16(units).unwrap();
        self.conditional_writes.push((expected_sequence, text));
        Ok(!self.owner_changed)
    }
}

#[test]
fn the_conditional_write_receives_the_sequence_read_with_the_text() {
    let mut port = RecordingPort {
        sequence: 7,
        text: "TiÕng".to_owned(),
        conditional_writes: Vec::new(),
        owner_changed: false,
    };
    let outcome = convert_clipboard(
        &mut port,
        Charset::Tcvn3,
        Charset::Unicode,
        ConvertOptions::default(),
        false,
    );
    assert_eq!(outcome, Ok(ClipboardOutcome::Converted { lossy: false }));
    assert_eq!(port.conditional_writes, [(7, "Tiếng".to_owned())]);

    port.owner_changed = true;
    let dyn_port: &mut dyn ClipboardPort = &mut port;
    let outcome = convert_clipboard(
        dyn_port,
        Charset::Tcvn3,
        Charset::Unicode,
        ConvertOptions::default(),
        false,
    );
    assert_eq!(outcome, Err(ConvertError::OwnershipConflict));
}
