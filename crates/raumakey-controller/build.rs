//! Build script of `raumakey-controller`: the Windows resources of `raumakey.exe`.
//!
//! On Windows MSVC targets, `raumakey_resources::build_script` writes `raumakey.res` (the
//! `asInvoker` manifest, version information, and the app, `V`, and `E` icons) into `OUT_DIR`
//! and prints `cargo:rustc-link-arg-bin=raumakey=<path>`, so only the `raumakey` bin target
//! links it, including that target's own unit-test harness: not the library, the integration
//! test binaries, or other packages. Other targets get no resources.
//!
//! On every target it also writes `icon_ids.rs` into `OUT_DIR`: the icon group ids of the app
//! icon (the settings panel's) and of the `V` and `E` icons as `raumakey-resources` assigns
//! them. `src/tray.rs` includes that file, so the controller needs `raumakey-resources` only
//! as a build-dependency (plan §1.1); its tests compare the ids with `raumakey-resources`
//! again. Nothing is written outside `OUT_DIR`.

use std::io;
use std::path::PathBuf;

use raumakey_resources::{Artifact, ICON_APP, ICON_ENGLISH, ICON_VIETNAMESE, build_script};

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    build_script(Artifact::Controller)?;
    write_icon_ids()
}

/// Writes `OUT_DIR/icon_ids.rs` with the documented constants `ICON_APP`, `ICON_VIETNAMESE`,
/// and `ICON_ENGLISH`.
fn write_icon_ids() -> io::Result<()> {
    let out_dir = std::env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("OUT_DIR is not set"))?;
    let text = format!(
        "/// Icon group of the app icon in the resources of `raumakey.exe`, which the settings\n\
         /// panel shows (`raumakey_resources::ICON_APP`, copied by `build.rs`).\n\
         pub const ICON_APP: u16 = {ICON_APP};\n\
         /// Icon group of the Vietnamese-mode icon (\"V\") in the resources of `raumakey.exe`\n\
         /// (`raumakey_resources::ICON_VIETNAMESE`, copied by `build.rs`).\n\
         pub const ICON_VIETNAMESE: u16 = {ICON_VIETNAMESE};\n\
         /// Icon group of the English-mode icon (\"E\") in the resources of `raumakey.exe`\n\
         /// (`raumakey_resources::ICON_ENGLISH`, copied by `build.rs`).\n\
         pub const ICON_ENGLISH: u16 = {ICON_ENGLISH};\n"
    );
    std::fs::write(out_dir.join("icon_ids.rs"), text)
}
