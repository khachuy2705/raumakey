//! Build script of `raumakey-tsf`: the Windows resources of `raumakey_tip.dll`.
//!
//! On Windows MSVC targets, `raumakey_resources::build_script` writes `raumakey_tip.res`
//! (version information and the app, `V`, and `E` icon groups; a DLL has no manifest) into
//! `OUT_DIR` and prints `cargo:rustc-link-arg-cdylib=<path>`, so only the cdylib links it: not
//! the rlib, the unit-test harness, or the integration test binaries. The first icon group is
//! the profile icon that TSF registration names by index
//! (`raumakey_windows::tsf_registration::PROFILE_ICON_INDEX`; `tests/resources.rs` keeps the
//! two definitions in step). Other targets get no resources. Nothing is written outside
//! `OUT_DIR`.

use std::io;

use raumakey_resources::{Artifact, build_script};

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    build_script(Artifact::Tip)?;
    Ok(())
}
