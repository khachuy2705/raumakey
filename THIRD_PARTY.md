# Third-party provenance and notices

Raumakey is licensed under GPL-3.0-or-later (see `LICENSE`).

## UniKey

**Not used.** No UniKey source file has been copied, translated, or adapted. The Vietnamese
engine was written from the Raumakey implementation plan
(`docs/superpowers/plans/2026-10-16-raumakey-implementation.md`), the clean-room behavioral
specification, and the compatibility matrix. Behaviors labeled "baseline" in the matrix are
treated as behavioral expectations only.

If UniKey source is ever reused or translated, record here **before** the change lands: repository
URL, immutable commit, license text, affected files, and required notices.

## EVKey

No EVKey binary, resource, icon, string, decompiler output, pseudocode, class layout, or control
flow is used. The reconstruction dossier under `restricted_evidence/` is excluded from Git and was
not read by implementation or review agents. Raumakey uses its own CLSID, profile GUID, upgrade
code, named-object prefix, registry root, icons, and strings.

The optional `evkey-compat` Cargo feature implements the legacy shared-memory layout described in
`docs/reference/evkey_clean_room_spec.md` §6 for interoperability; it is disabled by default.

## Rust crate dependencies

Runtime and build dependencies are fetched from crates.io and locked in `Cargo.lock`.
`cargo run -p xtask -- notices` regenerates the full list with license expressions, and
`cargo run -p xtask -- audit` enforces `deny.toml`.

| Crate | License | Used by |
|---|---|---|
| serde, serde_derive, serde_json | MIT OR Apache-2.0 | documents, IPC payloads |
| toml | MIT OR Apache-2.0 | configuration files |
| unicode-normalization | MIT OR Apache-2.0 | NFC/NFD |
| sha2 | MIT OR Apache-2.0 | checksums |
| crc32fast | MIT OR Apache-2.0 | IPC frame checksum |
| windows, windows-core | MIT OR Apache-2.0 | Win32/COM/TSF bindings |
| object | Apache-2.0 OR MIT | PE inspection in `xtask` (not shipped) |
| proptest, tempfile | MIT OR Apache-2.0 | tests only (not shipped) |

## Build tooling (not shipped)

- Rust toolchain (MIT OR Apache-2.0).
- On Windows hosts without Visual Studio, the MSVC CRT and Windows SDK import libraries are
  obtained with `xwin`; their use is governed by Microsoft's license terms, which the developer
  accepts when downloading them. Release builds link the CRT statically (`+crt-static`, set by
  `xtask`), so packages do not depend on the Visual C++ redistributable.

## Verification oracles

Charset tables in `raumakey-converter` were written from published charset standards (RFC 1456
for VISCII/VIQR, TCVN 5712:1993, Windows-1258, VNI Windows). Independent codec implementations may
be used as test oracles during development; no code or data files are copied from them.
