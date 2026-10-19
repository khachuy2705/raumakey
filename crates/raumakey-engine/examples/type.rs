//! Types a key script through the Raumakey engine and prints every transaction.
//!
//! ```text
//! cargo run -q -p raumakey-engine --example type -- [--method telex|simple-telex|vni|viqr] [--modern]
//!     [--nfd|--preserve] [--no-spell] [--no-restore] [--no-free] [--off] [--macros-when-off]
//!     [--no-autocase] [--context N] [--macro TRIGGER=REPLACEMENT ...] "<script>"
//! ```
//!
//! Output: one line per step, `step N <token>: Tx(delete,"insert",consume,replay,reset)`, then
//! `final: "<text>"` (UTF-8). Script syntax: see `raumakey_engine::script`.
//!
//! Output is UTF-8 in every case, and the example never changes console settings: a console
//! shows the text through its Unicode API whatever its code page, and a pipe receives UTF-8
//! bytes that the reader must decode as UTF-8 (in PowerShell set
//! `[Console]::OutputEncoding = [Text.Encoding]::UTF8` first). Exit codes: 0 success, 1 engine
//! error, 2 usage or script error.

use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::Arc;

use raumakey_engine::VietnameseEngine;
use raumakey_engine::script::{KeyToken, format_transaction, parse_keys, quote, run_script};
use raumakey_model::macros::{Boundary, CaseMode, MacroExpansion, MacroId, MacroLookup};
use raumakey_model::options::{EngineOptions, InputMethod, Normalization};

const USAGE: &str = "usage: type [--method telex|simple-telex|vni|viqr] [--modern] [--nfd|--preserve] \
[--no-spell] [--no-restore] [--no-free] [--off] [--macros-when-off] [--no-autocase] \
[--context N] [--macro TRIGGER=REPLACEMENT ...] \"<script>\"";

/// Macro double: exact trigger first; in adaptive mode a lowercase match whose replacement
/// follows the typed case (all upper with more than one letter → upper; first letter upper →
/// capitalized).
struct ExampleMacros {
    entries: Vec<(String, String)>,
}

impl ExampleMacros {
    fn expansion(index: usize, replacement: String) -> MacroExpansion {
        MacroExpansion {
            id: MacroId(u64::try_from(index).unwrap_or(u64::MAX)),
            replacement,
        }
    }
}

fn adapt_case(typed: &str, replacement: &str) -> String {
    let letters: Vec<char> = typed.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return replacement.to_uppercase();
    }
    if letters.first().is_some_and(|c| c.is_uppercase()) {
        let mut chars = replacement.chars();
        return chars
            .next()
            .map(|first| first.to_uppercase().chain(chars).collect())
            .unwrap_or_default();
    }
    replacement.to_owned()
}

impl MacroLookup for ExampleMacros {
    fn lookup(
        &self,
        typed: &str,
        _boundary: Boundary,
        case_mode: CaseMode,
    ) -> Option<MacroExpansion> {
        if let Some((index, (_, replacement))) = self
            .entries
            .iter()
            .enumerate()
            .find(|(_, (trigger, _))| trigger == typed)
        {
            return Some(Self::expansion(index, replacement.clone()));
        }
        if case_mode == CaseMode::Exact {
            return None;
        }
        let lower = typed.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .find(|(_, (trigger, _))| trigger.to_lowercase() == lower)
            .map(|(index, (_, replacement))| Self::expansion(index, adapt_case(typed, replacement)))
    }
}

struct Invocation {
    options: EngineOptions,
    macros: Vec<(String, String)>,
    script: String,
}

fn parse_args(args: &[String]) -> Result<Invocation, String> {
    let mut options = EngineOptions::defaults_v1();
    let mut macros = Vec::new();
    let mut positional = Vec::new();
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        index += 1;
        match arg.as_str() {
            "--method" => {
                let value = args.get(index).ok_or("--method needs a value")?;
                index += 1;
                options.input_method = match value.to_ascii_lowercase().as_str() {
                    "telex" => InputMethod::Telex,
                    "simple-telex" => InputMethod::SimpleTelex,
                    "vni" => InputMethod::Vni,
                    "viqr" => InputMethod::Viqr,
                    _ => return Err(format!("unknown method {value:?}")),
                };
            }
            "--modern" => options.modern_tone = true,
            "--nfd" => options.normalization = Normalization::Nfd,
            "--preserve" => options.normalization = Normalization::Preserve,
            "--no-spell" => options.spell_check = false,
            "--no-restore" => options.auto_restore_invalid = false,
            "--no-free" => options.free_marking = false,
            "--off" => options.enabled = false,
            "--macros-when-off" => options.macros_when_disabled = true,
            "--no-autocase" => options.auto_case_macro = false,
            "--context" => {
                let value = args.get(index).ok_or("--context needs a value")?;
                index += 1;
                options.context_limit_codepoints = value
                    .parse()
                    .map_err(|_| format!("invalid context limit {value:?}"))?;
            }
            "--macro" => {
                let mut taken = 0;
                // Every following TRIGGER=REPLACEMENT except the final script argument.
                while index + 1 < args.len() {
                    let Some((trigger, replacement)) = args
                        .get(index)
                        .filter(|next| !next.starts_with("--"))
                        .and_then(|next| next.split_once('='))
                    else {
                        break;
                    };
                    macros.push((trigger.to_owned(), replacement.to_owned()));
                    index += 1;
                    taken += 1;
                }
                if taken == 0 {
                    return Err("--macro needs TRIGGER=REPLACEMENT".to_owned());
                }
            }
            "--help" | "-h" => return Err(String::new()),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            _ => positional.push(arg.clone()),
        }
    }
    if positional.is_empty() {
        return Err("missing script".to_owned());
    }
    Ok(Invocation {
        options,
        macros,
        script: positional.join(" "),
    })
}

/// Token label for the step lines; whitespace and control characters are shown as code points.
fn label(token: &KeyToken) -> String {
    match token {
        KeyToken::Char(c) if c.is_whitespace() || c.is_control() => {
            format!("U+{:04X}", u32::from(*c))
        }
        other => other.to_string(),
    }
}

/// Runs the tool with `args` (without the program name). Results go to `out`, diagnostics to
/// `err`; returns the exit code. Visible to `tests/type_example.rs`, which includes this file.
pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let invocation = match parse_args(args) {
        Ok(invocation) => invocation,
        Err(message) if message.is_empty() => {
            // --help
            let _ = writeln!(out, "{USAGE}");
            return 0;
        }
        Err(message) => {
            let _ = writeln!(err, "error: {message}\n{USAGE}");
            return 2;
        }
    };
    let tokens = match parse_keys(&invocation.script) {
        Ok(tokens) => tokens,
        Err(error) => {
            let _ = writeln!(err, "error: invalid script: {error}");
            return 2;
        }
    };
    let engine = VietnameseEngine::new(
        Vec::new(),
        Arc::new(ExampleMacros {
            entries: invocation.macros,
        }),
    );
    let outcome = match run_script(&engine, &invocation.options, &tokens) {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = writeln!(err, "error: {error}");
            return 1;
        }
    };
    for (number, step) in outcome.steps.iter().enumerate() {
        let rejected = if step.accepted { "" } else { " (rejected)" };
        let _ = writeln!(
            out,
            "step {} {}: {}{rejected}",
            number + 1,
            label(&step.token),
            format_transaction(&step.transaction)
        );
    }
    let _ = writeln!(out, "final: {}", quote(&outcome.final_text));
    let _ = out.flush();
    0
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = run(&args, &mut io::stdout().lock(), &mut io::stderr().lock());
    ExitCode::from(code)
}
