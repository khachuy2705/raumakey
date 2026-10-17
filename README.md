# Raumakey

Raumakey is a Vietnamese input method for Windows, licensed GPL-3.0-or-later and implemented in Rust with native Win32 and TSF integration. It is under development.

Status on 2026-10-17: the core libraries (model, engine, macros, configuration, policy, IPC, conversion, migration, resources, Windows wrappers), the platform-independent core of the TSF text service, the shared test suites (fuzz smoke, compatibility vectors, test runner), and the `xtask` build tooling are implemented and tested. The controller (`raumakey.exe`), the COM layer of the TSF text service (`raumakey_tip.dll`), and the Windows acceptance harness are not written yet, so Raumakey cannot be installed or used yet. Section 14 of the implementation plan lists what is done and what is outstanding; section 20 of the design has the detailed evidence of the earlier snapshot.

## Target

- Windows 10 and later
- Runtime acceptance: x86_64
- Compile gates: i686, x86_64, and ARM64 MSVC targets
- Native Win32 UI, tray controller, COM TSF TIP, fallback input transport, macros, conversion, configuration, and read-only EVKey migration

## Documents

- Design and implementation status: [`docs/superpowers/specs/2026-10-16-raumakey-design.md`](docs/superpowers/specs/2026-10-16-raumakey-design.md)
- Implementation plan: [`docs/superpowers/plans/2026-10-16-raumakey-implementation.md`](docs/superpowers/plans/2026-10-16-raumakey-implementation.md)
- Clean-room behavioral reference: [`docs/reference/evkey_clean_room_spec.md`](docs/reference/evkey_clean_room_spec.md)
- Compatibility matrix: [`docs/reference/evkey_compatibility_test_matrix.md`](docs/reference/evkey_compatibility_test_matrix.md)

The original reconstruction dossier is retained locally under `restricted_evidence/` and excluded from Git. Implementation agents are prohibited from reading it.
