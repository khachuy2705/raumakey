//! UTF-8 literal: the UTF-8 bytes of a text shown as Windows-1252 characters (`Tiáº¿ng`).
//!
//! Decoding is lenient: byte runs that are not valid UTF-8, and characters that have no
//! Windows-1252 byte, are kept as they are.

use crate::codepage::{cp1252_to_byte, cp1252_to_char};

/// Writes the UTF-8 bytes of `text` as Windows-1252 characters.
pub(crate) fn encode(text: &str) -> String {
    text.bytes().map(cp1252_to_char).collect()
}

/// Reads Windows-1252 characters back as UTF-8 bytes.
pub(crate) fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = Vec::new();
    for c in text.chars() {
        match cp1252_to_byte(c) {
            Some(byte) => run.push(byte),
            None => {
                flush(&mut run, &mut out);
                out.push(c);
            }
        }
    }
    flush(&mut run, &mut out);
    out
}

fn flush(run: &mut Vec<u8>, out: &mut String) {
    for chunk in run.utf8_chunks() {
        out.push_str(chunk.valid());
        out.extend(chunk.invalid().iter().copied().map(cp1252_to_char));
    }
    run.clear();
}
