//! The excluded applications (D47, D48) as pure data: the edits that the exclusions dialog
//! (`crate::exclusions_dialog`) asks for ([`ExclusionsRequest`]) and what each does to the list
//! of `[exclusions] apps` in `raumakey.toml`, and the state the dialog shows
//! ([`ExclusionsState`], built by the tray, `crate::tray::Tray::exclusions_state`).
//!
//! # Data flow
//!
//! The tray is the only source of the dialog's data, as for the settings panel (D43): the
//! dialog sends an edit, the tray hands it to the settings worker
//! (`crate::worker::WorkerCommand::Exclusions`), whose `Settings` applies it to `[exclusions]
//! apps` in effect and commits it at once (`crate::settings::Settings::edit_exclusions`, with
//! the guarantees of every change: a change that cannot be saved stays in effect and is
//! retried). The tray does not work the edit out itself: it takes the new list and the new
//! runtime snapshot (whose compatibility rules start with one rule per excluded application)
//! from the worker's answer, and sends the dialog the list again after every edit, and
//! whenever the list in effect changes another way (a hand edit of `[exclusions]`).
//!
//! An edit names an application by its executable ([`ExeName`]), normalized the way the file
//! is read (`raumakey_config::normalize_excluded_app`): "Thêm" adds one that is not listed
//! yet (at the end), "Xoá" removes one, and "Mặc định" puts back
//! [`ExclusionsSection::DEFAULT_APPS`](raumakey_config::ExclusionsSection::DEFAULT_APPS) (the
//! Remote Desktop and Hyper-V clients). An empty list is a list like any other: it excludes
//! nothing, and is written as `apps = []`.
//!
//! Nothing here holds keystrokes, document text, or macro text: executable names only.

use std::path::Path;

use raumakey_config::ExclusionsSection;
use raumakey_model::host::ExeName;

use crate::settings::Location;

/// An edit of `[exclusions] apps` that the exclusions dialog asks for (D48).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExclusionsRequest {
    /// "Thêm": the application is excluded, added at the end of the list unless it is listed.
    Add(ExeName),
    /// "Xoá": the application is no longer excluded.
    Remove(ExeName),
    /// "Mặc định": the list is [`default_apps`] again.
    Defaults,
}

impl ExclusionsRequest {
    /// Whether `apps` is what this edit makes of it already: nothing to change, nothing to
    /// write.
    pub fn holds_in(&self, apps: &[ExeName]) -> bool {
        match self {
            Self::Add(exe) => apps.contains(exe),
            Self::Remove(exe) => !apps.contains(exe),
            Self::Defaults => apps == default_apps(),
        }
    }

    /// Makes this edit on `apps`.
    pub fn apply_to(&self, apps: &mut Vec<ExeName>) {
        match self {
            Self::Add(exe) => {
                if !apps.contains(exe) {
                    apps.push(exe.clone());
                }
            }
            Self::Remove(exe) => apps.retain(|listed| listed != exe),
            Self::Defaults => *apps = default_apps(),
        }
    }
}

/// The applications excluded unless the file says otherwise
/// ([`ExclusionsSection::DEFAULT_APPS`], in their order).
pub fn default_apps() -> Vec<ExeName> {
    ExclusionsSection::default().apps
}

/// What the exclusions dialog shows: the list in effect, as the tray holds it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExclusionsState {
    /// `[exclusions] apps` in effect, in the order of the file.
    pub apps: Vec<ExeName>,
    /// Whether an edit is saved: there is a folder for `raumakey.toml`. Without one, the
    /// edits take effect until Raumakey exits.
    pub can_save: bool,
}

impl ExclusionsState {
    /// The state of the list `apps` in effect, with `raumakey.toml` at `config_path` in
    /// `location` (none for [`Location::Unavailable`]).
    pub fn new(apps: &[ExeName], location: Location, config_path: Option<&Path>) -> Self {
        Self {
            apps: apps.to_vec(),
            can_save: location != Location::Unavailable && config_path.is_some(),
        }
    }
}
