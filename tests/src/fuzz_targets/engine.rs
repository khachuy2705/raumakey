//! `engine_sequences`: input event sequences through `VietnameseEngine` with a real
//! `MacroTable`, applied to `TextSimulator` the way an adapter does (and the way
//! `raumakey_engine::script` does): the next state is committed only after the transaction
//! applied, and a failure resets the engine (plan §3, design §5.2, matrix A–B, #47–#54).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use raumakey_engine::script::{self, KeyToken};
use raumakey_engine::{EngineResult, EngineState, STATE_VERSION, VietnameseEngine};
use raumakey_macro::{MacroTable, normalize_entry};
use raumakey_model::error::{EngineError, ResetReason};
use raumakey_model::host::{DocumentId, ExeName, HostContext, SessionId};
use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::{InputEvent, InputKind, vk};
use raumakey_model::keymap::{
    BaseVowel, KEYMAP_SCHEMA_VERSION, KeyAction, KeyBinding, KeyMap, Tone,
};
use raumakey_model::macros::{
    Boundary, BoundaryPolicy, CaseMode, CasePolicy, MacroEntry, MacroExpansion, MacroId,
    MacroLookup,
};
use raumakey_model::options::{EngineOptions, InputMethod, Normalization};
use raumakey_model::transaction::{EditTransaction, TextSimulator, TransactionError};

use super::oracle::{splits_pair, well_formed};
use super::reader::Reader;

/// Most events fed to the engine for one input.
const MAX_EVENTS: usize = 128;
/// Prefix of the key-script form.
const SCRIPT_PREFIX: &[u8] = b"#!";
/// Id of the user-defined key map.
const USER_MAP_ID: &str = "fuzz-map";
/// The EVKey replay sentinel: a foreign tag the Raumakey engine must not treat as its own.
const FOREIGN_TAG: u64 = 0xEBCA_1234;

const ACTIONS: [KeyAction; 15] = [
    KeyAction::Tone(Tone::Acute),
    KeyAction::Tone(Tone::Grave),
    KeyAction::Tone(Tone::Hook),
    KeyAction::Tone(Tone::Tilde),
    KeyAction::Tone(Tone::Dot),
    KeyAction::RemoveTone,
    KeyAction::Circumflex,
    KeyAction::CircumflexOn(BaseVowel::A),
    KeyAction::CircumflexOn(BaseVowel::E),
    KeyAction::CircumflexOn(BaseVowel::O),
    KeyAction::Breve,
    KeyAction::Horn,
    KeyAction::HornOrBreve,
    KeyAction::Stroke,
    KeyAction::Escape,
];

/// Keys a user map may bind: the tone and mark keys of every built-in method and more.
const KEYMAP_KEYS: &[u8] = b"sfrxjzaeowdSAW1234567890'`?~.-^(+*\\;/[]!=";
/// Letters, weighted towards vowels and the Telex keys.
const LETTERS: &[u8] = b"aaaeeeoooiuyndghtcklmnpqrsvxbfjwzADEOUWG";
/// The tone and mark keys of Telex, VNI, and VIQR.
const METHOD_KEYS: &[u8] = b"sfrxjzwdaeo0123456789'`?~.-^(+*\\";
/// Words as typed in Telex (also used for user maps), including golden vectors, undo cases,
/// and words that auto-restore (plan §3.10).
const TELEX_WORDS: &[&str] = &[
    "tieengs",
    "vieetj",
    "nguwowif",
    "nguoiwf",
    "thuowr",
    "huow",
    "truowngf",
    "ruowuj",
    "khuyeens",
    "quas",
    "quys",
    "quyeets",
    "gias",
    "gif",
    "giuwax",
    "giuwowngf",
    "ddi",
    "did",
    "Ddaay",
    "VIEETJ",
    "hoaf",
    "thuyr",
    "khoer",
    "hoafn",
    "chaof",
    "cuar",
    "muoons",
    "nghieeng",
    "awa",
    "tesst",
    "after",
    "windows",
    "class",
    "ass",
    "aaa",
    "ddd",
    "dduowcj",
    "nawng",
    "owf",
    "uw",
    "w",
    "ww",
    "tw",
    "nhw",
    "toans",
    "xuaan",
    "Nguyeenx",
];
/// Words as typed in VNI.
const VNI_WORDS: &[&str] = &[
    "tie6ng1", "vie65t", "nguo72i", "a66", "d9", "ho2a", "thuy3", "khoe3", "d9u7o7c5", "qua1",
    "gia1", "to6i", "Vie65t",
];
/// Words as typed in VIQR.
const VIQR_WORDS: &[&str] = &[
    "tie^'ng", "vie^.t", "ngu+o+`i", "\\'", "dd", "ho`a", "qua'", "ddu+o+.c", "a^^", "a\\.",
];

fn entry(
    id: u64,
    trigger: &str,
    replacement: String,
    boundary: BoundaryPolicy,
    case_policy: CasePolicy,
) -> MacroEntry {
    MacroEntry {
        id: MacroId(id),
        trigger: trigger.to_owned(),
        replacement,
        enabled: true,
        boundary,
        case_policy,
    }
}

/// The fixed macro table: one entry per lookup rule, including an NFD trigger, a disabled
/// entry, a duplicate trigger, token triggers with boundary punctuation (D28), and the
/// longest valid replacement (D30: `MAX_INSERT_UNITS` units with a delimiter).
fn base_table() -> &'static (Vec<MacroEntry>, Arc<MacroTable>) {
    static BASE: OnceLock<(Vec<MacroEntry>, Arc<MacroTable>)> = OnceLock::new();
    BASE.get_or_init(|| {
        use BoundaryPolicy::{Delimiter, Whitespace};
        use CasePolicy::{Adapt, Exact};
        let mut disabled = entry(11, "off", "tắt".to_owned(), Delimiter, Adapt);
        disabled.enabled = false;
        let entries = vec![
            entry(1, "ko", "không".to_owned(), Delimiter, Adapt),
            entry(2, "vn", "Việt Nam".to_owned(), Delimiter, Exact),
            entry(3, "dc", "được".to_owned(), Whitespace, Adapt),
            entry(4, "a1", "😀".to_owned(), Delimiter, Adapt),
            entry(5, "â", "âm".to_owned(), Delimiter, Adapt),
            entry(6, "tiếng", "tiếng Việt".to_owned(), Delimiter, Adapt),
            entry(7, "UBND", "Ủy ban nhân dân".to_owned(), Delimiter, Adapt),
            entry(8, "dd", "đi đâu".to_owned(), Delimiter, Adapt),
            entry(9, "after", "sau đó".to_owned(), Delimiter, Adapt),
            entry(10, "nl", "dòng 1\r\ndòng 2".to_owned(), Delimiter, Adapt),
            disabled,
            entry(
                12,
                "big",
                format!("{}a", "😀".repeat(4095)),
                Delimiter,
                Adapt,
            ),
            entry(
                13,
                "vie\u{302}\u{323}t",
                "Việt".to_owned(),
                Delimiter,
                Adapt,
            ),
            entry(14, "ko", "khong".to_owned(), Delimiter, Adapt),
            // Token triggers (D28): punctuation inside, at the start, and a raw-key form.
            entry(15, "v.v", "vân vân".to_owned(), Delimiter, Adapt),
            entry(16, "...", "…".to_owned(), Whitespace, Adapt),
            entry(17, "aa.ko", "â.không".to_owned(), Delimiter, Exact),
        ];
        let table = MacroTable::new(entries.clone()).expect("the fixed macro table is valid");
        (entries, Arc::new(table))
    })
}

/// The key map of the key-script form: multi-action candidates and bound punctuation.
fn script_user_map() -> KeyMap {
    let binding = |key: char, actions: &[KeyAction]| KeyBinding {
        key,
        actions: actions.to_vec(),
    };
    KeyMap {
        id: USER_MAP_ID.to_owned(),
        version: KEYMAP_SCHEMA_VERSION,
        name: "Fuzz map".to_owned(),
        bindings: vec![
            binding('s', &[KeyAction::Tone(Tone::Acute)]),
            binding('f', &[KeyAction::Tone(Tone::Grave)]),
            binding('w', &[KeyAction::HornOrBreve]),
            binding('d', &[KeyAction::Stroke]),
            binding('6', &[KeyAction::Circumflex]),
            binding('7', &[KeyAction::Horn, KeyAction::Breve]),
            binding(
                ';',
                &[
                    KeyAction::CircumflexOn(BaseVowel::A),
                    KeyAction::CircumflexOn(BaseVowel::E),
                    KeyAction::CircumflexOn(BaseVowel::O),
                    KeyAction::Breve,
                ],
            ),
            binding('[', &[KeyAction::Horn]),
            binding('/', &[KeyAction::Escape]),
            binding('.', &[KeyAction::Tone(Tone::Dot), KeyAction::RemoveTone]),
            binding('\'', &[KeyAction::Tone(Tone::Acute)]),
        ],
    }
}

/// Options of the key-script form: defaults v1 changed by the words of the first line.
fn script_options(line: &str) -> EngineOptions {
    let mut options = EngineOptions::defaults_v1();
    options.user_keymap_id = Some(USER_MAP_ID.to_owned());
    for word in line.split_whitespace() {
        match word {
            "telex" => options.input_method = InputMethod::Telex,
            "simple-telex" => options.input_method = InputMethod::SimpleTelex,
            "vni" => options.input_method = InputMethod::Vni,
            "viqr" => options.input_method = InputMethod::Viqr,
            "user" => options.input_method = InputMethod::UserDefined,
            "nfc" => options.normalization = Normalization::Nfc,
            "nfd" => options.normalization = Normalization::Nfd,
            "preserve" => options.normalization = Normalization::Preserve,
            "modern" => options.modern_tone = true,
            "nofree" => options.free_marking = false,
            "nospell" => options.spell_check = false,
            "norestore" => options.auto_restore_invalid = false,
            "nomacros" => options.macros_enabled = false,
            "always-macro" => options.macros_when_disabled = true,
            "exact-case" => options.auto_case_macro = false,
            "disabled" => options.enabled = false,
            other => {
                if let Some(limit) = other.strip_prefix("limit=").and_then(|n| n.parse().ok()) {
                    options.context_limit_codepoints = limit;
                }
            }
        }
    }
    options
}

/// Options of the binary form; context limits outside `8..=128` are included on purpose.
fn decode_options(reader: &mut Reader<'_>) -> EngineOptions {
    let input_method = reader.pick(&[
        InputMethod::Telex,
        InputMethod::Telex,
        InputMethod::Telex,
        InputMethod::SimpleTelex,
        InputMethod::Vni,
        InputMethod::Viqr,
        InputMethod::UserDefined,
    ]);
    let normalization = reader.pick(&[
        Normalization::Nfc,
        Normalization::Nfc,
        Normalization::Nfd,
        Normalization::Preserve,
    ]);
    let flags = reader.byte();
    let enabled = !reader.chance(32);
    let user_keymap_id = match reader.below(8) {
        0 => None,
        1 => Some("missing".to_owned()),
        _ => Some(USER_MAP_ID.to_owned()),
    };
    EngineOptions {
        enabled,
        input_method,
        normalization,
        modern_tone: flags & 1 != 0,
        free_marking: flags & 2 == 0,
        spell_check: flags & 4 == 0,
        auto_restore_invalid: flags & 8 == 0,
        macros_enabled: flags & 16 == 0,
        macros_when_disabled: flags & 32 != 0,
        auto_case_macro: flags & 64 == 0,
        user_keymap_id,
        context_limit_codepoints: reader.pick(&[32, 32, 8, 12, 128, 0, 7, 129, 1_000, u32::MAX]),
        charset: Default::default(),
    }
}

/// A user key map; one in 32 is invalid (repeated keys, action counts, or version).
fn decode_keymap(reader: &mut Reader<'_>) -> KeyMap {
    let corrupt = reader.chance(8);
    let mut bindings: Vec<KeyBinding> = Vec::new();
    for _ in 0..reader.below(13) {
        let key = char::from(reader.pick(KEYMAP_KEYS));
        if !corrupt
            && bindings
                .iter()
                .any(|binding| binding.key.eq_ignore_ascii_case(&key))
        {
            continue;
        }
        let count = if corrupt {
            reader.below(6)
        } else {
            1 + reader.below(KeyMap::MAX_ACTIONS)
        };
        let actions = (0..count).map(|_| reader.pick(&ACTIONS)).collect();
        bindings.push(KeyBinding { key, actions });
    }
    KeyMap {
        id: USER_MAP_ID.to_owned(),
        version: if corrupt && reader.flag() {
            KEYMAP_SCHEMA_VERSION + 1
        } else {
            KEYMAP_SCHEMA_VERSION
        },
        name: "Fuzz map".to_owned(),
        bindings,
    }
}

/// Up to two extra valid macros with short triggers and replacements.
fn decode_extra_macros(reader: &mut Reader<'_>) -> Vec<MacroEntry> {
    const TRIGGER: [char; 16] = [
        'a', 'k', 'o', 'v', 'n', 'd', 'c', 'A', 'K', 'O', 'ô', 'đ', '1', '.', '😀', '\u{302}',
    ];
    const REPLACEMENT: [char; 9] = ['x', 'ê', ' ', '\n', '\t', '😀', '\u{301}', 'A', '.'];
    let mut extra = Vec::new();
    for index in 0..reader.below(3) {
        let trigger: String = (0..=reader.below(4))
            .map(|_| reader.pick(&TRIGGER))
            .collect();
        let replacement: String = (0..=reader.below(9))
            .map(|_| reader.pick(&REPLACEMENT))
            .collect();
        let candidate = MacroEntry {
            id: MacroId(100 + u64::try_from(index).unwrap_or(0)),
            trigger,
            replacement,
            enabled: !reader.chance(32),
            boundary: reader.pick(&[BoundaryPolicy::Delimiter, BoundaryPolicy::Whitespace]),
            case_policy: reader.pick(&[CasePolicy::Adapt, CasePolicy::Exact]),
        };
        if let Ok(valid) = normalize_entry(candidate) {
            extra.push(valid);
        }
    }
    extra
}

/// One character of the mixed alphabet.
fn alphabet_char(reader: &mut Reader<'_>) -> char {
    match reader.below(16) {
        0..=8 => char::from(reader.pick(LETTERS)),
        9 => char::from(reader.pick(b"0123456789")),
        10 => char::from(reader.pick(b"'`?~.-^(+*\\")),
        11 => char::from(reader.pick(b",;:!\")[]{}<>/|@#$%&_=")),
        12 => reader.pick(&[
            'ă', 'â', 'ê', 'ô', 'ơ', 'ư', 'đ', 'Đ', 'á', 'ế', 'ọ', 'ữ', 'Ư',
        ]),
        13 => reader.pick(&[
            '\u{300}', '\u{301}', '\u{302}', '\u{303}', '\u{306}', '\u{309}', '\u{31B}', '\u{323}',
        ]),
        14 => reader.pick(&[
            '😀', '𝔸', '中', '€', 'ß', 'İ', '\u{FEFF}', '\u{A0}', '\u{2028}',
        ]),
        _ => reader.pick(&['\u{0}', '\u{8}', '\u{1B}', '\u{7F}', '\u{85}']),
    }
}

/// The macro table, counting the lookups and trigger-prefix queries the engine makes.
struct Watched {
    table: Arc<MacroTable>,
    queries: AtomicUsize,
}

impl MacroLookup for Watched {
    fn lookup(
        &self,
        typed: &str,
        boundary: Boundary,
        case_mode: CaseMode,
    ) -> Option<MacroExpansion> {
        self.queries.fetch_add(1, Ordering::Relaxed);
        self.table.lookup(typed, boundary, case_mode)
    }

    fn has_token_triggers(&self) -> bool {
        self.table.has_token_triggers()
    }

    fn is_trigger_prefix(&self, prefix: &str, case_mode: CaseMode) -> bool {
        self.queries.fetch_add(1, Ordering::Relaxed);
        self.table.is_trigger_prefix(prefix, case_mode)
    }
}

/// An adapter over two documents of one TSF session, applying every transaction the way the
/// TSF path does (design §5.2).
struct Session {
    engine: VietnameseEngine,
    macros: Arc<Watched>,
    entries: Vec<MacroEntry>,
    options: EngineOptions,
    /// The user key map passes validation (so the engine registered it).
    user_map_valid: bool,
    documents: [TextSimulator; 2],
    hosts: [HostContext; 2],
    active: usize,
    state: EngineState,
    /// The document the state's word belongs to (meaningful while the state is not idle).
    state_document: usize,
    secure: bool,
    /// The host edited the document since the state's word was last applied.
    stale: bool,
    clock: u64,
    events: usize,
}

impl Session {
    fn new(options: EngineOptions, user_map: KeyMap, extra: Vec<MacroEntry>) -> Self {
        let (base_entries, base) = base_table();
        let mut entries = base_entries.clone();
        let table = if extra.is_empty() {
            Arc::clone(base)
        } else {
            entries.extend(extra);
            match MacroTable::new(entries.clone()) {
                Ok(table) => Arc::new(table),
                Err(error) => panic!("valid entries with unique ids do not build a table: {error}"),
            }
        };
        let user_map_valid = user_map.validate().is_ok();
        let exe = ExeName::new("fuzz-engine.exe").expect("constant name is valid");
        let host = |document: u128| {
            let mut host = HostContext::new(SessionId::from_u128(0x456E), exe.clone());
            host.document_id = Some(DocumentId::from_u128(document));
            host
        };
        let macros = Arc::new(Watched {
            table,
            queries: AtomicUsize::new(0),
        });
        Self {
            engine: VietnameseEngine::new(vec![user_map], Arc::clone(&macros) as _),
            macros,
            entries,
            options,
            user_map_valid,
            documents: [TextSimulator::new(), TextSimulator::new()],
            hosts: [host(1), host(2)],
            active: 0,
            state: EngineState::new(),
            state_document: 0,
            secure: false,
            stale: false,
            clock: 0,
            events: 0,
        }
    }

    /// The active document's host.
    fn host(&self) -> HostContext {
        let mut host = self.hosts[self.active].clone();
        host.is_secure_field = self.secure;
        host
    }

    /// Whether the engine can resolve the configured input method.
    fn keymap_usable(&self) -> bool {
        self.options.input_method != InputMethod::UserDefined
            || (self.user_map_valid && self.options.user_keymap_id.as_deref() == Some(USER_MAP_ID))
    }

    fn character(&self, key: char) -> InputEvent {
        let mut event = InputEvent::character(key, self.host());
        event.modifiers.shift = key.is_ascii_uppercase();
        if key.is_ascii_alphanumeric() {
            event.virtual_key = Some(u32::from(key.to_ascii_uppercase()));
        }
        event
    }

    /// Processes one event like an adapter: apply, then commit; on failure, reset.
    fn feed(&mut self, mut event: InputEvent) {
        if self.events >= MAX_EVENTS {
            return;
        }
        self.events += 1;
        self.clock += 1;
        event.monotonic_time_ms = self.clock;
        let queries = self.macros.queries.load(Ordering::Relaxed);
        let outcome = self.engine.process(&event, &self.options, &self.state);
        assert!(
            self.engine.process(&event, &self.options, &self.state) == outcome,
            "processing is not deterministic"
        );
        assert!(
            !event.host.is_secure_field || self.macros.queries.load(Ordering::Relaxed) == queries,
            "a macro was looked up for a secure field (D25, matrix #29)"
        );
        match outcome {
            Ok(result) => self.commit(&event, result),
            Err(error) => self.recover(&event, error),
        }
    }

    /// Input tagged as Raumakey's own replay: the engine lets it through untouched. Its effect
    /// on the document came with the transaction that asked for the replay.
    fn inject(&mut self, mut event: InputEvent) {
        event.injected_tag = Some(RAUMAKEY_INJECTED_TAG);
        match self.engine.process(&event, &self.options, &self.state) {
            Ok(result) => assert!(
                result.transaction == EditTransaction::pass_through() && result.next == self.state,
                "Raumakey's own injected input was processed"
            ),
            Err(error) => panic!("Raumakey's own injected input failed with {error:?}"),
        }
    }

    fn commit(&mut self, event: &InputEvent, result: EngineResult) {
        let tx = &result.transaction;
        let next = &result.next;
        assert!(
            next.version() == STATE_VERSION,
            "the next state has another version"
        );
        assert!(
            *next == self.state || next.generation() == self.state.generation().wrapping_add(1),
            "the generation did not advance with the state"
        );
        assert!(
            self.state.is_idle() || self.state_document == self.active,
            "a word of another document was continued (matrix #20)"
        );
        if event.is_shortcut() {
            assert!(
                tx.is_pass_through() && tx.reset_context && next.is_idle(),
                "a shortcut reached the engine (matrix #21)"
            );
        }
        if tx.reset_context {
            assert!(next.is_idle(), "reset_context without resetting the state");
        }
        if let Some(replay) = tx.replay_key {
            assert!(
                matches!(replay.virtual_key, vk::RETURN | vk::TAB)
                    && event.virtual_key == Some(replay.virtual_key),
                "a replay of another key than Enter or Tab"
            );
        }
        let owned = self.state.owned_utf16().len();
        assert!(
            usize::try_from(tx.delete_before).is_ok_and(|delete| delete <= owned),
            "the engine deletes text it does not own (matrix #54)"
        );
        // Whatever the document holds: a well-formed insert within the limit, and a replay
        // only of a consumed key (plan §2.7).
        assert!(
            tx.validate(u32::MAX, u32::MAX).is_ok(),
            "the engine produced an invalid transaction"
        );
        let document = &mut self.documents[self.active];
        let len = document.units().len();
        let cut = len.checked_sub(usize::try_from(tx.delete_before).unwrap_or(usize::MAX));
        if self.stale {
            // The host changed the text behind the engine, so the deletion may no longer fit:
            // it may exceed the document or end inside a surrogate pair. The adapter commits
            // only an edit that applies, and otherwise resets (design §5.2). No other failure is
            // possible for a transaction that validated above.
            match document.apply(event, tx) {
                Ok(()) => {}
                Err(
                    TransactionError::DeleteExceedsContext { .. }
                    | TransactionError::SplitsSurrogate,
                ) => {
                    self.state = self.engine.reset(&self.state, ResetReason::ApplyFailed);
                    assert!(
                        document
                            .apply(event, &EditTransaction::pass_through_reset())
                            .is_ok(),
                        "a pass-through edit was rejected"
                    );
                    self.stale = false;
                    assert!(
                        well_formed(document.units()),
                        "the document is not well-formed UTF-16"
                    );
                    return;
                }
                Err(error) => panic!("a valid transaction failed to apply with {error:?}"),
            }
        } else {
            assert!(
                tx.validate(u32::try_from(len).unwrap_or(u32::MAX), u32::MAX)
                    .is_ok(),
                "the engine deletes more than the document holds"
            );
            assert!(
                cut.is_some_and(|cut| !splits_pair(document.units(), cut)),
                "the deletion splits a surrogate pair"
            );
            assert!(
                document.apply(event, tx).is_ok(),
                "the engine's transaction does not apply to the document"
            );
        }
        assert!(
            well_formed(document.units()),
            "the document is not well-formed UTF-16"
        );
        assert!(
            self.stale || document.units().ends_with(next.owned_utf16()),
            "the owned text is not the document suffix"
        );
        self.state = result.next;
        if self.state.is_idle() {
            self.stale = false;
        } else {
            self.state_document = self.active;
        }
    }

    /// The application edits the active document without a key the engine sees (an
    /// autocorrection, a paste): the engine's word may no longer be the document's suffix.
    fn host_edit(&mut self, reader: &mut Reader<'_>) {
        let host = self.host();
        let event = if reader.flag() {
            InputEvent::backspace(host)
        } else {
            let text = reader.pick(&["x", "😀", "e\u{302}", " ", "Đ"]);
            InputEvent::text(text.encode_utf16().collect(), host)
        };
        let document = &mut self.documents[self.active];
        assert!(
            document
                .apply(&event, &EditTransaction::pass_through())
                .is_ok(),
            "a pass-through edit was rejected"
        );
        if !self.state.is_idle() && self.state_document == self.active {
            self.stale = true;
        }
    }

    fn recover(&mut self, event: &InputEvent, error: EngineError) {
        let own = event.validate();
        match error {
            EngineError::InvalidUtf16
            | EngineError::CapacityExceeded
            | EngineError::InternalInvariant => assert!(
                own == Err(error),
                "{error:?} for an event that does not break that rule"
            ),
            EngineError::ContextMismatch => assert!(
                own.is_ok() && !self.state.is_idle() && self.state_document != self.active,
                "ContextMismatch without a word of another document"
            ),
            EngineError::UnsupportedInputMethod => assert!(
                own.is_ok() && self.options.enabled && !self.keymap_usable(),
                "UnsupportedInputMethod although the key map is available"
            ),
            EngineError::InvalidStateVersion => panic!("the engine rejected a state it produced"),
        }
        // Nothing is committed: the adapter resets the engine.
        self.state = self.engine.reset(&self.state, ResetReason::ApplyFailed);
        self.stale = false;
        assert!(
            self.state.is_idle() && self.state.version() == STATE_VERSION,
            "reset did not produce an idle state"
        );
        if error == EngineError::ContextMismatch {
            // The adapter noticed the document change late; it processes the key again.
            match self.engine.process(event, &self.options, &self.state) {
                Ok(result) => self.commit(event, result),
                Err(again) => {
                    assert!(
                        again != EngineError::ContextMismatch,
                        "a reset state still mismatches"
                    );
                    self.recover(event, again);
                }
            }
        } else if own.is_ok() {
            // A well-formed key passes through.
            let document = &mut self.documents[self.active];
            assert!(
                document
                    .apply(event, &EditTransaction::pass_through_reset())
                    .is_ok(),
                "a pass-through edit was rejected"
            );
        }
    }

    /// Types a trigger of the table (as stored or in another case) and ends the word.
    fn type_trigger(&mut self, reader: &mut Reader<'_>) {
        let Some(entry) = self.entries.get(reader.below(self.entries.len())) else {
            return;
        };
        let trigger = entry.trigger.clone();
        let variant = reader.below(5);
        for (index, c) in trigger.chars().enumerate() {
            let typed: String = match variant {
                0 => c.to_string(),
                1 => c.to_lowercase().collect(),
                2 => c.to_uppercase().collect(),
                3 if index == 0 => c.to_uppercase().collect(),
                3 => c.to_lowercase().collect(),
                _ if reader.flag() => c.to_uppercase().collect(),
                _ => c.to_lowercase().collect(),
            };
            for key in typed.chars() {
                self.feed(self.character(key));
            }
        }
        let host = self.host();
        let boundary = match reader.below(10) {
            0..=3 => InputEvent::separator(' ', vk::SPACE, host),
            4 => InputEvent::separator('\r', vk::RETURN, host),
            5 => InputEvent::separator('\t', vk::TAB, host),
            6 => {
                let mut event = InputEvent::separator(' ', vk::SPACE, host);
                event.modifiers.shift = true;
                event
            }
            7 => self.character(char::from(reader.pick(b",.;!?"))),
            8 => {
                let mut event = InputEvent::separator(' ', vk::SPACE, host);
                event.modifiers.ctrl = true;
                event
            }
            _ => return,
        };
        self.feed(boundary);
    }

    /// Types a word of the active input method, sometimes followed by backspaces.
    fn type_word(&mut self, reader: &mut Reader<'_>) {
        let words = match self.options.input_method {
            InputMethod::Vni => VNI_WORDS,
            InputMethod::Viqr => VIQR_WORDS,
            InputMethod::Telex | InputMethod::SimpleTelex | InputMethod::UserDefined => TELEX_WORDS,
        };
        let word = reader.pick(words);
        for key in word.chars() {
            self.feed(self.character(key));
        }
        for _ in 0..reader.below(4) {
            self.feed(InputEvent::backspace(self.host()));
        }
    }

    /// A configuration change the engine is not told about: the next keys arrive with a state
    /// built under the old options. With `announce`, a `ConfigChanged` event goes first.
    fn change_options(&mut self, reader: &mut Reader<'_>, announce: bool) {
        if announce {
            self.feed(InputEvent::simple(InputKind::ConfigChanged, self.host()));
        }
        let options = &mut self.options;
        match reader.below(8) {
            0 => {
                options.normalization = reader.pick(&[
                    Normalization::Nfc,
                    Normalization::Nfd,
                    Normalization::Preserve,
                ]);
            }
            1 => {
                options.input_method = reader.pick(&[
                    InputMethod::Telex,
                    InputMethod::SimpleTelex,
                    InputMethod::Vni,
                    InputMethod::Viqr,
                    InputMethod::UserDefined,
                ]);
            }
            2 => options.spell_check = !options.spell_check,
            3 => options.modern_tone = !options.modern_tone,
            4 => options.free_marking = !options.free_marking,
            5 => options.macros_when_disabled = !options.macros_when_disabled,
            6 => options.context_limit_codepoints = reader.pick(&[8, 12, 32, 128, 0, 1_000]),
            _ => options.enabled = !options.enabled,
        }
    }

    /// One operation of the binary form.
    fn step(&mut self, reader: &mut Reader<'_>) {
        match reader.below(100) {
            0..=44 => {
                let key = alphabet_char(reader);
                self.feed(self.character(key));
            }
            45..=54 => {
                let key = char::from(reader.pick(METHOD_KEYS));
                self.feed(self.character(key));
            }
            55..=60 => self.feed(InputEvent::backspace(self.host())),
            61..=67 => {
                let (key, virtual_key) = reader.pick(&[
                    (' ', vk::SPACE),
                    (' ', vk::SPACE),
                    ('\r', vk::RETURN),
                    ('\t', vk::TAB),
                ]);
                let mut event = InputEvent::separator(key, virtual_key, self.host());
                match reader.below(8) {
                    5 => event.modifiers.shift = true,
                    6 => event.modifiers.ctrl = true,
                    7 => event.modifiers.alt = true,
                    _ => {}
                }
                self.feed(event);
            }
            68 | 69 => {
                let key = reader.pick(&[
                    vk::LEFT,
                    vk::RIGHT,
                    vk::UP,
                    vk::DOWN,
                    vk::HOME,
                    vk::END,
                    vk::PRIOR,
                    vk::NEXT,
                    vk::DELETE,
                    vk::ESCAPE,
                    vk::INSERT,
                ]);
                self.feed(InputEvent::navigation(key, self.host()));
            }
            70 | 71 => {
                let kind = reader.pick(&[
                    InputKind::MouseReset,
                    InputKind::FocusReset,
                    InputKind::ConfigChanged,
                    InputKind::Shutdown,
                    InputKind::ModeToggle,
                ]);
                self.feed(InputEvent::simple(kind, self.host()));
            }
            72..=74 => {
                let key = alphabet_char(reader);
                let mut event = if reader.flag() {
                    self.character(key)
                } else {
                    let mut event = InputEvent::text(Vec::new(), self.host());
                    event.virtual_key = Some(u32::from(reader.pick(b"ACVXZ")));
                    event
                };
                match reader.below(3) {
                    0 => event.modifiers.ctrl = true,
                    1 => event.modifiers.alt = true,
                    _ => event.modifiers.win = true,
                }
                event.modifiers.shift = reader.flag();
                self.feed(event);
            }
            75 | 76 => {
                let key = reader.pick(&['@', '€', '{', '}', '[', '\\', 'a', 'đ', 'ß']);
                let mut event = self.character(key);
                event.modifiers.ctrl = true;
                event.modifiers.alt = true;
                event.modifiers.alt_gr = true;
                self.feed(event);
            }
            77 | 78 => {
                let text = reader.pick(&[" ", "\t", "\r", "\n", "\u{8}", "ab", "😀😀", "a\u{301}"]);
                let event = InputEvent::text(text.encode_utf16().collect(), self.host());
                self.feed(event);
            }
            79 => {
                let mut event = match reader.below(4) {
                    0 => InputEvent::text(vec![0x61, 0xD800], self.host()),
                    1 => InputEvent::text(vec![0x61; 17], self.host()),
                    2 => InputEvent::backspace(self.host()),
                    _ => InputEvent::navigation(vk::LEFT, self.host()),
                };
                if event.text_utf16.is_empty() {
                    event.text_utf16 = vec![0x61];
                }
                self.feed(event);
            }
            80 | 81 => {
                // Focus moves into or out of a secure field.
                self.secure = !self.secure;
                self.feed(InputEvent::simple(InputKind::FocusReset, self.host()));
            }
            82 | 83 => {
                let event = match reader.below(4) {
                    0 => InputEvent::separator('\r', vk::RETURN, self.host()),
                    1 => InputEvent::separator('\t', vk::TAB, self.host()),
                    2 => InputEvent::backspace(self.host()),
                    _ => self.character(alphabet_char(reader)),
                };
                self.inject(event);
            }
            84 => {
                let tag = reader.pick(&[FOREIGN_TAG, 0, u64::MAX, RAUMAKEY_INJECTED_TAG + 1]);
                let event = self.character(alphabet_char(reader)).with_injected_tag(tag);
                self.feed(event);
            }
            85..=87 => self.type_trigger(reader),
            88..=91 => self.type_word(reader),
            92 | 93 => {
                if reader.flag() {
                    // An adapter that notices the document change resets the engine first.
                    self.state = self.engine.reset(&self.state, ResetReason::ContextChanged);
                    self.stale = false;
                }
                self.active = 1 - self.active;
            }
            94 | 95 => {
                self.feed(InputEvent::simple(InputKind::ModeToggle, self.host()));
                self.options.enabled = !self.options.enabled;
            }
            96 | 97 => {
                let announce = reader.flag();
                self.change_options(reader, announce);
            }
            98 => self.host_edit(reader),
            _ => {
                let mut event = self.character(alphabet_char(reader));
                event.modifiers.caps_lock = reader.flag();
                event.repeat = reader.flag();
                event.scan_code = Some(u32::from(reader.byte()));
                self.feed(event);
            }
        }
    }
}

/// The key-script form: `#!<options>\n<key script>`.
fn run_script(data: &[u8]) {
    let text = String::from_utf8_lossy(data.get(SCRIPT_PREFIX.len()..).unwrap_or_default());
    let (line, keys) = text.split_once('\n').unwrap_or((&text, ""));
    let options = script_options(line);
    let tokens: Vec<KeyToken> = script::parse_keys(keys)
        .unwrap_or_else(|_| keys.chars().map(KeyToken::Char).collect())
        .into_iter()
        .take(MAX_EVENTS)
        .collect();
    let mut session = Session::new(options.clone(), script_user_map(), Vec::new());
    for token in &tokens {
        match token {
            KeyToken::SecureOn => session.secure = true,
            KeyToken::SecureOff => session.secure = false,
            _ => {}
        }
        let event = script::event_for(token, &session.host(), session.secure);
        session.feed(event);
    }
    // The script runner, an independent adapter loop, ends with the same document.
    let host = session.hosts[0].clone();
    if let Ok(outcome) = script::run_script_with(&session.engine, &options, &tokens, &host, "") {
        assert!(
            outcome.steps.iter().all(|step| step.accepted),
            "the script runner had a transaction rejected"
        );
        assert!(
            outcome.final_text == session.documents[0].as_string(),
            "the script runner and this adapter disagree"
        );
    }
}

pub(crate) fn run(data: &[u8]) {
    if data.starts_with(SCRIPT_PREFIX) {
        run_script(data);
        return;
    }
    let mut reader = Reader::new(data);
    let options = decode_options(&mut reader);
    let user_map = decode_keymap(&mut reader);
    let extra = decode_extra_macros(&mut reader);
    let mut session = Session::new(options, user_map, extra);
    while session.events < MAX_EVENTS && !reader.is_empty() {
        session.step(&mut reader);
    }
}

#[cfg(test)]
mod tests {
    use raumakey_model::input::{ModifierSet, ReplayKey};
    use raumakey_model::transaction::MAX_INSERT_UNITS;

    use super::*;

    /// A session whose engine owns `a` in a document that the host edited since.
    fn stale_session() -> Session {
        let mut session = Session::new(EngineOptions::defaults_v1(), script_user_map(), Vec::new());
        session.feed(session.character('a'));
        assert!(!session.state.is_idle(), "the engine owns the word");
        session.stale = true;
        session
    }

    /// An engine result that keeps the current state and asks for `transaction`.
    fn result(session: &Session, transaction: EditTransaction) -> EngineResult {
        EngineResult {
            transaction,
            next: session.state.clone(),
        }
    }

    #[test]
    #[should_panic(expected = "the engine produced an invalid transaction")]
    fn a_malformed_insert_is_a_finding_after_a_host_edit() {
        let mut session = stale_session();
        let event = session.character('b');
        let tx = EditTransaction {
            insert_utf16: vec![0xD800],
            consume_original: true,
            ..EditTransaction::default()
        };
        session.commit(&event, result(&session, tx));
    }

    #[test]
    #[should_panic(expected = "the engine produced an invalid transaction")]
    fn an_insert_over_the_limit_is_a_finding_after_a_host_edit() {
        let mut session = stale_session();
        let event = session.character('b');
        let tx = EditTransaction {
            insert_utf16: vec![u16::from(b'a'); MAX_INSERT_UNITS + 1],
            consume_original: true,
            ..EditTransaction::default()
        };
        session.commit(&event, result(&session, tx));
    }

    #[test]
    #[should_panic(expected = "the engine produced an invalid transaction")]
    fn a_replay_without_consume_is_a_finding_after_a_host_edit() {
        let mut session = stale_session();
        let event = InputEvent::separator('\r', vk::RETURN, session.host());
        let tx = EditTransaction {
            replay_key: Some(ReplayKey {
                virtual_key: vk::RETURN,
                scan_code: None,
                modifiers: ModifierSet::default(),
            }),
            ..EditTransaction::default()
        };
        session.commit(&event, result(&session, tx));
    }

    #[test]
    fn a_deletion_that_no_longer_fits_resets_after_a_host_edit() {
        // The host removed the engine's text: deleting it now exceeds the document.
        let mut session = stale_session();
        session.documents[session.active] = TextSimulator::new();
        let event = session.character('b');
        session.commit(&event, result(&session, EditTransaction::replace(1, "ab")));
        assert!(session.state.is_idle() && !session.stale);
        assert_eq!(session.documents[session.active].as_string(), "b");
    }

    #[test]
    fn a_deletion_that_splits_a_pair_resets_after_a_host_edit() {
        // The host appended an astral character after the engine's `a`.
        let mut session = stale_session();
        let host = session.host();
        let emoji = InputEvent::text("😀".encode_utf16().collect(), host);
        assert!(
            session.documents[session.active]
                .apply(&emoji, &EditTransaction::pass_through())
                .is_ok()
        );
        let event = session.character('b');
        session.commit(&event, result(&session, EditTransaction::replace(1, "ab")));
        assert!(session.state.is_idle() && !session.stale);
        assert_eq!(session.documents[session.active].as_string(), "a😀b");
    }
}
