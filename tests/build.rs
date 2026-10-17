//! Build script of `raumakey-tests`: the Windows resources of `raumakey-test-runner.exe`.
//!
//! On Windows MSVC targets, `raumakey_resources::build_script` writes
//! `raumakey-test-runner.res` (manifest, version information, application icon) into `OUT_DIR`
//! and prints `cargo:rustc-link-arg-bin=raumakey-test-runner=<path>`, so only the
//! `raumakey-test-runner` bin target links it, including that target's own unit-test harness:
//! not the library, the integration test binaries, or other packages. Other targets get no
//! resources. Nothing is written outside `OUT_DIR`.

use std::io;

use raumakey_resources::{Artifact, build_script};

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    build_script(Artifact::TestRunner)?;
    Ok(())
}
