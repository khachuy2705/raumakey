//! Raumakey pure-Rust Windows resource writer used by build scripts.
//!
//! The crate produces 32-bit `.res` files that `link.exe` and `rust-lld` (in `lld-link` mode)
//! accept as linker inputs, so no `rc.exe` is needed. It has no dependencies and no platform
//! code; the same bytes are produced on every host.
//!
//! - [`ResWriter`] writes the `.res` container: `RT_MANIFEST`, `RT_VERSION`,
//!   `RT_ICON`/`RT_GROUP_ICON`, `RT_STRING`, and raw resources.
//! - [`VersionInfo`] builds the `VS_VERSIONINFO` tree; [`version_quad`] parses `x.y.z`.
//! - [`Manifest`] builds the executable manifest (`asInvoker`, `uiAccess="false"`, Common
//!   Controls v6, Per-Monitor V2 DPI awareness, Windows 10/11, UTF-8 code page).
//! - [`IconImage`] renders the original Raumakey icons (app mark, `V`, `E`) at
//!   [`ICON_SIZES`]; [`ico_file`] writes a standalone `.ico`.
//! - [`emit`] writes the complete resources of one [`Artifact`] for a build script.
//!
//! | Artifact | Manifest | Version | Icon groups |
//! |---|---|---|---|
//! | [`Artifact::Controller`] (`raumakey.exe`) | yes | `VFT_APP` | 1 app, 2 `V`, 3 `E` |
//! | [`Artifact::Tip`] (`raumakey_tip.dll`) | no | `VFT_DLL` | 1 app (TSF icon index 0), 2 `V`, 3 `E` |
//! | [`Artifact::TestRunner`] (`raumakey-test-runner.exe`) | yes | `VFT_APP` | 1 app |
//!
//! # Build-script use
//!
//! ```no_run
//! use raumakey_resources::{Artifact, emit, links_res_files};
//!
//! println!("cargo:rerun-if-changed=build.rs");
//! let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
//! let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
//! if links_res_files(&os, &env) {
//!     let out_dir = std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR");
//!     let res = emit(&out_dir, Artifact::Controller).expect("Raumakey resources");
//!     // cargo:rustc-link-arg-bin=raumakey=<OUT_DIR>/raumakey.res
//!     println!("{}", Artifact::Controller.link_directive(&res));
//! }
//! ```
//!
//! [`build_script`] performs the same steps (everything except `rerun-if-changed`);
//! [`build_script_for`] does so with explicit target, output directory, and output stream.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod emit;
mod error;
mod icon;
mod manifest;
mod res;
mod strings;
mod version;

pub use emit::{
    Artifact, ICON_APP, ICON_ENGLISH, ICON_VIETNAMESE, PRODUCT_VERSION, TIP_PROFILE_ICON_INDEX,
    build_script, build_script_for, emit, emit_with_version, links_res_files,
};
pub use error::ResourceError;
pub use icon::{ICON_SIZES, IconGlyph, IconImage, MAX_ICON_SIZE, ico_file, render_icon_set};
pub use manifest::{ExecutionLevel, MANIFEST_ID, Manifest, SUPPORTED_OS_WINDOWS_10};
pub use res::{
    LANG_EN_US, LANG_NEUTRAL, LANG_VIETNAMESE, MEMORY_DISCARDABLE, MEMORY_MOVEABLE, MEMORY_PRELOAD,
    MEMORY_PURE, RT_GROUP_ICON, RT_ICON, RT_MANIFEST, RT_STRING, RT_VERSION, ResWriter, ResourceId,
    VERSION_INFO_ID,
};
pub use strings::{STRINGS_PER_BLOCK, StringTable};
pub use version::{
    CODE_PAGE_UNICODE, COMPANY_NAME, FileType, LEGAL_COPYRIGHT, PRODUCT_NAME, VS_FF_PRERELEASE,
    VersionInfo, version_quad,
};
