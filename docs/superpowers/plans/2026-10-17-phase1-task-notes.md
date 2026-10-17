# Phase 1 task notes — 2026-10-17

## Scope and process

This file records the failing test runs (plan §0 rule 1) and the independent reviews (plan §13,
design §16) of the work merged on 2026-10-17 after the review follow-up
(`2026-10-17-review-followup.md`). No restricted evidence was used.

Each part was built in its own Git worktree from `24ba7a6`. Two independent reviewers checked it,
one for specification and test adequacy and one for correctness, safety, and code quality. The
implementer then fixed or declined each finding, and a new reviewer re-reviewed the fixes until it
approved them with no blocker or major finding left. Reviewers checked claimed test coverage with
mutations on scratch copies. All tests are host-independent tests on the Windows 11 x86_64
development host. They are not evidence of runtime behavior in Windows host applications (plan §0
rule 6).

| Part | Content | Plan status |
|---|---|---|
| `raumakey-ipc` | `RuntimeSnapshot::validate` rejects a user-defined input method whose key map the snapshot does not carry (commit `d069063`) | §14.4 item 3 |
| `raumakey-windows` | Hidden and message-only windows with the message loop and `PostMessageW`, popup menus, icons, modules, COM server support (class factory, object and lock counts), message boxes, and `GetMessageExtraInfo` | §14.2 stage 5 |
| `raumakey-tsf` | The platform-independent TIP session core `raumakey_tip::session`, tested with fakes | §14.2 stage 6 |

## raumakey-ipc

- Red: `cargo test --locked -p raumakey-ipc --test messages a_user_defined_method` failed
  `a_user_defined_method_needs_its_key_map_in_the_snapshot`: `validate` returned `Ok(())` for a
  user-defined method without key maps.
- Green: the same test, then `cargo xtask ci --no-fuzz` (1,242 and 1,316 tests passed).
- Review: no separate review; the whole-branch review (plan §14.5 step 9) covers it.

## Evidence by part

The lists below are condensed from the implementers' and reviewers' reports. Each red run names
the command and the failing tests; green runs are counted only.

### raumakey-windows

Implementation: 17 red and 10 green runs.

- Red: module: integration tests written before the module `cargo test --locked -p raumakey-windows --test layer module::` failed `layer binary did not compile (module::*)`. E0432 unresolved import `raumakey_windows::module` (intended: module missing)
- Red: module: growing-buffer unit tests against a GetModuleFileNameW-contract fake `cargo test --locked -p raumakey-windows --lib module::` failed `lib test target did not compile`. unresolved imports `super::MAX_PATH_UNITS`, `super::read_growing` (helper missing)
- Red: module: read_growing factored out with a single 260-unit attempt `cargo test --locked -p raumakey-windows --lib module::` failed `module::tests::a_path_longer_than_max_path_is_read_completely`, `module::tests::paths_that_just_fit_or_just_do_not_fit_are_complete`, `module::tests::a_path_that_never_fits_is_an_insufficient_buffer_error`. assertion: paths longer than the buffer came back truncated to 259 units plus NUL;
- Red: window: tests written before the module `cargo test --locked -p raumakey-windows --test layer window::` failed `layer binary did not compile (window::*)`. E0432 unresolved import `raumakey_windows::window`
- Red: window: first increment (no unregister on drop, handler call not separately guarded) `cargo test --locked -p raumakey-windows --test layer window::` failed `window::win::the_class_is_unregistered_after_its_last_window_and_handle_are_dropped`, `window::win::a_panicking_handler_is_contained_and_the_default_procedure_answers`. assertions: class still registered after the last drop;
- Red: menu: tests written before the module `cargo test --locked -p raumakey-windows --test layer menu::` failed `layer binary did not compile (menu::*)`. E0432 unresolved import `raumakey_windows::menu`
- Red: menu: first increment without validation rules `cargo test --locked -p raumakey-windows --test layer menu::` failed `menu::command_ids_must_be_non_zero_and_texts_free_of_nul`, `menu::win::invalid_items_are_rejected_without_changing_the_menu`. assertion (unwrap_err on Ok): command id 0 was accepted
- Red: icon: tests written before the module (first RT_GROUP_ICON of shell32.dll found with EnumResourceNamesW) `cargo test --locked -p raumakey-windows --test layer icon::` failed `layer binary did not compile (icon::*)`. E0432 unresolved import `raumakey_windows::icon`
- Red: icon: first increment, per-DPI variant not yet implemented `cargo test --locked -p raumakey-windows --test layer icon::` failed `icon::win::per_dpi_icon_sizes_scale_with_the_dpi`. assertion (expect): icon_size_for_dpi returned None
- Red: input: GetMessageExtraInfo tests written first `cargo test --locked -p raumakey-windows --test layer input::` failed `layer binary did not compile (input::extra_info::*)`. unresolved imports `current_message_is_raumakey_injected`, `message_extra_info`
- Red: input: message_extra_info implemented, classification placeholder `cargo test --locked -p raumakey-windows --test layer input::` failed `input::extra_info::the_extra_information_of_the_thread_is_read_back_and_classified`. assertion: helper returned false for RAUMAKEY_INJECTED_TAG set with SetMessageExtraInfo
- Red: com_server: tests written before the module (in-process #[implement(IPersist)] test class) `cargo test --locked -p raumakey-windows --test layer com_server::` failed `layer binary did not compile (com_server::*)`. E0432 unresolved import `raumakey_windows::com_server`
- Red: com_server: first increment without aggregation check and LockServer counting `cargo test --locked -p raumakey-windows --test layer com_server::` failed `com_server::win::aggregation_is_rejected`, `com_server::win::lock_server_updates_the_lock_count`. assertions: aggregated CreateInstance succeeded;
- Red: dialog: tests written before the module `cargo test --locked -p raumakey-windows --test layer dialog::` failed `layer binary did not compile (dialog::*)`. E0432 unresolved import `raumakey_windows::dialog`
- Red: dialog: first increment with buttons and icons only `cargo test --locked -p raumakey-windows --test layer dialog::` failed `dialog::message_kinds_map_to_buttons_icons_and_foreground`. assertion: style 64 != 65600 (MB_SETFOREGROUND missing)
- Red: com_server review finding: regression test with the previous fallback `HRESULT(E_UNEXPECTED).into()` temporarily restored `cargo test --locked -p raumakey-windows --test layer com_server::` failed `com_server::win::factory_methods_leave_the_callers_error_object_alone`. assertion (expect at GetErrorInfo): From<HRESULT> for Error calls GetErrorInfo, so every LockServer/CreateInstance consumed the caller's thread error object
- Red: menu review finding: Debug must not show item text `cargo test --locked -p raumakey-windows --test layer menu::` failed `menu::debug_output_never_shows_item_text`. assertion: derived Debug printed `text: "gõ tắt: xin chào"`

First fix round: 17 red and 13 green runs.

- Red: A icon: red, DestroyIcon removed from Drop (temporary mutation) `cargo test --locked -p raumakey-windows --test layer icon::win::each_load` failed `icon::win::each_load_creates_an_icon_of_its_own_that_drop_destroys`. The dropped icon was not destroyed: GetIconInfo succeeded (left None, right Some(0x8007057A)).
- Red: A icon: red, LR_SHARED added to LoadImageW (temporary mutation) `cargo test --locked -p raumakey-windows --test layer icon::win::each_load` failed `icon::win::each_load_creates_an_icon_of_its_own_that_drop_destroys`. Both loads returned the same cached handle HICON(0x694f0be1).
- Red: B module: red, UNCHANGED_REFCOUNT removed (temporary mutation) `cargo test --locked -p raumakey-windows --test layer module::finding` failed `module::finding_the_module_of_an_address_does_not_keep_it_loaded`. "a lookup kept the module loaded": the copied DLL stayed loaded after one FreeLibrary.
- Red: C posting: red, missing items `cargo test --locked -p raumakey-windows --test layer window::` failed `(compile) tests/layer/window.rs`. Unresolved imports REGISTERED_MESSAGE_FIRST, REGISTERED_MESSAGE_LAST, is_postable.
- Red: C posting: red at assertion level after adding only the pure items `cargo test --locked -p raumakey-windows --test layer window::` failed `window::win::window_handles_post_only_private_and_registered_messages`, `window::win::the_message_loop_drops_posted_destroy_messages`, `window::win::a_window_cut_off_from_its_handler_by_a_posted_destroy_is_still_destroyed_on_drop`. post(WM_NULL) was accepted (unwrap_err on Ok);
- Red: D creation: red `cargo test --locked -p raumakey-windows --test layer -- window::win::the_handler_is_installed window::win::a_handler_that_rejects` failed `window::win::a_handler_that_rejects_creation_fails_it_and_is_dropped_once`. A refused creation surfaced as kind Other (0x80004005), not Cancelled.
- Red: D creation: red, handler installed at WM_CREATE (temporary mutation) `cargo test --locked -p raumakey-windows --test layer -- window::win::the_handler_is_installed` failed `window::win::the_handler_is_installed_at_wm_nccreate_and_sees_wm_create_with_the_title`. The first message seen was 1 (WM_CREATE) instead of 0x81 (WM_NCCREATE).
- Red: F foreign parameters: red, window created by name `cargo test --locked -p raumakey-windows --test layer -- window::win::windows_of_the_class_created_by_name_get_no_handler` failed `window::win::windows_of_the_class_created_by_name_get_no_handler`. Test process crashed with exit code 0xc000041d (STATUS_FATAL_USER_CALLBACK_EXCEPTION): install used the foreign create parameters as a PendingState.
- Red: F foreign parameters: red, WM_NCCREATE sent to a live window `cargo test --locked -p raumakey-windows --test layer -- window::win::a_wm_nccreate_sent` failed `window::win::a_wm_nccreate_sent_to_a_live_window_keeps_its_handler`. Same crash (0xc000041d): the live window's state was replaced with the foreign pointer.
- Red: E server lock: red, missing item `cargo test --locked -p raumakey-windows --test layer -- com_server::` failed `(compile) tests/layer/com_server.rs`. No associated function `for_module` on ServerLock.
- Red: E server lock: red with the minimal always-lock implementation `cargo test --locked -p raumakey-windows --test layer -- com_server::` failed `com_server::win::only_code_of_a_dll_takes_a_server_lock`. assertion failed: ServerLock::for_module(executable).unwrap().is_none().
- Red: E server lock: red, WindowClass always takes a lock (temporary mutation) `cargo test --locked -p raumakey-windows --test layer -- window::win::classes_and_windows` failed `window::win::classes_and_windows_of_an_executable_hold_no_server_lock`. lock_count was 1, expected 0.
- Red: G NotifyIcon: red, type-level `cargo test --locked -p raumakey-windows --test layer -- shell::` failed `(compile) shell::win::notify_icons_share_ownership_of_their_icon`. Mismatched types: add and set_icon take HICON, not Rc<Icon>.
- Red: Nit must_use: red `cargo test --locked -p raumakey-windows --doc` failed `com_server::ObjectGuard (compile_fail doctest)`, `com_server::ServerLock (compile_fail doctest)`. "Test compiled successfully, but it's marked compile_fail": a bare new() compiled without #[must_use].
- Red: Nit thread affinity: red, temporary unsafe impl Send/Sync for Window and Send for PopupMenu `cargo test --locked -p raumakey-windows --doc` failed `window::win::Window (compile_fail, line 307)`, `window::win::Window (compile_fail, line 314)`, `menu::win::PopupMenu (compile_fail, line 167)`. The snippets compiled once the types were Send or Sync.
- Red: Nit dialog: red, missing item `cargo test --locked -p raumakey-windows --test layer -- dialog::` failed `(compile) tests/layer/dialog.rs`. Unresolved import raumakey_windows::dialog::message_box_texts.
- Red: Nit read_growing: red, missing public item `cargo test --locked -p raumakey-windows --test layer -- strings::` failed `(compile) tests/layer/strings.rs`. Unresolved import raumakey_windows::strings::read_growing (it was private in module.rs).

Second fix round: 6 red and 4 green runs.

- Red: Finding 1: unit tests written first, calling the not-yet-existing WindowClass::register_with to hand a DLL's ServerLock to a class of the test executable `cargo test --locked -p raumakey-windows --lib` failed `window::win::tests::a_dll_class_keeps_its_lock_until_its_last_window_is_gone_and_it_is_unregistered`, `window::win::tests::a_dll_class_that_cannot_be_unregistered_keeps_its_lock`. Intended missing item: error[E0599] no associated function `register_with` for `WindowClass` (window.rs:752).
- Red: Finding 1: reviewer mutation 'always forget' (`if !unregistered {` -> `if true {`) applied by a script that restores the file and checks its sha256 `cargo test --locked -p raumakey-windows --lib -- window::` failed `window::win::tests::a_dll_class_keeps_its_lock_until_its_last_window_is_gone_and_it_is_unregistered`. Assertion 'and its lock released' at window.rs:797: left 1, right 0.
- Red: Finding 1: reviewer mutation 'always release' (`core::mem::forget(lock);` -> `drop(lock);`) `cargo test --locked -p raumakey-windows --lib -- window::` failed `window::win::tests::a_dll_class_that_cannot_be_unregistered_keeps_its_lock`. Assertion 'and the class keeps its lock' at window.rs:835: left 0, right 1.
- Red: Finding 2: mutation removes `|| read_slot(hwnd) != 0` from install (empty-slot check) `cargo test --locked -p raumakey-windows --test layer -- window::` failed `window::win::a_wm_nccreate_forwarded_to_a_live_window_during_a_creation_leaves_both_handlers`. Assertion 'the live window kept its handler' at tests/layer/window.rs:407: left 8, right 7.
- Red: Finding 2: mutation `creating.set(self.previous)` -> `creating.set(core::ptr::null())` in Creating::drop `cargo test --locked -p raumakey-windows --test layer -- window::` failed `window::win::a_window_created_while_another_is_being_created_leaves_both_handlers`. Assertion 'the window being created still got its handler' at tests/layer/window.rs:444: left 0, right 10.
- Red: Final re-run of all four mutations on the finished code (after the doc-only edits), unit and layer tests together `bash scratchpad/phase1-followup/mutate-window.sh <mutation> crates/raumakey-windows/src/window.rs ... -p raumakey-windows --lib --test layer -- window::` failed `window::win::tests::a_dll_class_keeps_its_lock_until_its_last_window_is_gone_and_it_is_unregistered`, `window::win::tests::a_dll_class_that_cannot_be_unregistered_keeps_its_lock`, `window::win::a_wm_nccreate_forwarded_to_a_live_window_during_a_creation_leaves_both_handlers`, `window::win::a_window_created_while_another_is_being_created_leaves_both_handlers`. Each mutation fails exactly its intended test, one each in the order always-forget, always-release, no-empty-slot-check, mark-not-restored.

Reviews:

- Specification and test adequacy: not approved (2 major, 3 minor, 5 nit).
- Correctness, safety, and code quality: not approved (1 major, 3 minor, 3 nit).
- Re-review after the first fix round: approved (1 minor, 1 nit).
- Re-review after the second fix round: approved (1 minor).

Open minor findings from the last re-review:

- minor, `crates/raumakey-windows/src/window.rs:262`: Finding 1 is fixed for the release and forget half of a DLL class's server lock, but the acquisition half is still untested.

### raumakey-tsf

Implementation: 16 red and 11 green runs.

- Red: Batch 1 (typing #1 #3 #15 #17 #40, apply failures #41, defaults v1): tests written before any session code `cargo test --locked -p raumakey-tsf` failed `session test target (compile error)`. E0432 unresolved import raumakey_tip::session: the API did not exist yet
- Red: Batch 1: API skeleton with stubbed behaviour (test_key_down=false, key_down=pass-through) `cargo test --locked -p raumakey-tsf` failed `typing::matrix_1_telex_a_a_gives_a_circumflex_in_one_edit`, `typing::matrix_3_telex_a_s_gives_a_acute`, `typing::a_whole_word_is_typed_with_one_edit_per_transformation`, `typing::matrix_15_an_invalid_converted_word_is_restored_at_the_boundary`, `typing::a_space_after_a_valid_word_reaches_the_application`, `typing::matrix_17_semantic_backspace_restores_the_previous_state`, `typing::matrix_40_two_units_are_replaced_by_one_in_a_single_edit`, `typing::navigation_ends_the_word_and_reaches_the_application`, and 6 more. 14 failed on their claim/edit assertions;
- Red: Batch 2 (context: #19 selection change, #20 document switches, close, bound, engine error) `cargo test --locked -p raumakey-tsf` failed `session test target (compile error)`. MAX_DOCUMENTS, set_focus, document_closed, document_count missing
- Red: Batch 2: no-op stubs for the new API `cargo test --locked -p raumakey-tsf` failed `context::matrix_19_a_selection_change_by_the_user_resets_the_word`, `context::matrix_19_a_reported_change_resets_even_when_the_caret_ends_where_it_was`, `context::only_the_first_change_after_a_passed_key_is_its_echo`, `context::matrix_20_a_focus_change_mid_word_never_deletes_across_documents`, `context::matrix_20_a_key_from_another_document_is_a_document_change`, `context::a_focus_change_away_from_every_document_ends_the_word`, `context::closing_a_document_drops_its_state`, `context::document_states_are_bounded`, and 1 more. 9 failed as intended;
- Red: Batch 3 (gates: #39 injected, non-TSF transport, #29 secure field) `cargo test --locked -p raumakey-tsf` failed `gates::hosts_without_tsf_range_edits_are_left_to_the_controller`, `gates::matrix_39_an_injected_key_from_another_document_changes_nothing`. no transport gate;
- Red: Batch 4 (snapshots, #16, macro + Enter replay, invalid snapshots, mode, reset-all, host rules) `cargo test --locked -p raumakey-tsf` failed `session test target (compile error)`. SnapshotError, apply_runtime_snapshot, apply_macro_snapshot, set_vietnamese_mode, reset_all missing
- Red: Batch 4: stubs returning Ok/no-op `cargo test --locked -p raumakey-tsf` failed `config::a_runtime_snapshot_replaces_the_configuration`, `config::matrix_16_without_auto_restore_the_rendered_word_is_committed`, `config::an_invalid_runtime_snapshot_keeps_the_last_good_configuration`, `config::a_new_configuration_ends_the_word_in_progress`, `config::macros_expand_at_a_boundary`, `config::enter_after_a_macro_is_replayed_with_the_injected_tag`, `config::an_invalid_macro_snapshot_keeps_the_last_good_table`, `config::a_macro_snapshot_over_the_entry_limit_is_rejected`, and 4 more. 12 failed as intended;
- Red: Batch 4: config installation, validation, mode, reset-all `cargo test --locked -p raumakey-tsf` failed `config::a_runtime_snapshot_replaces_the_configuration`. test bug: VNI 'a6' + space is restored because 'â' alone is not a complete syllable;
- Red: Batch 5 (D24 hotkeys) `cargo test --locked -p raumakey-tsf` failed `session test target (compile error)`. set_connected, set_altgr_layout missing
- Red: Batch 5: stubs `cargo test --locked -p raumakey-tsf` failed `hotkeys::d24_disconnected_ctrl_shift_toggles_the_local_mode`, `hotkeys::d24_the_toggle_ends_the_word_in_progress`, `hotkeys::d24_disconnected_ctrl_shift_alt_f12_hard_resets_and_is_eaten`, `hotkeys::d24_ctrl_shift_alt_space_is_the_other_hard_reset`, `hotkeys::d24_the_auto_repeat_of_a_hard_reset_key_is_swallowed`, `hotkeys::d24_losing_the_connection_hands_classification_back`, `hotkeys::d24_hotkeys_of_the_snapshot_replace_the_defaults`, `hotkeys::d24_altgr_types_characters_on_altgr_layouts`, and 2 more. 10 failed as intended;
- Red: Mutations after the new test (generation / revision / full-key / verdict-memory ignored) `cargo test --locked -q -p raumakey-tsf --test session (each mutation restored)` failed `cache::a_reset_between_the_test_of_a_passed_letter_and_key_down_starts_a_new_word`, `cache::a_macro_snapshot_between_test_and_key_down_is_honoured`, `cache::the_test_of_the_same_key_in_another_host_state_is_not_used`, `hotkeys::d24_disconnected_ctrl_shift_alt_f12_hard_resets_and_is_eaten (and 9 other hotkey tests)`. each mutated rule is caught;
- Red: Batch 7 privacy tests `cargo test --locked -p raumakey-tsf` failed `session test target (compile error)`. E0277 ServiceCore does not implement Debug
- Red: Batch 7 with temporary derived (leaky) Debug `cargo test --locked -p raumakey-tsf` failed `privacy::key_strokes_hide_their_key_and_text`, `privacy::edit_requests_hide_document_text`, `privacy::key_down_outcomes_hide_the_replayed_key`, `privacy::the_core_hides_words_documents_and_macros`. derived Debug printed key codes, UTF-16 text and owned words
- Red: Batch 7 with redacted Debug implementations `cargo test --locked -p raumakey-tsf` failed `privacy::edit_requests_hide_document_text`, `privacy::the_core_hides_words_documents_and_macros`. test bugs: typed precomposed 'ư' instead of Telex 'uw';
- Red: Batch 7 green, then mutation (leaky EditRequest and core Debug) against the corrected tests `cargo test --locked -q -p raumakey-tsf --test session privacy` failed `privacy::edit_requests_hide_document_text`, `privacy::the_core_hides_words_documents_and_macros`. the corrected tests still catch leaks;
- Red: Overflow found in review: the app's echo of keys typed past the context limit reset the engine's overflow state `cargo test --locked -p raumakey-tsf` failed `typing::a_word_over_the_context_limit_passes_through_until_the_next_boundary`. left '…xâ', right '…xaa': the word restarted mid-token

First fix round: 14 red and 11 green runs.

- Red: Mutant (a): reset after a failed apply removed (service.rs backed up and restored) `cargo test --locked -p raumakey-tsf --test session` failed `failures::matrix_41_the_key_after_a_failed_edit_starts_a_new_word`. The stale word "a" survives, so the next 'a' edits (attempts 2).
- Red: Mutant (b): commit next instead of reset after a failed apply `cargo test --locked -p raumakey-tsf --test session` failed `failures::matrix_41_the_key_after_a_failed_edit_starts_a_new_word`. The rejected "â" is committed, and the next 'a' attempts a second edit.
- Red: Mutant (c): no reset in the Decision::Reset arm `cargo test --locked -p raumakey-tsf --test session` failed `context::an_engine_error_ends_the_word_before_any_report_arrives`. After the engine error, the word "a" survives and 's' edits it.
- Red: Findings 1/7: reviewer's #19 scenario written against the old API `cargo test --locked -p raumakey-tsf --test session -- matrix_19_a_click_before_the_passed_key_is_reported_ends_the_word` failed `context::matrix_19_a_click_before_the_passed_key_is_reported_ends_the_word`. left "bá ba", right "bas ba": the click was taken as the echo, and the tone edited the word before the click.
- Red: Findings 1/7: tests and fake moved to selection_changed(document, SelectionChange) `cargo test --locked -p raumakey-tsf --test session --no-run` failed `(compile) 13 errors: E0432 unresolved import SelectionChange; E0061 selection_changed takes 1 argument`. The evidence-based API does not exist yet.
- Red: Mutant: Arrival::is always true (the old behaviour: any first change is the echo) `cargo test --locked -p raumakey-tsf --test session` failed `context::matrix_19_a_click_before_the_passed_key_is_reported_ends_the_word`, `context::matrix_19_a_report_of_other_text_than_the_passed_key_ends_the_word`. Shows that the new tests depend on comparing the evidence.
- Red: Findings 3/8: Ctrl+Shift with a reported foreign change `cargo test --locked -p raumakey-tsf --test session -- d24_a_click_reported_while_ctrl_shift_is_held_cancels_the_toggle` failed `hotkeys::d24_a_click_reported_while_ctrl_shift_is_held_cancels_the_toggle`. assertion failed: !claimed.
- Red: Findings 3/8: mouse tests written `cargo test --locked -p raumakey-tsf --test session --no-run` failed `(compile) E0599 no method named mouse_down (4 errors)`. The mouse entry point does not exist yet.
- Red: Finding 8 (probes): tests written `cargo test --locked -p raumakey-tsf --test session --no-run` failed `(compile) E0599 no method named needs_probe / record_probe (8 errors)`. The probe pass-throughs do not exist yet.
- Red: Finding 4 mutants: set_focus keeps the classifier / enter_document keeps it / key-downs keep the expected arrival `cargo test --locked -p raumakey-tsf --test session (once per mutant, service.rs restored after each)` failed `hotkeys::d24_a_focus_change_mid_chord_never_fires`, `hotkeys::d24_a_document_change_mid_chord_never_fires`, `context::an_arrival_reported_after_the_next_key_down_ends_the_word`. Each mutant fails exactly its own test (110 passed, 1 failed in each run).
- Red: Finding 9: user method without its key map `cargo test --locked -p raumakey-tsf --test session user_` failed `config::a_user_method_without_its_key_map_keeps_the_last_good_configuration`. left Ok(()), right Err(Runtime(Malformed(InvalidKeymap))): the snapshot was accepted.
- Red: Finding 10: build_macro_table/install_macro_table tests `cargo test --locked -p raumakey-tsf --test session --no-run` failed `(compile) E0432 unresolved import build_macro_table; E0599 no method named install_macro_table`. The API does not exist yet.
- Red: Finding 14 (nit): lock-key test, then the mutant with the lock-key branch removed `cargo test --locked -p raumakey-tsf --test session` failed `typing::lock_keys_do_not_end_the_word`. With the mutant, Caps Lock becomes a navigation key that is claimed and ends the word.
- Red: Privacy of the new SelectionChange: test, then the derived-Debug mutant `cargo test --locked -p raumakey-tsf --test session privacy` failed `privacy::reported_changes_hide_the_inserted_text`. A derived Debug prints the inserted UTF-16 units.

Second fix round: 7 red and 1 green runs.

- Red: Red: new probe, secure-field and mouse tests (first version) run against the unchanged code `cargo test --locked -p raumakey-tsf --test session -- matrix_27 matrix_29_a_secure_field_is_never_probed matrix_19_a_mouse_press_after` failed `gates::matrix_27_a_snapshot_that_carries_a_working_probe_overrides_a_failed_local_one`, `gates::matrix_27_a_snapshot_that_carries_a_failed_probe_overrides_a_working_local_one`, `gates::matrix_27_a_host_is_probed_again_once_a_snapshot_drops_the_record_it_carried`, `gates::matrix_29_a_secure_field_is_never_probed`, `gates::matrix_27_a_context_without_a_document_is_never_probed`. These failed for the intended reasons.
- Red: Red: the newer-version iteration of the probed-again test on its own (loop order swapped temporarily, then restored) `cargo test --locked -p raumakey-tsf --test session -- matrix_27_a_host_is_probed_again` failed `gates::matrix_27_a_host_is_probed_again_once_a_snapshot_drops_the_record_it_carried`. The old code kept the local result after a snapshot carried a newer-version record and re-applied it later, so needs_probe stayed false.
- Red: Red: the reviewer's mutant m03 (the Mouse word reset in mouse_down replaced by `let _ = document;`), applied to a backup and restored afterwards (checked with… `cargo test --locked -p raumakey-tsf --test session -- matrix_19 mouse` failed `context::matrix_19_a_mouse_press_after_the_last_report_ends_the_word`. The result was "bá ba" instead of "bas ba": the tone key edited the text in front of an unreported click, which is the matrix #19 violation.
- Red: First run after implementing the fix `cargo test --locked -p raumakey-tsf` failed `gates::matrix_27_a_snapshot_that_carries_a_working_probe_overrides_a_failed_local_one`, `gates::matrix_27_a_host_is_probed_again_once_a_snapshot_drops_the_record_it_carried`. The tests expected the wrong text;
- Red: Red again: the final test versions run against the original service.rs (swapped in from a backup, then the fix restored) `cargo test --locked -p raumakey-tsf --test session -- matrix_27 matrix_29_a_secure_field_is_never_probed` failed `gates::matrix_27_a_snapshot_that_carries_a_working_probe_overrides_a_failed_local_one`, `gates::matrix_27_a_snapshot_that_carries_a_failed_probe_overrides_a_working_local_one`, `gates::matrix_27_a_host_is_probed_again_once_a_snapshot_drops_the_record_it_carried`, `gates::matrix_29_a_secure_field_is_never_probed`, `gates::matrix_27_a_context_without_a_document_is_never_probed`. Same reasons as the first red run.
- Red: Mutation check M1: drop a local result when the snapshot carries any record for the executable, even an older-version one (applied, then restored) `cargo test --locked -p raumakey-tsf --test session -- matrix_27` failed `gates::matrix_27_a_record_of_an_older_probe_version_leaves_the_local_result_in_effect`. The guard test pins this boundary of the retention rule.
- Red: Mutation check M2: keep local results forever and apply them only while no snapshot record covers the executable (applied, then restored) `cargo test --locked -p raumakey-tsf --test session -- matrix_27` failed `gates::matrix_27_a_host_is_probed_again_once_a_snapshot_drops_the_record_it_carried`. The probed-again test pins dropping the local result for good.

Reviews:

- Specification and test adequacy: not approved (3 major, 2 minor, 1 nit).
- Correctness, safety, and code quality: not approved (1 major, 4 minor, 3 nit).
- Re-review after the first fix round: not approved (2 major, 1 minor).
- Re-review after the second fix round: approved (2 minor).

Open minor findings from the last re-review:

- minor, `crates/raumakey-tsf/tests/session/gates.rs:125`: No test covers the documented rule that a local probe result is applied again to every later snapshot that lacks a record (service.rs:530-531, mod.rs:112-117).
- minor, `crates/raumakey-tsf/src/session/mod.rs:167`: The COM-layer contract (mod.rs:167-170, record_probe docs at service.rs:524-540) only says to report each recorded result with Command::ReportProbe.
## Verification of the merged revision

Run on the main checkout after merging `raumakey-windows` and `raumakey-tsf` on top of
`d069063` (plan §14.6):

- `cargo fmt --all --check`: pass.
- `cargo xtask ci --no-fuzz`: Clippy with `-D warnings` passed with default features and with all
  five `evkey-compat` features; 1,442 tests passed with default features and 1,515 with the
  compatibility features, 0 failed, 10 manual tests ignored.
- `cargo test --locked -p raumakey-windows -p raumakey-tsf --target i686-pc-windows-msvc`: 346
  passed under WOW64, 0 failed, 10 manual tests ignored.
- `cargo xtask cross`: debug builds for x86_64, i686, and aarch64 passed.

The `raumakey-tests` work (fuzz targets, `fuzz_smoke`, macro/engine suite, compatibility vectors,
and the runner's `vectors` mode) is still under review and is not part of this revision.
