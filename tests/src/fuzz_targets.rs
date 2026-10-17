//! Fuzz target bodies (plan §12, design §15.1).
//!
//! Every target is a `pub fn name(data: &[u8])` that accepts any byte string. It decodes the
//! bytes into a bounded input, drives the real crates, and checks invariants with `assert!`: a
//! panic is a finding. Targets never panic on behavior the crates define as valid, and their
//! assertion messages name the broken rule with counts, indices, and error codes, never the
//! generated text. Where practical, reference results come from `std` (`oracle`) or restate the
//! plan; where a target reuses a public helper of the crate under test as its reference (case
//! adaptation, legacy exportability, legacy response validation) or restates a rule of that
//! crate that the plan does not state (the replay rules `TextSimulator` adds to plan §2.7,
//! `MacroTable`'s lookup rule 5), it says so.

mod config;
mod config_concurrency;
mod engine;
mod ipc;
#[cfg(feature = "evkey-compat")]
mod legacy;
mod macros;
mod oracle;
mod reader;
mod transaction;
mod utf16;

/// `engine_sequences`: event sequences through `VietnameseEngine` with a real `MacroTable`,
/// applied to `TextSimulator` the way an adapter (and `raumakey_engine::script`) does: the next
/// state is committed only after its transaction applied; otherwise the engine is reset and the
/// key passes through.
///
/// Input, key-script form: `#!` and option words on the first line (`telex`, `vni`, `viqr`,
/// `user`, `nfc`, `nfd`, `preserve`, `modern`, `nofree`, `nospell`, `norestore`, `nomacros`,
/// `always-macro`, `exact-case`, `disabled`, `limit=<n>`), then a `raumakey_engine::script` key
/// script. Binary form (anything else): options (method, normalization, the flags, the user
/// key map id, a context limit inside or outside `8..=128`), a user-defined key map (sometimes
/// invalid), up to two extra macros, then operations until 128 events were fed: characters of
/// a mixed alphabet (letters, the tone and mark keys of every method, digits, punctuation,
/// Vietnamese letters, combining marks, astral and other non-ASCII characters, controls),
/// backspace, space/enter/tab with and without Shift, Ctrl, or Alt, navigation keys, mouse,
/// focus, mode, configuration, and shutdown resets, Ctrl/Alt/Win shortcuts, AltGr
/// characters, whitespace and multi-scalar text, malformed events, secure-field changes,
/// events tagged `RAUMAKEY_INJECTED_TAG` or a foreign tag, macro triggers typed in any case
/// and ended by any boundary, switches between two documents (with or without a reset), mode
/// toggles, option changes the engine is not told about, and host edits of the document
/// behind the engine (after which a transaction may no longer apply).
///
/// The fixed table has an entry for every lookup rule: adaptive and exact case, a
/// whitespace-only boundary, an NFD trigger, a disabled entry, a duplicate trigger, raw-key
/// triggers, token triggers with boundary punctuation (D28), and the longest valid replacement
/// (D30: `MAX_INSERT_UNITS` units with a delimiter).
///
/// Checks: processing is deterministic; every transaction validates on its own (a well-formed
/// insert within `MAX_INSERT_UNITS`, a replay only of a consumed key) and deletes only text the
/// engine owns (matrix #54); unless the host edited the document behind the engine, it also
/// validates against the document, never splits a surrogate pair, and applies, and after such
/// an edit it may fail only because its deletion no longer fits (beyond the document or inside
/// a surrogate pair), after which the adapter resets; the document stays well-formed,
/// and the owned text stays its suffix until a host edit; shortcuts pass through with a reset
/// (matrix #21), and no macro is looked up for an event in a secure field, whose transformation
/// the adapter's policy decides (D25, matrix #29); `reset_context` always ends the
/// word; only Enter and Tab are replayed; the generation advances with every state change;
/// Raumakey's own injected input changes nothing; a word is never continued in another
/// document (matrix #20); errors are typed and match their cause (malformed event,
/// `ContextMismatch` for another document's word, `UnsupportedInputMethod` for a missing user
/// key map), never `InvalidStateVersion`; in the key-script form the
/// `raumakey_engine::script` runner ends with the same document.
pub fn engine_sequences(data: &[u8]) {
    engine::run(data);
}

/// `malformed_utf16`: arbitrary UTF-16 against the model helpers, event validation, the
/// engine, and every converter charset.
///
/// Input: up to 512 bytes read as little-endian code units (a trailing odd byte is ignored by
/// the unit view and kept for the byte decoders).
///
/// Checks: `raumakey_model::utf16` (`validate`, `is_well_formed`, `decode`, `scalar_count`,
/// `is_scalar_boundary`, `last_scalar_len`, `single_scalar`, `encode`) agrees with `std` on the
/// units, and on them without their first or their last unit; `InputEvent::validate` and
/// `VietnameseEngine::process` accept exactly the events of every kind that break no plan §2.4
/// rule and otherwise return one broken rule as a typed error, and an accepted event's
/// transaction applies, keeps the document well-formed, and keeps the owned text its suffix;
/// for every `Charset`, `decode` and `encode` of the (lossily decoded) text succeed, `decode` is
/// NFC, a lossless encoding decodes back to the NFC text, and `decode_bytes` of the raw bytes
/// fails only with `MalformedInput` for a Unicode charset.
pub fn malformed_utf16(data: &[u8]) {
    utf16::run(data);
}

/// `config_documents`: text decoded as each `raumakey-config` document (`raumakey.toml`,
/// `raumakey-macros.toml`, `raumakey-app-rules.toml`, `raumakey-migration.json`).
///
/// Input: up to 64 KiB of bytes, decoded as UTF-8 with replacement characters.
///
/// Checks: `decode` never panics and fails only with `Corrupt` or `UnsupportedVersion`; the
/// header revision shortcut agrees with a full decode; issue paths hold only schema names and
/// indices; a decoded document encodes, decodes again to the same document with no new
/// issues, and encodes to the same text (JSON floats may move by a few ULPs, and a TOML NaN
/// never equals itself, so those compare as text); a decoded configuration yields valid engine
/// options and key maps that the engine accepts (matrix #45); a decoded macro document builds a
/// table or fails with a typed error; a decoded `raumakey-app-rules.toml` never resolves the
/// clipboard while it is disabled or in a secure field, and secure fields never get macros,
/// nor Vietnamese in `Bypass` mode.
pub fn config_documents(data: &[u8]) {
    config::run(data);
}

/// `config_concurrency`: commit schedules of several `AtomicStore` handles on one
/// `raumakey.toml` (plan §5, design §8, matrix #46, and the matrix pass criteria's "concurrent
/// config update").
///
/// Input: the number of handles (1 to 4), then up to 12 operations: a load; a commit; a race of
/// 2 to 4 commits by any handles (one handle may race itself) on threads released together,
/// all expecting one revision; a hand edit, in which another writer rewrites the file at the
/// same revision or a higher one; a revision poll; or a commit through a new handle that never
/// loaded the file. A commit expects the revision on disk, the one before or after it, 0, or a
/// small number, and writes one of 256 valid configurations.
///
/// Checks: a commit succeeds exactly when it expects the revision on disk and its handle did
/// not last see that revision with other content (design §8: another writer's change is never
/// overwritten silently), and then writes that revision plus one; otherwise it fails with
/// `Conflict { expected, actual }` and leaves the file unchanged. Of racing commits that may
/// succeed, exactly one does, the others conflict, and the file holds the winner's document
/// (matrix #46). Loads return the document on disk without issues, polls its revision, and no
/// temporary file outlives a commit.
///
/// Each call writes real files in a new folder under the system temporary folder and removes
/// it afterwards, so a call is far slower than those of the other targets; it panics when no
/// such folder can be created.
pub fn config_concurrency(data: &[u8]) {
    config_concurrency::run(data);
}

/// `macro_files`: legacy macro text against the importer, `MacroTable`, and the exporter.
///
/// Input: up to 64 KiB of raw file bytes (UTF-8 with or without BOM, UTF-16 LE or BE with BOM,
/// or anything else).
///
/// Checks: `parse_legacy_text` never panics; issues are in line order within the input (or the
/// single whole-input `TooLarge` issue); every entry is valid and NFC (`normalize_entry` is the
/// identity), numbered 1, 2, 3, … with the default settings, and exportable unless its trigger
/// starts with U+FEFF; `MacroTable::new` accepts the entries; lookups of every trigger's case
/// variants and NFD form, and of the input's words, with every `Boundary` and `CaseMode`, give
/// exactly the result of the rules `MacroTable` documents: the plan §4 lookup order (exact
/// trigger first, then case-insensitive in adaptive mode, lowest identifier wins, NFC output)
/// with the table's rule 5, which departs from the plan §4 wording. Rule 5 is a code decision
/// that design §20.9 lists and the plan does not record yet: only a case-insensitive match is
/// case-adapted (a trigger typed exactly as stored never is), and an adaptation that would
/// exceed the replacement limit, or not fit one insert with a delimiter in NFC or NFD (D30),
/// keeps the stored text; every expansion fits one such insert. The token queries of D28
/// agree with the same entries: `has_token_triggers` (an enabled trigger contains boundary
/// punctuation) and `is_trigger_prefix` after each boundary punctuation of those texts (an
/// enabled trigger starts with the NFC prefix, or, in adaptive mode, its fold with the
/// prefix's fold). Export
/// reports exactly the entries it cannot hold, and re-importing the export yields the written
/// entries unchanged and exports to the same text.
pub fn macro_files(data: &[u8]) {
    macros::run(data);
}

/// `ipc_frames`: byte streams against the control-channel `FrameDecoder`.
///
/// Input: up to 64 KiB. A first byte with bit 7 set selects the assemble form, in which the
/// target writes headers with valid checksums around payloads taken from the input (so that a
/// byte-level fuzzer reaches the body decoder): per frame a raw kind (u16 LE), a client-id
/// choice, a request id, a corruption code (12 bad checksum, 13 version, 14 flags, 15 reserved),
/// a length mode (6 declares one byte more, 7 an oversize length), a payload length (u16 LE),
/// and the payload; its low six bits choose the chunking. Any other input is the stream itself.
///
/// Checks, for decoders of every `Sender`: feeding the stream whole or in chunks of 1 to 4096
/// bytes yields the same frames, error, and idle state; payload memory never exceeds the
/// length the frame in progress declares, its kind's limit, or the sender's limit, and none is
/// held between frames or after a failure; the first error is terminal and repeats; without an
/// error the decoder is idle exactly at a frame boundary; every decoded frame is allowed for its
/// sender, validates, re-encodes, and decodes back to the same frame.
pub fn ipc_frames(data: &[u8]) {
    ipc::run(data);
}

/// `legacy_sequence_wrap` (feature `evkey-compat`): the EVKey legacy codec over its
/// deterministic fake.
///
/// Input: whether a real `LegacyServer` (on its own endpoint) or scripted replies answer, the
/// starting request sequence (near the `u32` wrap and the sign boundary, or raw), the reset
/// gate, the three unknown flags, the server's configuration and handler replies, then up to
/// 64 operations: connect, characters (lone surrogates included), backspace, reset, disconnect,
/// delete limits, clock advances, unsolicited and stale responses (odd and oversized lengths,
/// stale and future epochs, foreign sequences, ill-formed output), a mutex held elsewhere or
/// abandoned, sequence jumps, gate changes, raw requests to the server, lost objects, and
/// shutdown.
///
/// Checks: nothing panics; every shared-memory access stays inside the `0x1038`-byte block;
/// the client writes only the request fields, releases the mutex on every path, and advances
/// the request sequence as a raw `u32` that skips 0; an accepted edit is valid and is exactly
/// what `validate_response` makes of the block for that request (so malformed, stale, future,
/// or unmatched responses are rejected); a failure passes the key through with a reset; errors
/// that require a re-sync, and three failed exchanges in a row, drop the bridge, while a single
/// timeout does not; the unknown flags are preserved; the server rejects requests that do not
/// decode and never hands one request sequence to its handler twice in a row.
#[cfg(feature = "evkey-compat")]
pub fn legacy_sequence_wrap(data: &[u8]) {
    legacy::run(data);
}

/// `transaction_application`: arbitrary documents and transactions against
/// `EditTransaction::validate` and `TextSimulator::apply`.
///
/// Input: a document of up to 48 code points (lone surrogates and astral pairs included), a
/// transaction (delete count around the document length or at the extremes, an insert of up
/// to 8 code points or exactly at or one past `MAX_INSERT_UNITS`, consume and reset flags, an
/// optional replay key), and an event of any kind with arbitrary text, virtual key, and
/// modifiers.
///
/// Checks: `validate` accepts exactly the transactions that break none of the plan §2.7 rules
/// and otherwise reports a rule that is broken; `apply` never panics; on success the text is
/// what plan §2.7 prescribes (the deletion never splits a surrogate pair, and well-formed
/// input stays well-formed); on error the text is unchanged. Two replay cases follow
/// `TextSimulator`'s own rules, which plan §2.7 does not state, so for them the check is a
/// regression check rather than an independent oracle: a replay of the event's own key has the
/// event's effect (plan §2.7 would type the key's text, `\r` for Enter, whatever the event's
/// text), and a replayed Backspace deletes the last code point.
pub fn transaction_application(data: &[u8]) {
    transaction::run(data);
}
