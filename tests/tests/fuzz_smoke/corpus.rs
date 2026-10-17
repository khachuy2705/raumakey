//! Hand-written edge cases, run before the generated inputs of each target.
//!
//! The input formats are documented in `raumakey_tests::fuzz_targets`. Every byte string is a
//! valid input for every target, so an entry that no longer means what its comment says still
//! runs; it only loses its focus.

use raumakey_ipc::frame::{Frame, HEADER_LEN, MAX_CONTROL_PAYLOAD, MAX_PAYLOAD, encode_frame};
use raumakey_ipc::message::{Arch, ClientKind, Command, MacroSnapshot, Message, RuntimeSnapshot};
use raumakey_model::diag::{Component, DiagCode, DiagEvent};
use raumakey_model::host::ExeName;
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::policy::ProbeRecord;

fn texts(items: &[&str]) -> Vec<Vec<u8>> {
    items.iter().map(|item| item.as_bytes().to_vec()).collect()
}

fn utf16_le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn units_le(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|unit| unit.to_le_bytes()).collect()
}

/// Key scripts (`#!` form) for the matrix rows the engine covers, plus raw byte patterns.
pub fn engine_sequences() -> Vec<Vec<u8>> {
    let history = format!(
        "#!telex nospell limit=128\n{}{}",
        "aw".repeat(20),
        "{BS}".repeat(40)
    );
    let mut corpus = texts(&[
        // A1–A3, golden vectors (plan §3.10).
        "#!telex\naa dd as aw ow uw w ww tw nhw ass aaa ddd ",
        "#!telex\ntieengs vieetj nguwowif nguoiwf thuowr huow truowngf ruowuj khuyeens ",
        "#!telex\nquas quys quyeets gias gif giuwax giuwowngf ddi did Ddaay VIEETJ ",
        "#!telex\nhoaf thuyr khoer hoafn chaof cuar muoons nghieeng awa tesst ",
        "#!telex modern\nhoaf thuyr khoer hoafn ",
        // A15/A16: restore of invalid transformed words.
        "#!telex\nafter windows class{SPACE}",
        "#!telex norestore\nafter{SPACE}",
        "#!telex nospell nofree\nafter tieengs ddi{SPACE}",
        // A4–A8: VNI and VIQR, including the VIQR escape.
        "#!vni\ntie6ng1 vie65t nguo72i a66 d9 ",
        "#!viqr\ntie^'ng vie^.t ngu+o+`i \\' dd ko\\.{ENTER}",
        // A17/A18: semantic backspace and the history bound.
        "#!telex\naa{BS}{BS}{BS}tieengs{BS}{BS}{BS}{BS}",
        history.as_str(),
        "#!telex limit=8\nabcdefghijklmnopqrstuvwxyz,{BS}{BS}",
        "#!telex limit=0\nnguwowif{SPACE}",
        // B21–B23, B29: shortcuts, AltGr, case, secure fields.
        "#!telex\na{CTRL+c}a{ALTGR+@}{ALTGR+a}{ALTGR+a}{CTRL+SPACE}{CTRL+ENTER}{CTRL+TAB}",
        "#!telex\nVIEETJ Vieetj vIEETJ DDAAY DDaay ",
        "#!telex\n{SECURE}aas ko{SPACE}{BS}{/SECURE}aa{SECURE}{ENTER}",
        "#!telex\ntieeng{NAV}s{MOUSE}s{FOCUS}s",
        // D47–D54: macros of the fixed table, NFD, supplementary scalars.
        "#!telex\nko{SPACE}Ko{SPACE}KO{SPACE}ko,ko{ENTER}ko{TAB}ko{SHIFT+SPACE}",
        "#!telex exact-case\nKO{SPACE}vn{SPACE}VN{SPACE}dc,dc{SPACE}",
        "#!telex disabled always-macro\naas ko{SPACE}dc,dc{SPACE}vn.",
        "#!telex disabled\naas ko{SPACE}",
        "#!telex nfd\ntieengs{SPACE}ko{SPACE}a\u{302}{SPACE}vieetj{SPACE}",
        "#!telex preserve\no\u{302}{SPACE}tieengs a\u{302}{SPACE}",
        "#!telex\nbig{ENTER}big{SPACE}big{TAB}nl{SPACE}after{SPACE}dd{SPACE}UBND{SPACE}off{SPACE}",
        "#!telex nfd\nbig{ENTER}tieengs{ENTER}",
        "#!telex\naa{BS}{BS}{BS}😀aa{BS}{BS}{BS}{BS}a😀aa{SPACE}",
        "#!telex nospell\na😀aa\u{301}\u{302}ê{BS}",
        // User key map, braces, controls, and odd characters.
        "#!user\naa;s.6/'7[{SPACE}",
        "#!user nfd modern\nhoa.s{SPACE}",
        "#!telex\n{LBRACE}{RBRACE}{CTRL+LBRACE}{ALTGR+RBRACE}",
        "#!telex\n\u{0}\u{8}\u{1b}\u{7f}x\u{85}\u{2028}\u{feff}İß",
        "#!vni nofree\ntoan1 d9i9 ",
        "#!telex\n{UNKNOWN}{CTRL+}unterminated{",
        "#!",
        "#!telex",
    ]);
    corpus.push(Vec::new());
    corpus.push(vec![0; 64]);
    corpus.push(vec![0xFF; 256]);
    corpus.push((0..=255).collect());
    corpus.push((0..=255).rev().collect());
    corpus
}

/// UTF-16 edge cases as little-endian code units, and raw bytes for the byte decoders.
pub fn malformed_utf16() -> Vec<Vec<u8>> {
    let mut corpus = vec![
        Vec::new(),
        vec![0x61],
        units_le(&[0xD800]),
        units_le(&[0xDC00]),
        units_le(&[0xDBFF, 0xDFFF]),
        units_le(&[0xDC00, 0xD800]),
        units_le(&[0xD800, 0xD800, 0xDC00]),
        units_le(&[0x61, 0xD800, 0x62]),
        units_le(&[0xD83D, 0xDE00, 0xD83D]),
        units_le(&[0xFEFF, 0x61]),
        units_le(&[0xFFFE, 0xFFFF, 0x0000]),
        utf16_le("Tiếng Việt có dấu"),
        utf16_le("Tie^'ng Vie^.t \\. dd DD"),
        utf16_le("&#7871;&#x1EBF;&#xD800;&#99999999;&#"),
        utf16_le("TiÕng ViÖt"),
        utf16_le("Tiáº¿ng Viá»‡t"),
        utf16_le("aaaaaaaaaaaaaaaaa"),
        utf16_le("a\u{302}\u{301}e\u{323}\u{302}"),
        utf16_le("😀😀😀😀😀😀😀😀😀"),
        vec![0xEF, 0xBB, 0xBF, 0x61],
        vec![0xFF, 0xFE, 0x61, 0x00, 0x00],
        vec![0xFE, 0xFF, 0x00, 0x61],
        vec![0xC3, 0x28, 0xA0, 0xA1],
    ];
    let mut long = utf16_le(&"ệ".repeat(200));
    long.extend(units_le(&[0xD800]));
    corpus.push(long);
    corpus
}

const CONFIG_EXAMPLE: &str = r#"version = 1
revision = 1
[input]
enabled = true
method = "telex"
user_keymap = "my-map"
normalization = "nfc"
modern_tone = false
free_marking = true
spell_check = true
auto_restore_invalid = true
context_limit = 32
[macros]
enabled = true
when_disabled = false
auto_case = true
[hotkeys]
toggle_mode = ["Ctrl+Shift"]
hard_reset = ["Ctrl+Shift+Alt+F12", "Ctrl+Shift+Alt+Space"]
[system]
startup = "none"
show_panel_on_start = true
diagnostics = false
[conversion]
source = "unicode"
target = "unicode"
case = "none"
remove_tones = false
[[keymaps]]
id = "my-map"
version = 1
name = "My map"
bindings = [ { key = "s", actions = ["tone:acute"] } ]
"#;

const APP_RULES_EXAMPLE: &str = r#"version = 1
revision = 1
clipboard_enabled = false
default_reset_on_mouse = true
secure_field_mode = "bypass"
default_max_delete_before = 64
[[rule]]
exe = "notepad.exe"
window_class = "Notepad"
transport = "synthetic-key"
reset_on_mouse = true
macros = true
vietnamese = false
max_delete_before = 64
[[named_set]]
name = "chromium-family"
members = ["msedge.exe", "chrome.exe"]
action = { reset_on_mouse = true }
[[probe]]
exe = "legacy.exe"
probe_version = 1
tsf_range_edit = false
"#;

const MACROS_EXAMPLE: &str = r#"version = 1
revision = 3
[[macro]]
id = 1
trigger = "ko"
replacement = "không"
[[macro]]
id = 2
trigger = "vn"
replacement = "Việt Nam"
enabled = false
boundary = "whitespace"
case = "exact"
x_note = { kept = [1, 2.5, "x"] }
[[macro]]
trigger = "set aside: no id"
"#;

const MIGRATION_EXAMPLE: &str = r#"{
  "version": 1,
  "revision": 2,
  "status": "completed",
  "started_at_ms": 1,
  "completed_at_ms": 2,
  "sources": [
    { "kind": "macro-text", "location": "evkmacro.txt", "sha256": "00", "size": 3, "x": 0.1 }
  ],
  "imported": { "settings": 0, "macros": 2, "named_sets": 1, "probes": 0, "x_new": [] },
  "quarantined": [ { "source": "setting.ini", "field": "line 3", "reason_code": "unmapped" } ],
  "preserved": { "ini": { "a": "b" }, "f": 1e300, "g": -0.0, "h": 5e-324 },
  "result": {
    "config_revision": 1, "macros_revision": 1, "app_rules_revision": 1,
    "checksums": { "config.toml": "ab", "bad": 1 }
  },
  "x_top": null
}
"#;

/// The plan §5.1 examples and documents that probe each decoding rule.
pub fn config_documents() -> Vec<Vec<u8>> {
    let mut corpus = texts(&[
        CONFIG_EXAMPLE,
        APP_RULES_EXAMPLE,
        MACROS_EXAMPLE,
        MIGRATION_EXAMPLE,
        "",
        "\u{feff}version = 1\nrevision = 1\n",
        "version = 1\nrevision = 9223372036854775806\n",
        "version = 1\nrevision = 9223372036854775807\n",
        "version = 1\nrevision = -1\n",
        "version = 2\nrevision = 1\n",
        "version = \"1\"\n",
        "version = 1.0\n",
        "revision = 5\n",
        "version = 1\r\nrevision = 1\r\n[input]\r\nmethod = \"vni\"\r\n",
        "version = 1\nrevision = 1\nx_nan = nan\nx_inf = -inf\nx_date = 1979-05-27T07:32:00Z\n",
        "version = 1\n[input]\nmethod = \"user\"\nuser_keymap = \"absent\"\ncontext_limit = 4096\n",
        "version = 1\n[[keymaps]]\nid = \"m\"\nversion = 1\nname = \"m\"\nbindings = [{ key = \"ss\", actions = [] }, { key = \"s\", actions = [\"tone:acute\", \"bogus\"] }]\n[[keymaps]]\nid = \"m\"\n",
        "version = 1\n[hotkeys]\ntoggle_mode = [\"Shift\", \"Ctrl+Shift\", 3]\nhard_reset = \"Ctrl+Alt+F12\"\n",
        "version = 1\n[[rule]]\nexe = \"C:\\\\bad\\\\path.exe\"\n[[rule]]\nexe = \"Ok.EXE\"\nwindow_class = 7\n[[named_set]]\nmembers = [\"a.exe\"]\n[[probe]]\nexe = \"p.exe\"\n",
        "version = 1\n[[macro]]\nid = -1\ntrigger = \"x\"\nreplacement = \"y\"\n[[macro]]\nid = 1\ntrigger = \"x\"\nreplacement = \"y\"\nboundary = \"sometimes\"\n[[macro]]\nid = 1\ntrigger = \"x\"\nreplacement = \"z\"\n",
        "version = 1\na = [[[[[[[[[[1]]]]]]]]]]\nb = { c = { d = { e = {} } } }\n",
        "[input\nmethod = ",
        "{\"version\": 1, \"revision\": 18446744073709551614}",
        "{\"version\": 1, \"revision\": 18446744073709551615, \"status\": \"rolled-back\"}",
        "{\"version\": 1, \"result\": null, \"sources\": {}, \"imported\": []}",
        "{\"version\": 1, \"sources\": [{\"kind\": \"k\"}, 7, {\"kind\": \"k\", \"location\": \"l\", \"y\": 1}]}",
        "[]",
        "null",
        "{\"version\": 1, \"a\": 1, \"a\": 2}",
    ]);
    corpus.push(vec![0xFF, 0xFE, 0x76, 0x00]);
    corpus.push(b"version = 1\n\xC3\x28 = 1\n".to_vec());
    corpus
}

/// Commit schedules in the binary form of `fuzz_targets::config_concurrency`. Each byte is one
/// decision of the decoder, in order; the comments name them.
pub fn config_concurrency() -> Vec<Vec<u8>> {
    let four_racers_at_zero = vec![
        3, // four handles
        3, 2, // a race of four commits
        0, 1, 2, 3, // by handles 0, 1, 2, and 3
        0, // all expecting the current revision, 0
        1, 2, 3, 4, // four documents
    ];
    let a_handle_racing_itself = vec![
        1, // two handles
        3, 0, 0, 0, 0, 5, 6, // a race of two commits by handle 0, expecting 0
        3, 0, 1, 1, 0, 7, 8, // the same for handle 1, expecting 1
    ];
    let a_hand_edit_is_never_overwritten = vec![
        1, // two handles
        1, 0, 0, 7, // handle 0 commits revision 1
        0, 1, // handle 1 loads it
        5, 0, 9, // another writer rewrites the file and keeps revision 1
        1, 1, 0, 10, // handle 1 commits on revision 1: conflict
        1, 0, 0, 11, // handle 0 wrote revision 1 itself: conflict too
        7, 0, 12, // a handle that never loaded relies on the revision alone: revision 2
        6, 1, // handle 1 polls revision 2
        1, 1, 0, 13, // handle 1 never saw revision 2, so it commits revision 3
    ];
    let stale_and_future_revisions = vec![
        0, // one handle
        1, 0, 4, 1, // expects u64::MAX (0 - 1): conflict
        1, 0, 5, 2, // expects 1 (0 + 1): conflict
        1, 0, 6, 3, // expects 0: revision 1
        1, 0, 7, 9, // expects 9: conflict
        5, 3, 4, // another writer jumps to revision 6
        1, 0, 0, 5, // expects 6: revision 7
    ];
    let nobody_may_win = vec![
        2, // three handles
        1, 0, 0, 1, // handle 0 commits revision 1
        3, 1, 0, 1, 2, 4, 2, 3, 4, // three racers expect 0: every one conflicts
    ];
    vec![
        Vec::new(),
        // Four handles; twelve commits through new handles expecting revision 255: conflicts.
        vec![0xFF; 64],
        four_racers_at_zero,
        a_handle_racing_itself,
        a_hand_edit_is_never_overwritten,
        stale_and_future_revisions,
        nobody_may_win,
    ]
}

/// Legacy macro files in every supported encoding, with each kind of bad line.
pub fn macro_files() -> Vec<Vec<u8>> {
    let lines = "; header\r\n# comment\r\n// comment\r\nDO NOT DELETE this line\r\n\r\n\
                 ko:không\r\nko : không \r\nvn:Việt Nam\r\ntab:\t\r\nnbsp:\u{a0}\r\n\
                 url:http://example.com\r\nbroken\r\n:empty trigger\r\nempty replacement:\r\n\
                 k o:space\r\nnul:a\u{0}b\r\nnfd:vie\u{302}\u{323}t\r\nvie\u{302}\u{323}t:nfd trigger\r\n\
                 \u{feff}bom:second line\r\nko:duplicate\r\nKO:upper\r\n";
    let mut corpus = texts(&[
        lines,
        "",
        "\n\r\n\r",
        "ko:không",
        "ko:không\rvn:Việt Nam\ndc:được",
        "\u{feff}\u{feff}ko:không\n",
        "DO NOT DELETE:x\n",
        "a:DO NOT DELETE\n",
    ]);
    corpus.push(b"\xEF\xBB\xBFko:kh\xC3\xB4ng\r\n".to_vec());
    corpus.push(b"ko:\xC3\x28\nvn:ok\n".to_vec());
    let mut le = vec![0xFF, 0xFE];
    le.extend(utf16_le("ko:không\r\nvn:Việt Nam\r\n"));
    corpus.push(le.clone());
    le.push(0x41);
    corpus.push(le);
    let mut be = vec![0xFE, 0xFF];
    be.extend("ko:không\nx:😀\n".encode_utf16().flat_map(u16::to_be_bytes));
    corpus.push(be);
    let mut lone = vec![0xFF, 0xFE];
    lone.extend(units_le(&[0x61, 0x3A, 0xD800, 0x0A, 0x62, 0x3A, 0x63]));
    corpus.push(lone);
    corpus.push(format!("{}:x\n{}:y\n", "t".repeat(64), "u".repeat(65)).into_bytes());
    corpus.push(format!("a:{}\nb:{}\n", "r".repeat(4096), "s".repeat(4097)).into_bytes());
    corpus
}

fn frame_bytes(message: Message, client_id: u64, request_id: u64) -> Vec<u8> {
    encode_frame(&Frame::new(client_id, request_id, message)).unwrap_or_default()
}

/// A frame with a valid checksum around `payload` that declares its true length.
fn raw_frame(kind: u16, payload: &[u8]) -> Vec<u8> {
    raw_frame_declaring(
        kind,
        u32::try_from(payload.len()).unwrap_or(u32::MAX),
        payload,
    )
}

/// A header with a valid checksum around `payload`, declaring `declared` payload bytes.
fn raw_frame_declaring(kind: u16, declared: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
    bytes.extend_from_slice(b"RMK1");
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&kind.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&40_u16.to_le_bytes());
    bytes.extend_from_slice(&declared.to_le_bytes());
    bytes.extend_from_slice(&5_u64.to_le_bytes());
    bytes.extend_from_slice(&6_u64.to_le_bytes());
    let mut crc = crc32fast::Hasher::new();
    crc.update(&bytes);
    crc.update(payload);
    bytes.extend_from_slice(&crc.finalize().to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

/// Frames in the assemble form of `fuzz_targets::ipc_frames` (the target writes the headers
/// and checksums): kind, client-id choice 1, request id, no corruption, true length, payload.
fn assembled(frames: &[(u16, &[u8])]) -> Vec<u8> {
    let mut bytes = vec![0x80];
    for (kind, payload) in frames {
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(&[1, 9, 0, 0]);
        let len = u16::try_from(payload.len()).unwrap_or(u16::MAX);
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.extend_from_slice(payload);
    }
    bytes
}

/// Valid frames of every kind, then each header and body rule broken once.
pub fn ipc_frames() -> Vec<Vec<u8>> {
    let probe = ProbeRecord {
        exe: ExeName::new("legacy.exe").expect("constant name is valid"),
        probe_version: 1,
        tsf_range_edit: false,
    };
    let entry = MacroEntry {
        id: MacroId(1),
        trigger: "ko".to_owned(),
        replacement: "không".to_owned(),
        enabled: true,
        boundary: BoundaryPolicy::Delimiter,
        case_policy: CasePolicy::Adapt,
    };
    let messages = vec![
        Message::Hello {
            client_kind: ClientKind::Tip,
            pid: 42,
            arch: Arch::X64,
            protocol: 1,
        },
        Message::Welcome {
            client_id: 7,
            server_revision: 3,
        },
        Message::RuntimeSnapshot(RuntimeSnapshot::defaults_v1()),
        Message::MacroSnapshot(MacroSnapshot {
            macro_revision: 9,
            entries: vec![entry],
        }),
        Message::Command {
            command: Command::ReportProbe(probe),
        },
        Message::Command {
            command: Command::SetMode(false),
        },
        Message::ModeChanged {
            vietnamese_mode: true,
            revision: 4,
        },
        Message::ResetAll,
        Message::Diagnostic(DiagEvent {
            timestamp_ms: 1,
            component: Component::Tsf,
            code: DiagCode(7),
            revision: Some(2),
            host_exe: None,
            vector_id: None,
        }),
        Message::Ping,
        Message::Pong,
        Message::Goodbye,
    ];
    let mut stream = Vec::new();
    let mut corpus = Vec::new();
    for (index, message) in messages.into_iter().enumerate() {
        let request_id = u64::try_from(index).unwrap_or(0);
        let frame = frame_bytes(message, 7, request_id);
        stream.extend_from_slice(&frame);
        corpus.push(frame);
    }
    corpus.push(stream.clone());
    corpus.push(stream.get(..stream.len() / 2).unwrap_or_default().to_vec());
    let ping: &[u8] = br#"{"type":"ping"}"#;
    let welcome_zero: &[u8] = br#"{"type":"welcome","client_id":0,"server_revision":1}"#;
    let inconsistent: &[u8] = br#"{"type":"command_result","ok":true,"code":3}"#;
    let upper_exe: &[u8] = br#"{"type":"command","command":{"report_probe":{"exe":"BAD.EXE","probe_version":1,"tsf_range_edit":true}}}"#;
    let unknown_field: &[u8] = br#"{"type":"ping","extra":[1,2]}"#;
    let partial_snapshot: &[u8] = br#"{"type":"macro_snapshot","macro_revision":1}"#;
    let mut bad_magic = raw_frame(10, ping);
    bad_magic[0] = b'X';
    corpus.push(bad_magic);
    let mut bad_version = raw_frame(10, ping);
    bad_version[4] = 2;
    corpus.push(bad_version);
    let mut bad_header_len = raw_frame(10, ping);
    bad_header_len[10] = 41;
    corpus.push(bad_header_len);
    let mut flags = raw_frame(10, ping);
    flags[8] = 1;
    corpus.push(flags);
    let mut reserved = raw_frame(10, ping);
    reserved[39] = 1;
    corpus.push(reserved);
    let mut bad_crc = raw_frame(10, ping);
    bad_crc[32] ^= 1;
    corpus.push(bad_crc);
    corpus.push(raw_frame(0, ping));
    corpus.push(raw_frame(14, ping));
    corpus.push(raw_frame_declaring(10, MAX_CONTROL_PAYLOAD + 1, ping));
    corpus.push(raw_frame_declaring(3, MAX_PAYLOAD + 1, ping));
    corpus.push(raw_frame_declaring(3, MAX_PAYLOAD, b"{"));
    corpus.push(raw_frame(10, b""));
    corpus.push(raw_frame(11, ping));
    corpus.push(raw_frame(2, welcome_zero));
    corpus.push(raw_frame(6, inconsistent));
    corpus.push(raw_frame(5, upper_exe));
    corpus.push(raw_frame(10, unknown_field));
    corpus.push(raw_frame(3, ping));
    corpus.push(raw_frame(4, partial_snapshot));
    let mut header_only = raw_frame(10, ping);
    header_only.truncate(HEADER_LEN);
    corpus.push(header_only);
    corpus.push(b"RMK".to_vec());
    corpus.push(Vec::new());
    corpus.push(assembled(&[
        (10, ping),
        (2, welcome_zero),
        (6, inconsistent),
        (5, upper_exe),
    ]));
    corpus.push(assembled(&[
        (10, unknown_field),
        (4, partial_snapshot),
        (11, b""),
    ]));
    corpus.push(vec![0x80]);
    corpus
}

/// Legacy scenarios in the binary form of `fuzz_targets::legacy_sequence_wrap`. Each byte is
/// one decision of the decoder, in order; the comments name them.
#[cfg(feature = "evkey-compat")]
pub fn legacy_sequence_wrap() -> Vec<Vec<u8>> {
    let header_scripted_at_wrap = [
        0, // scripted controller replies
        0, // request sequence u32::MAX
        0, // reset gate 1
        0, 0, 0, // unknown flags 0, 0, 0
    ];
    let connect = [0, 0, 0, 0, 0]; // connect; reply pushed: delete 0, no output, benign
    let char_a = [3, 0, 1, 1, 0, 0, 0, 0, 0]; // 'a'; reply: delete 1, output "a"
    let backspace = [9, 0, 0, 0, 0]; // backspace; reply: empty edit
    let jump = [17, 1]; // another TIP moved the sequence to u32::MAX - 1
    let wrap_scenario = [
        &header_scripted_at_wrap[..],
        &connect,
        &char_a,
        &backspace,
        &jump,
        &char_a,
        &char_a,
    ]
    .concat();
    let server_scenario = [
        &[
            1, // a real LegacyServer answers
            1, // request sequence u32::MAX - 1
            0, // gate value (the server publishes its own)
            0, 0, 0, // unknown flags
            0, // server reset gate on
            0, // server delete limit 0x800
            0, // one handler edit:
            0, 1, 1, 2, // small: delete 1, insert "ế"
        ][..],
        &[0],                 // connect (op 5 sync)
        &[3, 0, 0, 0],        // 'a'
        &[10],                // reset: the gate is on, so op 3 goes to the server
        &[14, 4, 0, 0, 0, 0], // a raw request with opcode 0: rejected
        &[3, 3, 0, 0],        // a lone surrogate: refused without a request
        &[19, 0, 1],          // the server shuts down
        &[3, 0, 0, 0],        // 'a': no response any more
    ]
    .concat();
    vec![
        Vec::new(),
        vec![0; 32],
        vec![0xFF; 64],
        wrap_scenario,
        server_scenario,
        (0..=255).collect(),
        (0..=255).rev().collect(),
    ]
}

/// Documents with lone surrogates and transactions at the edges of the validation rules, in
/// the binary form of `fuzz_targets::transaction_application` (one byte per decision).
pub fn transaction_application() -> Vec<Vec<u8>> {
    // Document "😀" (one astral item), delete 1: the cut splits the pair and is rejected.
    let splits_pair = vec![1, 0xFF, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0];
    // Document [lone high surrogate, 'a'], insert U+0301, a replayed Enter without consuming
    // the key (rejected), and an event whose text is a lone low surrogate.
    let replay_without_consume = vec![
        2, 0, 3, 0, 0, 0, 0x41, 6, 1, 0, 5, 2, 0, 0, 3, 0, 0, 0xFF, 1, 0, 4, 0, 1, 0, 0, 0, 0,
    ];
    // An empty document and an insert of exactly MAX_INSERT_UNITS units, then one more.
    let at_limit = vec![0, 6, 0, 0xFF, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0];
    let over_limit = vec![0, 6, 0, 0xFF, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0];
    vec![
        Vec::new(),
        vec![0; 16],
        vec![0xFF; 64],
        (0..=255).collect(),
        (0..=255).rev().collect(),
        splits_pair,
        replay_without_consume,
        at_limit,
        over_limit,
    ]
}
