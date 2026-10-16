# Raumakey repository instructions

## Clean-room boundary

Implementation and review agents MUST NOT read, search, summarize, or use anything under `restricted_evidence/`.

Permitted implementation inputs are:

- `docs/superpowers/specs/`
- `docs/superpowers/plans/`
- `docs/reference/evkey_clean_room_spec.md`
- `docs/reference/evkey_compatibility_test_matrix.md`
- source and tests created inside this repository
- properly licensed upstream UniKey source only after its repository, commit, and license have been recorded in `THIRD_PARTY.md`

Do not copy EVKey decompiler control flow, resources, icons, strings, class layouts, or branding. Raumakey uses new product and TSF identities.

## Development rules

- Use test-driven development: observe a relevant test fail before production code is added.
- Keep the engine and model crates free of Win32, COM, registry, HWND, handles, and raw pointers.
- Do not allow panics to cross COM, Win32 callback, or FFI boundaries.
- Never log user keystrokes, document text, clipboard content, or macro replacement text.
- Format with `cargo fmt`; lint with Clippy using `-D warnings`.
- A successful cross-build is not evidence that Windows runtime behavior works.
