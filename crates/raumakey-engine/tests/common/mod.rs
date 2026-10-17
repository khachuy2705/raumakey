//! Shared helpers for the engine integration tests.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use raumakey_engine::VietnameseEngine;
use raumakey_engine::script::{self, ScriptOutcome};
use raumakey_model::input::is_boundary_punctuation;
use raumakey_model::macros::{Boundary, CaseMode, MacroExpansion, MacroId, MacroLookup, NoMacros};
use raumakey_model::options::{EngineOptions, InputMethod, Normalization};

/// Engine without user key maps or macros.
pub fn engine() -> VietnameseEngine {
    VietnameseEngine::new(Vec::new(), Arc::new(NoMacros))
}

/// Engine with a macro test double.
pub fn engine_with(macros: Arc<MacroDouble>) -> VietnameseEngine {
    VietnameseEngine::new(Vec::new(), macros)
}

/// Defaults v1 with `method`.
pub fn options(method: InputMethod) -> EngineOptions {
    EngineOptions {
        input_method: method,
        ..EngineOptions::defaults_v1()
    }
}

pub fn telex() -> EngineOptions {
    options(InputMethod::Telex)
}

pub fn vni() -> EngineOptions {
    options(InputMethod::Vni)
}

pub fn viqr() -> EngineOptions {
    options(InputMethod::Viqr)
}

pub fn modern(mut options: EngineOptions) -> EngineOptions {
    options.modern_tone = true;
    options
}

pub fn nfd(mut options: EngineOptions) -> EngineOptions {
    options.normalization = Normalization::Nfd;
    options
}

/// Runs a key script and returns the outcome.
pub fn run(engine: &VietnameseEngine, options: &EngineOptions, keys: &str) -> ScriptOutcome {
    let tokens = script::parse_keys(keys).expect("valid script");
    script::run_script(engine, options, &tokens).expect("script runs")
}

/// Final text of a key script.
pub fn typed(engine: &VietnameseEngine, options: &EngineOptions, keys: &str) -> String {
    run(engine, options, keys).final_text
}

/// Final text with the default engine.
pub fn type_keys(options: &EngineOptions, keys: &str) -> String {
    typed(&engine(), options, keys)
}

/// Transactions of every step in matrix notation.
pub fn txs(engine: &VietnameseEngine, options: &EngineOptions, keys: &str) -> Vec<String> {
    run(engine, options, keys)
        .steps
        .iter()
        .map(|step| script::format_transaction(&step.transaction))
        .collect()
}

/// Macro lookup double: exact trigger first, then (adaptive) a lowercase match with the
/// replacement adapted to the typed case. Records every lookup and every prefix query.
/// Triggers with boundary punctuation are token triggers (D28).
#[derive(Debug, Default)]
pub struct MacroDouble {
    entries: Vec<(String, String)>,
    pub calls: Mutex<Vec<(String, Boundary, CaseMode)>>,
    pub prefix_calls: Mutex<Vec<(String, CaseMode)>>,
}

impl MacroDouble {
    pub fn new(entries: &[(&str, &str)]) -> Arc<Self> {
        Arc::new(Self {
            entries: entries
                .iter()
                .map(|(t, r)| ((*t).to_owned(), (*r).to_owned()))
                .collect(),
            calls: Mutex::new(Vec::new()),
            prefix_calls: Mutex::new(Vec::new()),
        })
    }

    pub fn calls(&self) -> Vec<(String, Boundary, CaseMode)> {
        self.calls.lock().expect("calls").clone()
    }

    pub fn prefix_calls(&self) -> Vec<(String, CaseMode)> {
        self.prefix_calls.lock().expect("prefix calls").clone()
    }
}

fn adapt(typed: &str, replacement: &str) -> String {
    let letters: Vec<char> = typed.chars().filter(|c| c.is_alphabetic()).collect();
    let all_upper = letters.len() > 1 && letters.iter().all(|c| c.is_uppercase());
    if all_upper {
        return replacement.to_uppercase();
    }
    if letters.first().is_some_and(|c| c.is_uppercase()) {
        let mut chars = replacement.chars();
        return match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        };
    }
    replacement.to_owned()
}

impl MacroLookup for MacroDouble {
    fn lookup(
        &self,
        typed: &str,
        boundary: Boundary,
        case_mode: CaseMode,
    ) -> Option<MacroExpansion> {
        self.calls
            .lock()
            .expect("calls")
            .push((typed.to_owned(), boundary, case_mode));
        if let Some((index, (_, replacement))) = self
            .entries
            .iter()
            .enumerate()
            .find(|(_, (trigger, _))| trigger == typed)
        {
            return Some(MacroExpansion {
                id: MacroId(index as u64),
                replacement: replacement.clone(),
            });
        }
        if case_mode == CaseMode::Adaptive {
            let lower = typed.to_lowercase();
            if let Some((index, (_, replacement))) = self
                .entries
                .iter()
                .enumerate()
                .find(|(_, (trigger, _))| trigger.to_lowercase() == lower)
            {
                return Some(MacroExpansion {
                    id: MacroId(index as u64),
                    replacement: adapt(typed, replacement),
                });
            }
        }
        None
    }

    fn has_token_triggers(&self) -> bool {
        self.entries
            .iter()
            .any(|(trigger, _)| trigger.chars().any(is_boundary_punctuation))
    }

    fn is_trigger_prefix(&self, prefix: &str, case_mode: CaseMode) -> bool {
        self.prefix_calls
            .lock()
            .expect("prefix calls")
            .push((prefix.to_owned(), case_mode));
        let lower = prefix.to_lowercase();
        self.entries.iter().any(|(trigger, _)| {
            trigger.starts_with(prefix)
                || (case_mode == CaseMode::Adaptive && trigger.to_lowercase().starts_with(&lower))
        })
    }
}
