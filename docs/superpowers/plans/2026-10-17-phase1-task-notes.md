# Ghi chép task Giai đoạn 1 — 2026-10-17

## Phạm vi và quy trình

Tệp này ghi lại các lần chạy test fail (kế hoạch §0 quy tắc 1) và các review độc lập (kế hoạch
§13, thiết kế §16) của phần việc được merge ngày 2026-10-17 sau đợt khắc phục theo review
(`2026-10-17-review-followup.md`). Không sử dụng bất kỳ bằng chứng hạn chế (restricted evidence)
nào.

Mỗi phần được phát triển trong Git worktree riêng, tách từ `24ba7a6`. Hai reviewer độc lập kiểm
tra từng phần: một người về đặc tả và mức đầy đủ của test, một người về tính đúng đắn, độ an toàn
và chất lượng mã. Sau đó người triển khai sửa hoặc từ chối từng phát hiện, và một reviewer mới
review lại các bản sửa cho đến khi reviewer này duyệt mà không còn phát hiện nào ở mức blocker
hoặc major. Các reviewer kiểm tra độ bao phủ test được khẳng định bằng các đột biến (mutation) trên
bản sao nháp. Mọi test đều là test không phụ thuộc host, chạy trên host phát triển Windows 11
x86_64. Chúng không phải là bằng chứng về hành vi runtime trong các ứng dụng chủ Windows (kế hoạch
§0 quy tắc 6).

| Phần | Nội dung | Trạng thái trong kế hoạch |
|---|---|---|
| `raumakey-ipc` | `RuntimeSnapshot::validate` từ chối một kiểu gõ tự định nghĩa mà snapshot không mang theo bảng phím của nó (commit `d069063`) | §14.4 mục 3 |
| `raumakey-windows` | Cửa sổ ẩn và cửa sổ message-only cùng vòng lặp thông điệp và `PostMessageW`, popup menu, icon, module, hỗ trợ COM server (class factory, bộ đếm object và bộ đếm khoá), hộp thông báo, và `GetMessageExtraInfo` | §14.2 giai đoạn 5 |
| `raumakey-tsf` | Lõi session TIP không phụ thuộc nền tảng `raumakey_tip::session`, được test bằng các fake (đối tượng giả) | §14.2 giai đoạn 6 |
| `raumakey-tests` | Tám fuzz target do `fuzz_smoke` chạy, bộ test macro/engine dùng `MacroTable` thật, 202 vector tương thích, và `raumakey-test-runner vectors [--json]` | §14.2 giai đoạn 7 |

## raumakey-ipc

- Đỏ: `cargo test --locked -p raumakey-ipc --test messages a_user_defined_method` fail ở
  `a_user_defined_method_needs_its_key_map_in_the_snapshot`: `validate` trả về `Ok(())` cho một
  kiểu gõ tự định nghĩa không có bảng phím.
- Xanh: cùng test đó, sau đó `cargo xtask ci --no-fuzz` (1,242 và 1,316 test pass).
- Review: không có review riêng; review toàn bộ branch (kế hoạch §14.5 bước 9) bao gồm phần này.

## Bằng chứng theo từng phần

Các danh sách dưới đây được rút gọn từ báo cáo của người triển khai và của reviewer. Mỗi lần chạy
đỏ ghi rõ lệnh và các test fail; các lần chạy xanh chỉ được đếm số lượng.

### raumakey-windows

Triển khai: 17 lần chạy đỏ và 10 lần chạy xanh.

- Đỏ: module: test tích hợp được viết trước module `cargo test --locked -p raumakey-windows --test layer module::` fail ở `layer binary did not compile (module::*)`. E0432 unresolved import `raumakey_windows::module` (cố ý: module chưa có)
- Đỏ: module: test đơn vị cho buffer tăng dần, chạy với một fake theo hợp đồng của GetModuleFileNameW `cargo test --locked -p raumakey-windows --lib module::` fail ở `lib test target did not compile`. unresolved imports `super::MAX_PATH_UNITS`, `super::read_growing` (chưa có hàm trợ giúp)
- Đỏ: module: tách read_growing thành hàm riêng với một lần thử duy nhất 260 đơn vị `cargo test --locked -p raumakey-windows --lib module::` fail ở `module::tests::a_path_longer_than_max_path_is_read_completely`, `module::tests::paths_that_just_fit_or_just_do_not_fit_are_complete`, `module::tests::a_path_that_never_fits_is_an_insufficient_buffer_error`. assertion: các đường dẫn dài hơn buffer bị trả về ở dạng cắt ngắn còn 259 đơn vị cộng NUL;
- Đỏ: window: test được viết trước module `cargo test --locked -p raumakey-windows --test layer window::` fail ở `layer binary did not compile (window::*)`. E0432 unresolved import `raumakey_windows::window`
- Đỏ: window: bước tăng dần đầu tiên (chưa hủy đăng ký khi drop, lời gọi handler chưa được bảo vệ riêng) `cargo test --locked -p raumakey-windows --test layer window::` fail ở `window::win::the_class_is_unregistered_after_its_last_window_and_handle_are_dropped`, `window::win::a_panicking_handler_is_contained_and_the_default_procedure_answers`. assertion: class vẫn còn được đăng ký sau lần drop cuối cùng;
- Đỏ: menu: test được viết trước module `cargo test --locked -p raumakey-windows --test layer menu::` fail ở `layer binary did not compile (menu::*)`. E0432 unresolved import `raumakey_windows::menu`
- Đỏ: menu: bước tăng dần đầu tiên chưa có quy tắc kiểm tra hợp lệ `cargo test --locked -p raumakey-windows --test layer menu::` fail ở `menu::command_ids_must_be_non_zero_and_texts_free_of_nul`, `menu::win::invalid_items_are_rejected_without_changing_the_menu`. assertion (unwrap_err on Ok): command id 0 đã được chấp nhận
- Đỏ: icon: test được viết trước module (RT_GROUP_ICON đầu tiên của shell32.dll, tìm bằng EnumResourceNamesW) `cargo test --locked -p raumakey-windows --test layer icon::` fail ở `layer binary did not compile (icon::*)`. E0432 unresolved import `raumakey_windows::icon`
- Đỏ: icon: bước tăng dần đầu tiên, chưa triển khai biến thể theo DPI `cargo test --locked -p raumakey-windows --test layer icon::` fail ở `icon::win::per_dpi_icon_sizes_scale_with_the_dpi`. assertion (expect): icon_size_for_dpi trả về None
- Đỏ: input: test GetMessageExtraInfo được viết trước `cargo test --locked -p raumakey-windows --test layer input::` fail ở `layer binary did not compile (input::extra_info::*)`. unresolved imports `current_message_is_raumakey_injected`, `message_extra_info`
- Đỏ: input: đã triển khai message_extra_info, phần phân loại vẫn là mã giữ chỗ `cargo test --locked -p raumakey-windows --test layer input::` fail ở `input::extra_info::the_extra_information_of_the_thread_is_read_back_and_classified`. assertion: hàm trợ giúp trả về false với RAUMAKEY_INJECTED_TAG được đặt bằng SetMessageExtraInfo
- Đỏ: com_server: test được viết trước module (class test in-process #[implement(IPersist)]) `cargo test --locked -p raumakey-windows --test layer com_server::` fail ở `layer binary did not compile (com_server::*)`. E0432 unresolved import `raumakey_windows::com_server`
- Đỏ: com_server: bước tăng dần đầu tiên chưa kiểm tra aggregation và chưa đếm LockServer `cargo test --locked -p raumakey-windows --test layer com_server::` fail ở `com_server::win::aggregation_is_rejected`, `com_server::win::lock_server_updates_the_lock_count`. assertion: CreateInstance có aggregation vẫn thành công;
- Đỏ: dialog: test được viết trước module `cargo test --locked -p raumakey-windows --test layer dialog::` fail ở `layer binary did not compile (dialog::*)`. E0432 unresolved import `raumakey_windows::dialog`
- Đỏ: dialog: bước tăng dần đầu tiên chỉ có nút và icon `cargo test --locked -p raumakey-windows --test layer dialog::` fail ở `dialog::message_kinds_map_to_buttons_icons_and_foreground`. assertion: style 64 != 65600 (thiếu MB_SETFOREGROUND)
- Đỏ: com_server, phát hiện review: test hồi quy với đường dự phòng (fallback) cũ `HRESULT(E_UNEXPECTED).into()` được khôi phục tạm thời `cargo test --locked -p raumakey-windows --test layer com_server::` fail ở `com_server::win::factory_methods_leave_the_callers_error_object_alone`. assertion (expect tại GetErrorInfo): From<HRESULT> for Error gọi GetErrorInfo, nên mọi lời gọi LockServer/CreateInstance đều tiêu thụ error object của luồng thuộc bên gọi
- Đỏ: menu, phát hiện review: Debug không được hiển thị văn bản của mục `cargo test --locked -p raumakey-windows --test layer menu::` fail ở `menu::debug_output_never_shows_item_text`. assertion: Debug được derive đã in ra `text: "gõ tắt: xin chào"`

Vòng sửa thứ nhất: 17 lần chạy đỏ và 13 lần chạy xanh.

- Đỏ: A icon: đỏ, bỏ DestroyIcon khỏi Drop (đột biến tạm thời) `cargo test --locked -p raumakey-windows --test layer icon::win::each_load` fail ở `icon::win::each_load_creates_an_icon_of_its_own_that_drop_destroys`. Icon đã drop không bị hủy: GetIconInfo thành công (left None, right Some(0x8007057A)).
- Đỏ: A icon: đỏ, thêm LR_SHARED vào LoadImageW (đột biến tạm thời) `cargo test --locked -p raumakey-windows --test layer icon::win::each_load` fail ở `icon::win::each_load_creates_an_icon_of_its_own_that_drop_destroys`. Cả hai lần nạp đều trả về cùng một handle đã được cache HICON(0x694f0be1).
- Đỏ: B module: đỏ, bỏ UNCHANGED_REFCOUNT (đột biến tạm thời) `cargo test --locked -p raumakey-windows --test layer module::finding` fail ở `module::finding_the_module_of_an_address_does_not_keep_it_loaded`. "a lookup kept the module loaded": DLL được sao chép vẫn còn được nạp sau một lần FreeLibrary.
- Đỏ: C gửi thông điệp (posting): đỏ, thiếu item `cargo test --locked -p raumakey-windows --test layer window::` fail ở `(compile) tests/layer/window.rs`. Unresolved imports REGISTERED_MESSAGE_FIRST, REGISTERED_MESSAGE_LAST, is_postable.
- Đỏ: C gửi thông điệp: đỏ ở mức assertion sau khi chỉ thêm các item thuần (pure) `cargo test --locked -p raumakey-windows --test layer window::` fail ở `window::win::window_handles_post_only_private_and_registered_messages`, `window::win::the_message_loop_drops_posted_destroy_messages`, `window::win::a_window_cut_off_from_its_handler_by_a_posted_destroy_is_still_destroyed_on_drop`. post(WM_NULL) đã được chấp nhận (unwrap_err on Ok);
- Đỏ: D tạo cửa sổ: đỏ `cargo test --locked -p raumakey-windows --test layer -- window::win::the_handler_is_installed window::win::a_handler_that_rejects` fail ở `window::win::a_handler_that_rejects_creation_fails_it_and_is_dropped_once`. Một lần tạo cửa sổ bị từ chối lộ ra dưới dạng kind Other (0x80004005), không phải Cancelled.
- Đỏ: D tạo cửa sổ: đỏ, handler được cài tại WM_CREATE (đột biến tạm thời) `cargo test --locked -p raumakey-windows --test layer -- window::win::the_handler_is_installed` fail ở `window::win::the_handler_is_installed_at_wm_nccreate_and_sees_wm_create_with_the_title`. Thông điệp đầu tiên nhận được là 1 (WM_CREATE) thay vì 0x81 (WM_NCCREATE).
- Đỏ: F tham số ngoại lai: đỏ, cửa sổ được tạo theo tên `cargo test --locked -p raumakey-windows --test layer -- window::win::windows_of_the_class_created_by_name_get_no_handler` fail ở `window::win::windows_of_the_class_created_by_name_get_no_handler`. Tiến trình test bị sập với mã thoát 0xc000041d (STATUS_FATAL_USER_CALLBACK_EXCEPTION): install đã dùng tham số tạo cửa sổ ngoại lai như một PendingState.
- Đỏ: F tham số ngoại lai: đỏ, WM_NCCREATE được gửi tới một cửa sổ đang tồn tại `cargo test --locked -p raumakey-windows --test layer -- window::win::a_wm_nccreate_sent` fail ở `window::win::a_wm_nccreate_sent_to_a_live_window_keeps_its_handler`. Cùng kiểu sập đó (0xc000041d): trạng thái của cửa sổ đang tồn tại bị thay bằng con trỏ ngoại lai.
- Đỏ: E khoá server: đỏ, thiếu item `cargo test --locked -p raumakey-windows --test layer -- com_server::` fail ở `(compile) tests/layer/com_server.rs`. Không có associated function `for_module` trên ServerLock.
- Đỏ: E khoá server: đỏ với bản triển khai tối thiểu luôn lấy khoá `cargo test --locked -p raumakey-windows --test layer -- com_server::` fail ở `com_server::win::only_code_of_a_dll_takes_a_server_lock`. assertion failed: ServerLock::for_module(executable).unwrap().is_none().
- Đỏ: E khoá server: đỏ, WindowClass luôn lấy một khoá (đột biến tạm thời) `cargo test --locked -p raumakey-windows --test layer -- window::win::classes_and_windows` fail ở `window::win::classes_and_windows_of_an_executable_hold_no_server_lock`. lock_count là 1, kỳ vọng là 0.
- Đỏ: G NotifyIcon: đỏ, ở mức kiểu `cargo test --locked -p raumakey-windows --test layer -- shell::` fail ở `(compile) shell::win::notify_icons_share_ownership_of_their_icon`. Mismatched types: add và set_icon nhận HICON, không phải Rc<Icon>.
- Đỏ: Nit must_use: đỏ `cargo test --locked -p raumakey-windows --doc` fail ở `com_server::ObjectGuard (compile_fail doctest)`, `com_server::ServerLock (compile_fail doctest)`. "Test compiled successfully, but it's marked compile_fail": một lời gọi new() trần vẫn biên dịch được khi không có #[must_use].
- Đỏ: Nit gắn với luồng (thread affinity): đỏ, tạm thời thêm unsafe impl Send/Sync cho Window và Send cho PopupMenu `cargo test --locked -p raumakey-windows --doc` fail ở `window::win::Window (compile_fail, line 307)`, `window::win::Window (compile_fail, line 314)`, `menu::win::PopupMenu (compile_fail, line 167)`. Các đoạn mã biên dịch được ngay khi các kiểu này là Send hoặc Sync.
- Đỏ: Nit dialog: đỏ, thiếu item `cargo test --locked -p raumakey-windows --test layer -- dialog::` fail ở `(compile) tests/layer/dialog.rs`. Unresolved import raumakey_windows::dialog::message_box_texts.
- Đỏ: Nit read_growing: đỏ, thiếu item public `cargo test --locked -p raumakey-windows --test layer -- strings::` fail ở `(compile) tests/layer/strings.rs`. Unresolved import raumakey_windows::strings::read_growing (trước đó nó là private trong module.rs).

Vòng sửa thứ hai: 6 lần chạy đỏ và 4 lần chạy xanh.

- Đỏ: Phát hiện 1: test đơn vị được viết trước, gọi WindowClass::register_with khi hàm này chưa tồn tại để trao ServerLock của một DLL cho một class của tệp thực thi test `cargo test --locked -p raumakey-windows --lib` fail ở `window::win::tests::a_dll_class_keeps_its_lock_until_its_last_window_is_gone_and_it_is_unregistered`, `window::win::tests::a_dll_class_that_cannot_be_unregistered_keeps_its_lock`. Item thiếu đúng như dự định: error[E0599] no associated function `register_with` for `WindowClass` (window.rs:752).
- Đỏ: Phát hiện 1: đột biến 'always forget' của reviewer (`if !unregistered {` -> `if true {`), được áp dụng bằng một script có khôi phục tệp và kiểm tra sha256 của tệp `cargo test --locked -p raumakey-windows --lib -- window::` fail ở `window::win::tests::a_dll_class_keeps_its_lock_until_its_last_window_is_gone_and_it_is_unregistered`. Assertion 'and its lock released' tại window.rs:797: left 1, right 0.
- Đỏ: Phát hiện 1: đột biến 'always release' của reviewer (`core::mem::forget(lock);` -> `drop(lock);`) `cargo test --locked -p raumakey-windows --lib -- window::` fail ở `window::win::tests::a_dll_class_that_cannot_be_unregistered_keeps_its_lock`. Assertion 'and the class keeps its lock' tại window.rs:835: left 0, right 1.
- Đỏ: Phát hiện 2: đột biến bỏ `|| read_slot(hwnd) != 0` khỏi install (phép kiểm tra slot trống) `cargo test --locked -p raumakey-windows --test layer -- window::` fail ở `window::win::a_wm_nccreate_forwarded_to_a_live_window_during_a_creation_leaves_both_handlers`. Assertion 'the live window kept its handler' tại tests/layer/window.rs:407: left 8, right 7.
- Đỏ: Phát hiện 2: đột biến `creating.set(self.previous)` -> `creating.set(core::ptr::null())` trong Creating::drop `cargo test --locked -p raumakey-windows --test layer -- window::` fail ở `window::win::a_window_created_while_another_is_being_created_leaves_both_handlers`. Assertion 'the window being created still got its handler' tại tests/layer/window.rs:444: left 0, right 10.
- Đỏ: Lần chạy lại cuối cùng của cả bốn đột biến trên mã đã hoàn thiện (sau các chỉnh sửa chỉ ở tài liệu), chạy chung test đơn vị và test layer `bash scratchpad/phase1-followup/mutate-window.sh <mutation> crates/raumakey-windows/src/window.rs ... -p raumakey-windows --lib --test layer -- window::` fail ở `window::win::tests::a_dll_class_keeps_its_lock_until_its_last_window_is_gone_and_it_is_unregistered`, `window::win::tests::a_dll_class_that_cannot_be_unregistered_keeps_its_lock`, `window::win::a_wm_nccreate_forwarded_to_a_live_window_during_a_creation_leaves_both_handlers`, `window::win::a_window_created_while_another_is_being_created_leaves_both_handlers`. Mỗi đột biến làm fail đúng test dự định của nó, mỗi đột biến một test, theo thứ tự always-forget, always-release, no-empty-slot-check, mark-not-restored.

Các review:

- Đặc tả và mức đầy đủ của test: không được duyệt (2 major, 3 minor, 5 nit).
- Tính đúng đắn, độ an toàn và chất lượng mã: không được duyệt (1 major, 3 minor, 3 nit).
- Review lại sau vòng sửa thứ nhất: được duyệt (1 minor, 1 nit).
- Review lại sau vòng sửa thứ hai: được duyệt (1 minor).

Các phát hiện mức minor còn mở từ lần review lại cuối cùng:

- minor, `crates/raumakey-windows/src/window.rs:262`: Phát hiện 1 đã được sửa cho nửa giải phóng và forget của khoá server thuộc một class DLL, nhưng nửa lấy khoá vẫn chưa được test.

### raumakey-tsf

Triển khai: 16 lần chạy đỏ và 11 lần chạy xanh.

- Đỏ: Đợt 1 (gõ phím #1 #3 #15 #17 #40, áp dụng thất bại #41, giá trị mặc định v1): test được viết trước mọi mã session `cargo test --locked -p raumakey-tsf` fail ở `session test target (compile error)`. E0432 unresolved import raumakey_tip::session: API chưa tồn tại
- Đỏ: Đợt 1: khung API với hành vi stub (test_key_down=false, key_down=pass-through) `cargo test --locked -p raumakey-tsf` fail ở `typing::matrix_1_telex_a_a_gives_a_circumflex_in_one_edit`, `typing::matrix_3_telex_a_s_gives_a_acute`, `typing::a_whole_word_is_typed_with_one_edit_per_transformation`, `typing::matrix_15_an_invalid_converted_word_is_restored_at_the_boundary`, `typing::a_space_after_a_valid_word_reaches_the_application`, `typing::matrix_17_semantic_backspace_restores_the_previous_state`, `typing::matrix_40_two_units_are_replaced_by_one_in_a_single_edit`, `typing::navigation_ends_the_word_and_reaches_the_application`, và 6 test khác. 14 test fail ở các assertion về claim/edit của chúng;
- Đỏ: Đợt 2 (context: #19 thay đổi vùng chọn, #20 chuyển tài liệu, đóng, giới hạn, lỗi engine) `cargo test --locked -p raumakey-tsf` fail ở `session test target (compile error)`. Thiếu MAX_DOCUMENTS, set_focus, document_closed, document_count
- Đỏ: Đợt 2: stub no-op cho API mới `cargo test --locked -p raumakey-tsf` fail ở `context::matrix_19_a_selection_change_by_the_user_resets_the_word`, `context::matrix_19_a_reported_change_resets_even_when_the_caret_ends_where_it_was`, `context::only_the_first_change_after_a_passed_key_is_its_echo`, `context::matrix_20_a_focus_change_mid_word_never_deletes_across_documents`, `context::matrix_20_a_key_from_another_document_is_a_document_change`, `context::a_focus_change_away_from_every_document_ends_the_word`, `context::closing_a_document_drops_its_state`, `context::document_states_are_bounded`, và 1 test khác. 9 test fail đúng như dự định;
- Đỏ: Đợt 3 (gate: #39 phím được chèn (injected), đường truyền không phải TSF, #29 ô bảo mật) `cargo test --locked -p raumakey-tsf` fail ở `gates::hosts_without_tsf_range_edits_are_left_to_the_controller`, `gates::matrix_39_an_injected_key_from_another_document_changes_nothing`. chưa có gate theo đường truyền;
- Đỏ: Đợt 4 (snapshot, #16, macro + phát lại Enter, snapshot không hợp lệ, chế độ, reset-all, quy tắc theo host) `cargo test --locked -p raumakey-tsf` fail ở `session test target (compile error)`. Thiếu SnapshotError, apply_runtime_snapshot, apply_macro_snapshot, set_vietnamese_mode, reset_all
- Đỏ: Đợt 4: stub trả về Ok/no-op `cargo test --locked -p raumakey-tsf` fail ở `config::a_runtime_snapshot_replaces_the_configuration`, `config::matrix_16_without_auto_restore_the_rendered_word_is_committed`, `config::an_invalid_runtime_snapshot_keeps_the_last_good_configuration`, `config::a_new_configuration_ends_the_word_in_progress`, `config::macros_expand_at_a_boundary`, `config::enter_after_a_macro_is_replayed_with_the_injected_tag`, `config::an_invalid_macro_snapshot_keeps_the_last_good_table`, `config::a_macro_snapshot_over_the_entry_limit_is_rejected`, và 4 test khác. 12 test fail đúng như dự định;
- Đỏ: Đợt 4: cài đặt cấu hình, kiểm tra hợp lệ, chế độ, reset-all `cargo test --locked -p raumakey-tsf` fail ở `config::a_runtime_snapshot_replaces_the_configuration`. lỗi của test: VNI 'a6' + dấu cách bị khôi phục vì riêng 'â' không phải là một âm tiết hoàn chỉnh;
- Đỏ: Đợt 5 (phím tắt D24) `cargo test --locked -p raumakey-tsf` fail ở `session test target (compile error)`. Thiếu set_connected, set_altgr_layout
- Đỏ: Đợt 5: stub `cargo test --locked -p raumakey-tsf` fail ở `hotkeys::d24_disconnected_ctrl_shift_toggles_the_local_mode`, `hotkeys::d24_the_toggle_ends_the_word_in_progress`, `hotkeys::d24_disconnected_ctrl_shift_alt_f12_hard_resets_and_is_eaten`, `hotkeys::d24_ctrl_shift_alt_space_is_the_other_hard_reset`, `hotkeys::d24_the_auto_repeat_of_a_hard_reset_key_is_swallowed`, `hotkeys::d24_losing_the_connection_hands_classification_back`, `hotkeys::d24_hotkeys_of_the_snapshot_replace_the_defaults`, `hotkeys::d24_altgr_types_characters_on_altgr_layouts`, và 2 test khác. 10 test fail đúng như dự định;
- Đỏ: Các đột biến sau test mới (bỏ qua generation / revision / full-key / verdict-memory) `cargo test --locked -q -p raumakey-tsf --test session (each mutation restored)` fail ở `cache::a_reset_between_the_test_of_a_passed_letter_and_key_down_starts_a_new_word`, `cache::a_macro_snapshot_between_test_and_key_down_is_honoured`, `cache::the_test_of_the_same_key_in_another_host_state_is_not_used`, `hotkeys::d24_disconnected_ctrl_shift_alt_f12_hard_resets_and_is_eaten (and 9 other hotkey tests)`. mỗi quy tắc bị đột biến đều bị phát hiện;
- Đỏ: Đợt 7, test quyền riêng tư `cargo test --locked -p raumakey-tsf` fail ở `session test target (compile error)`. E0277 ServiceCore không triển khai Debug
- Đỏ: Đợt 7 với Debug được derive tạm thời (làm lộ dữ liệu) `cargo test --locked -p raumakey-tsf` fail ở `privacy::key_strokes_hide_their_key_and_text`, `privacy::edit_requests_hide_document_text`, `privacy::key_down_outcomes_hide_the_replayed_key`, `privacy::the_core_hides_words_documents_and_macros`. Debug được derive đã in ra mã phím, văn bản UTF-16 và các từ mà core sở hữu
- Đỏ: Đợt 7 với các bản triển khai Debug đã che dữ liệu `cargo test --locked -p raumakey-tsf` fail ở `privacy::edit_requests_hide_document_text`, `privacy::the_core_hides_words_documents_and_macros`. lỗi của test: gõ 'ư' dựng sẵn thay vì Telex 'uw';
- Đỏ: Đợt 7 xanh, sau đó chạy đột biến (Debug làm lộ dữ liệu cho EditRequest và core) với các test đã sửa `cargo test --locked -q -p raumakey-tsf --test session privacy` fail ở `privacy::edit_requests_hide_document_text`, `privacy::the_core_hides_words_documents_and_macros`. các test đã sửa vẫn bắt được rò rỉ;
- Đỏ: Lỗi tràn được phát hiện khi review: echo của ứng dụng đối với các phím được gõ vượt quá giới hạn context đã đặt lại trạng thái tràn của engine `cargo test --locked -p raumakey-tsf` fail ở `typing::a_word_over_the_context_limit_passes_through_until_the_next_boundary`. left '…xâ', right '…xaa': từ bị bắt đầu lại giữa chừng token

Vòng sửa thứ nhất: 14 lần chạy đỏ và 11 lần chạy xanh.

- Đỏ: Đột biến (a): bỏ việc đặt lại sau một lần áp dụng thất bại (service.rs được sao lưu và khôi phục) `cargo test --locked -p raumakey-tsf --test session` fail ở `failures::matrix_41_the_key_after_a_failed_edit_starts_a_new_word`. Từ cũ "a" vẫn còn, nên phím 'a' tiếp theo tạo edit (attempts 2).
- Đỏ: Đột biến (b): commit next thay vì đặt lại sau một lần áp dụng thất bại `cargo test --locked -p raumakey-tsf --test session` fail ở `failures::matrix_41_the_key_after_a_failed_edit_starts_a_new_word`. "â" đã bị từ chối lại được commit, và phím 'a' tiếp theo thử thực hiện edit thứ hai.
- Đỏ: Đột biến (c): không đặt lại trong nhánh Decision::Reset `cargo test --locked -p raumakey-tsf --test session` fail ở `context::an_engine_error_ends_the_word_before_any_report_arrives`. Sau lỗi engine, từ "a" vẫn còn và 's' sửa nó.
- Đỏ: Phát hiện 1/7: kịch bản #19 của reviewer được viết theo API cũ `cargo test --locked -p raumakey-tsf --test session -- matrix_19_a_click_before_the_passed_key_is_reported_ends_the_word` fail ở `context::matrix_19_a_click_before_the_passed_key_is_reported_ends_the_word`. left "bá ba", right "bas ba": cú click bị coi là echo, và dấu thanh đã sửa từ nằm trước cú click.
- Đỏ: Phát hiện 1/7: chuyển test và fake sang selection_changed(document, SelectionChange) `cargo test --locked -p raumakey-tsf --test session --no-run` fail ở `(compile) 13 errors: E0432 unresolved import SelectionChange; E0061 selection_changed takes 1 argument`. API dựa trên bằng chứng chưa tồn tại.
- Đỏ: Đột biến: Arrival::is luôn là true (hành vi cũ: mọi thay đổi đầu tiên đều là echo) `cargo test --locked -p raumakey-tsf --test session` fail ở `context::matrix_19_a_click_before_the_passed_key_is_reported_ends_the_word`, `context::matrix_19_a_report_of_other_text_than_the_passed_key_ends_the_word`. Cho thấy các test mới phụ thuộc vào việc so sánh bằng chứng.
- Đỏ: Phát hiện 3/8: Ctrl+Shift kèm một thay đổi ngoại lai đã được báo cáo `cargo test --locked -p raumakey-tsf --test session -- d24_a_click_reported_while_ctrl_shift_is_held_cancels_the_toggle` fail ở `hotkeys::d24_a_click_reported_while_ctrl_shift_is_held_cancels_the_toggle`. assertion failed: !claimed.
- Đỏ: Phát hiện 3/8: đã viết test chuột `cargo test --locked -p raumakey-tsf --test session --no-run` fail ở `(compile) E0599 no method named mouse_down (4 errors)`. Điểm vào cho chuột chưa tồn tại.
- Đỏ: Phát hiện 8 (probe): đã viết test `cargo test --locked -p raumakey-tsf --test session --no-run` fail ở `(compile) E0599 no method named needs_probe / record_probe (8 errors)`. Các hàm chuyển tiếp (pass-through) cho probe chưa tồn tại.
- Đỏ: Các đột biến của phát hiện 4: set_focus giữ lại bộ phân loại (classifier) / enter_document giữ lại nó / các key-down giữ lại arrival đang chờ `cargo test --locked -p raumakey-tsf --test session (once per mutant, service.rs restored after each)` fail ở `hotkeys::d24_a_focus_change_mid_chord_never_fires`, `hotkeys::d24_a_document_change_mid_chord_never_fires`, `context::an_arrival_reported_after_the_next_key_down_ends_the_word`. Mỗi đột biến làm fail đúng test của riêng nó (mỗi lần chạy có 110 test pass, 1 test fail).
- Đỏ: Phát hiện 9: kiểu gõ tự định nghĩa không có bảng phím của nó `cargo test --locked -p raumakey-tsf --test session user_` fail ở `config::a_user_method_without_its_key_map_keeps_the_last_good_configuration`. left Ok(()), right Err(Runtime(Malformed(InvalidKeymap))): snapshot đã được chấp nhận.
- Đỏ: Phát hiện 10: test build_macro_table/install_macro_table `cargo test --locked -p raumakey-tsf --test session --no-run` fail ở `(compile) E0432 unresolved import build_macro_table; E0599 no method named install_macro_table`. API chưa tồn tại.
- Đỏ: Phát hiện 14 (nit): test phím khoá, sau đó đột biến bỏ nhánh xử lý phím khoá `cargo test --locked -p raumakey-tsf --test session` fail ở `typing::lock_keys_do_not_end_the_word`. Với đột biến này, Caps Lock trở thành một phím điều hướng bị claim và kết thúc từ.
- Đỏ: Quyền riêng tư của SelectionChange mới: test, sau đó đột biến dùng Debug được derive `cargo test --locked -p raumakey-tsf --test session privacy` fail ở `privacy::reported_changes_hide_the_inserted_text`. Một Debug được derive sẽ in ra các đơn vị UTF-16 được chèn.

Vòng sửa thứ hai: 7 lần chạy đỏ và 1 lần chạy xanh.

- Đỏ: Đỏ: các test mới về probe, ô bảo mật và chuột (phiên bản đầu tiên) chạy với mã chưa thay đổi `cargo test --locked -p raumakey-tsf --test session -- matrix_27 matrix_29_a_secure_field_is_never_probed matrix_19_a_mouse_press_after` fail ở `gates::matrix_27_a_snapshot_that_carries_a_working_probe_overrides_a_failed_local_one`, `gates::matrix_27_a_snapshot_that_carries_a_failed_probe_overrides_a_working_local_one`, `gates::matrix_27_a_host_is_probed_again_once_a_snapshot_drops_the_record_it_carried`, `gates::matrix_29_a_secure_field_is_never_probed`, `gates::matrix_27_a_context_without_a_document_is_never_probed`. Các test này fail đúng vì những lý do dự định.
- Đỏ: Đỏ: riêng lượt lặp với phiên bản mới hơn của test probe lại (thứ tự vòng lặp được đổi tạm thời, sau đó khôi phục) `cargo test --locked -p raumakey-tsf --test session -- matrix_27_a_host_is_probed_again` fail ở `gates::matrix_27_a_host_is_probed_again_once_a_snapshot_drops_the_record_it_carried`. Mã cũ giữ lại kết quả cục bộ sau khi một snapshot mang theo bản ghi có phiên bản mới hơn và áp dụng lại kết quả đó về sau, nên needs_probe vẫn là false.
- Đỏ: Đỏ: đột biến m03 của reviewer (việc đặt lại từ theo lý do Mouse trong mouse_down được thay bằng `let _ = document;`), áp dụng trên một bản sao lưu và khôi phục sau đó (kiểm tra bằng… `cargo test --locked -p raumakey-tsf --test session -- matrix_19 mouse` fail ở `context::matrix_19_a_mouse_press_after_the_last_report_ends_the_word`. Kết quả là "bá ba" thay vì "bas ba": phím dấu thanh đã sửa văn bản nằm trước một cú click chưa được báo cáo, đây chính là vi phạm ma trận #19.
- Đỏ: Lần chạy đầu tiên sau khi triển khai bản sửa `cargo test --locked -p raumakey-tsf` fail ở `gates::matrix_27_a_snapshot_that_carries_a_working_probe_overrides_a_failed_local_one`, `gates::matrix_27_a_host_is_probed_again_once_a_snapshot_drops_the_record_it_carried`. Các test kỳ vọng sai văn bản;
- Đỏ: Lại đỏ: các phiên bản test cuối cùng chạy với service.rs gốc (thay vào từ bản sao lưu, sau đó khôi phục bản sửa) `cargo test --locked -p raumakey-tsf --test session -- matrix_27 matrix_29_a_secure_field_is_never_probed` fail ở `gates::matrix_27_a_snapshot_that_carries_a_working_probe_overrides_a_failed_local_one`, `gates::matrix_27_a_snapshot_that_carries_a_failed_probe_overrides_a_working_local_one`, `gates::matrix_27_a_host_is_probed_again_once_a_snapshot_drops_the_record_it_carried`, `gates::matrix_29_a_secure_field_is_never_probed`, `gates::matrix_27_a_context_without_a_document_is_never_probed`. Cùng lý do như lần chạy đỏ đầu tiên.
- Đỏ: Kiểm tra đột biến M1: bỏ kết quả cục bộ khi snapshot mang bất kỳ bản ghi nào cho tệp thực thi, kể cả bản ghi có phiên bản cũ hơn (áp dụng, sau đó khôi phục) `cargo test --locked -p raumakey-tsf --test session -- matrix_27` fail ở `gates::matrix_27_a_record_of_an_older_probe_version_leaves_the_local_result_in_effect`. Test bảo vệ (guard) này cố định ranh giới này của quy tắc giữ lại.
- Đỏ: Kiểm tra đột biến M2: giữ kết quả cục bộ mãi mãi và chỉ áp dụng chúng khi không có bản ghi snapshot nào bao phủ tệp thực thi (áp dụng, sau đó khôi phục) `cargo test --locked -p raumakey-tsf --test session -- matrix_27` fail ở `gates::matrix_27_a_host_is_probed_again_once_a_snapshot_drops_the_record_it_carried`. Test probe lại cố định việc bỏ hẳn kết quả cục bộ.

Các review:

- Đặc tả và mức đầy đủ của test: không được duyệt (3 major, 2 minor, 1 nit).
- Tính đúng đắn, độ an toàn và chất lượng mã: không được duyệt (1 major, 4 minor, 3 nit).
- Review lại sau vòng sửa thứ nhất: không được duyệt (2 major, 1 minor).
- Review lại sau vòng sửa thứ hai: được duyệt (2 minor).

Các phát hiện mức minor còn mở từ lần review lại cuối cùng:

- minor, `crates/raumakey-tsf/tests/session/gates.rs:125`: Không có test nào bao phủ quy tắc đã được ghi trong tài liệu rằng kết quả probe cục bộ được áp dụng lại cho mọi snapshot về sau không có bản ghi (service.rs:530-531, mod.rs:112-117).
- minor, `crates/raumakey-tsf/src/session/mod.rs:167`: Hợp đồng của lớp COM (mod.rs:167-170, tài liệu của record_probe tại service.rs:524-540) chỉ nói rằng phải báo cáo từng kết quả đã ghi bằng Command::ReportProbe.
### raumakey-tests

Triển khai, bước 1: 5 lần chạy đỏ và 10 lần chạy xanh.

- Đỏ: viết fuzz_smoke.rs (cùng corpus, các generator và một PRNG splitmix64) trước khi raumakey_tests::fuzz_targets tồn tại `cargo test --locked -p raumakey-tests --test fuzz_smoke` fail ở `fuzz_smoke (compile error E0432: unresolved import `raumakey_tests::fuzz_targets`)`. Khi đó module fuzz_targets chưa tồn tại;
- Đỏ: thêm module fuzz_targets cùng byte reader và mỗi target một stub gây panic `cargo test --locked -p raumakey-tests --features evkey-compat --test fuzz_smoke` fail ở `engine_sequences`, `malformed_utf16`, `config_documents`, `macro_files`, `ipc_frames`, `legacy_sequence_wrap`, `transaction_application`, `every_target_accepts_the_empty_input_and_one_byte`. Mỗi stub panic với 'not implemented', nên test của mọi target đều fail ở mục corpus 0.
- Đỏ: Reader::chance phải là false khi gặp các byte 0; `cargo test --locked -p raumakey-tests --lib` fail ở `fuzz_targets::reader::tests::take_and_rest_stop_at_the_end`, `fuzz_targets::reader::tests::rare_choices_are_off_for_zero_bytes`. chance() coi các byte 0 của một reader đã cạn là "có", nên các input ngắn gần như luôn đi vào nhánh insert 8K unit vốn hiếm gặp.
- Đỏ: các test BUG bị ignore cho những trigger mà bảng chấp nhận nhưng engine không thể chạm tới `cargo test --locked -p raumakey-tests --test macro_engine -- --ignored` fail ở `d47_an_accepted_trigger_with_boundary_punctuation_expands`, `d47_an_accepted_trigger_longer_than_the_default_context_expands`. 'v.v' + dấu cách vẫn là 'v.v ' và một trigger 40 ký tự + dấu cách vẫn không được mở rộng trong Telex, VNI và VIQR;
- Đỏ: kiểm tra đột biến cho oracle: mỗi target một kỳ vọng bị cố ý làm sai (dùng sed/perl trên các bản sao, rồi khôi phục) `cargo test --locked -p raumakey-tests --features evkey-compat --test fuzz_smoke -- <target>` fail ở `transaction_application`, `macro_files`, `malformed_utf16`, `legacy_sequence_wrap`, `ipc_frames`, `config_documents`, `engine_sequences`. Mỗi bản tham chiếu bị đột biến (sai văn bản replay, id lớn nhất thắng, sai ranh giới scalar, sequence+1, số frame đã tới, so sánh văn bản TOML, độ dài hậu tố) đều bị bắt;

Triển khai, bước 2: 8 lần chạy đỏ và 4 lần chạy xanh.

- Đỏ: viết tests/compat_vectors.rs và tests/test_runner.rs trước mọi mã production `cargo test --locked -p raumakey-tests --test compat_vectors --test test_runner` fail ở `compat_vectors (compile error E0432: unresolved import raumakey_tests::vectors)`, `test_runner (compile error E0432: unresolved imports raumakey_tests::runner and raumakey_tests::vectors)`. Cả hai module đều chưa tồn tại.
- Đỏ: thêm khung của vectors và runner (kiểu, tài liệu, thân hàm stub) cùng các test đơn vị của loader và executor (src/vectors/tests.rs) `cargo test --locked -p raumakey-tests --lib --test compat_vectors --test test_runner --no-fail-fast` fail ở `17 vectors::tests (all loader and executor tests)`, `compat_vectors::every_embedded_vector_passes`, `compat_vectors::every_engine_level_matrix_row_has_a_vector`, `compat_vectors::every_plan_golden_vector_has_a_vector`, `compat_vectors::the_embedded_list_matches_the_fixture_directory`, `14 test_runner tests (parsing, report, environment, binary)`. Các stub: parse_fixture trả về Empty, run_vector trả về Fail, EMBEDDED rỗng và thư mục fixture chưa tồn tại, parse_args trả về NoCommand, report rỗng, và binary vẫn là fn main() {}.
- Đỏ: hiện thực phần kiểm tra hợp lệ và loader TOML `cargo test --locked -p raumakey-tests --lib vectors` fail ở `vectors::tests::ids_are_lowercase_groups_and_unique`, `vectors::tests::a_passing_vector_records_the_run`, `vectors::tests::every_mismatch_is_a_failure`, `vectors::tests::invalid_vectors_fail_setup_without_running`, `vectors::tests::vectors_run_with_the_real_macro_table_and_the_initial_text`. Bốn test của executor fail với run_vector dạng stub, đúng như dự định.
- Đỏ: chép lại 5 file fixture và điền EMBEDDED `cargo test --locked -p raumakey-tests --test compat_vectors` fail ở `no_two_vectors_share_a_setup`. Cả 203 vector đều đạt ngay lần chạy đầu tiên.
- Đỏ: test đơn vị của runner cho các bộ lọc giá trị registry, viết với các helper dạng stub `cargo test --locked -p raumakey-tests --lib runner` fail ở `runner::tests::registry_values_are_kept_only_in_their_documented_form`. Các stub is_build_number và is_display_version trả về false.
- Đỏ: test rằng chỉ binary của runner mang các chuỗi phiên bản và danh tính manifest, viết trước build.rs `cargo test --locked -p raumakey-tests --test test_runner only_the_runner` fail ở `only_the_runner_binary_carries_the_windows_resources`. Fail với 'no FileDescription': runner chưa có resource phiên bản.
- Đỏ: test đặc tả hành vi (characterisation test) cho các kiểm tra chưa áp dụng (not-applied) và hậu tố sở hữu (owned-suffix) của executor, dùng một ScriptOutcome được chỉnh tay `cargo test --locked -p raumakey-tests --lib unapplied` fail ở `unapplied_transactions_and_a_stray_word_are_failures (compile error E0603: function check is private)`. Một lần kiểm tra đột biến trước đó đã cho thấy không test nào chạm tới các kiểm tra phòng vệ này, vì engine thật không bao giờ tạo ra một transaction chưa được áp dụng.
- Đỏ: kiểm tra đột biến: áp một lỗi, chạy test tương ứng, khôi phục file và xác minh SHA-256 của nó `python -I <scratchpad>/mutate.py <worktree> (cargo test --locked -p raumakey-tests ... per mutation)` fail ở `debug_output_hides_keys_texts_and_macros`, `every_embedded_vector_passes (wrong text; wrong transaction)`, `every_vector_names_an_existing_source_test`, `every_engine_level_matrix_row_has_a_vector`, `every_plan_golden_vector_has_a_vector`, `unapplied_transactions_and_a_stray_word_are_failures (2 mutations)`, `the_report_holds_every_vector_with_the_design_fields`, `the_probed_environment_names_this_process (wrong UBR value)`. 11 trên 12 đột biến bị bắt và mọi file đều được khôi phục.

Vòng sửa thứ nhất: 10 lần chạy đỏ và 9 lần chạy xanh.

- Đỏ: đặc tả lỗi macro vượt giới hạn bằng test mới bị ignore dạng BUG `cargo test --locked -p raumakey-tests --test macro_engine -- --ignored d47_d53_accepted_replacements_beyond_the_insert_limit_expand` fail ở `d47_d53_accepted_replacements_beyond_the_insert_limit_expand`. 'big{SPACE}: an accepted macro never expands' (lỗi production, kế hoạch §14.4 mục 4).
- Đỏ: loader phải từ chối các id không phải VectorId `cargo test --locked -p raumakey-tests --lib ids_are_diagnostic_vector_ids` fail ở `vectors::tests::ids_are_diagnostic_vector_ids`. 'accepted 1 vectors': một id 33 byte lọt qua vì MAX_ID_LEN là 64.
- Đỏ: mọi id được nhúng phải là một VectorId `cargo test --locked -p raumakey-tests --test compat_vectors every_vector_id_is_a_diagnostic_vector_id` fail ở `every_vector_id_is_a_diagnostic_vector_id`. 40 id không phải VectorId hợp lệ (dài hơn 32 ký tự).
- Đỏ: test guard: các vector dựa trên quyết định về escape của VIQR phải nêu tên quyết định đó `cargo test --locked -p raumakey-tests --test compat_vectors` fail ở `vectors_resting_on_the_viqr_escape_decision_name_it`, `the_escape_scan_follows_plan_3_6`. Guard liệt kê đúng a08-viqr-escape-then-a-word.
- Đỏ: các test guard: không có dấu kết hợp (combining mark) thô trong fixture; `cargo test --locked -p raumakey-tests --test compat_vectors` fail ở `fixture_sources_write_combining_marks_as_escapes`, `rowless_baseline_vectors_are_plan_golden_vectors`. Có 20 dòng chứa dấu thô trong matrix-d-macros.toml.
- Đỏ: target engine phải báo các transaction không hợp lệ sau một lần host chỉnh sửa `cargo test --locked -p raumakey-tests --lib fuzz_targets::engine` fail ở `fuzz_targets::engine::tests::a_malformed_insert_is_a_finding_after_a_host_edit`, `fuzz_targets::engine::tests::an_insert_over_the_limit_is_a_finding_after_a_host_edit`, `fuzz_targets::engine::tests::a_replay_without_consume_is_a_finding_after_a_host_edit`. 'test did not panic as expected': nhánh stale lặng lẽ reset khi gặp bất kỳ lỗi apply nào.
- Đỏ: tách oracle thành các quy tắc suy ra từ kế hoạch và các quy tắc mô hình có gắn nhãn `cargo test --locked -p raumakey-tests --lib fuzz_targets::transaction` fail ở `fuzz_targets::transaction::tests::plan_2_7_gives_enter_tab_and_space_their_text`, `fuzz_targets::transaction::tests::the_model_rules_apply_only_where_plan_2_7_does_not_decide`, `fuzz_targets::transaction::tests::the_simulator_replays_the_events_own_key_with_its_text`. Lỗi biên dịch E0425: plan_replay_text và model_replay_rule chưa tồn tại.
- Đỏ: một vector gây panic phải được tính là thất bại của vector đó, không phải một lần abort `cargo test --locked -p raumakey-tests --lib a_panicking_run` fail ở `vectors::tests::a_panicking_run_is_a_failure_of_its_vector`. Lỗi biên dịch: run::run_vector_with chưa được resolve và Failure::Panic chưa tồn tại.
- Đỏ: dòng panic chỉ nêu vị trí cho binary của runner `cargo test --locked -p raumakey-tests --lib runner::` fail ở `runner::tests::a_panic_is_reported_by_its_location_only`. Lỗi biên dịch: runner::panic_line chưa được resolve.
- Đỏ: test fuzz_smoke cho config_concurrency được viết trước `cargo test --locked -p raumakey-tests --test fuzz_smoke` fail ở `config_concurrency`, `every_target_accepts_the_empty_input_and_one_byte`. Lỗi biên dịch E0425: fuzz_targets::config_concurrency chưa tồn tại.

Các review:

- Đặc tả và mức đầy đủ của test: không được duyệt (1 major, 5 minor, 3 nit).
- Tính đúng đắn, độ an toàn và chất lượng mã: không được duyệt (2 major, 3 minor, 4 nit).
- Review lại sau vòng sửa thứ nhất: được duyệt (1 minor).

Các phát hiện mức minor còn mở từ lần review lại cuối cùng:

- minor, `tests/tests/fuzz_smoke/corpus.rs:285`: Mục corpus mới `stale_and_future_revisions` của config_concurrency không giải mã theo đúng cách mà các chú thích của nó mô tả.

## Kiểm chứng

Sau khi merge `raumakey-windows` và `raumakey-tsf` lên trên `d069063` (commit `834dbf5`):

- `cargo fmt --all --check`: đạt.
- `cargo xtask ci --no-fuzz`: Clippy với `-D warnings` đạt với các feature mặc định và với cả
  năm feature `evkey-compat`; 1,442 test đạt với các feature mặc định và 1,515 với các feature
  tương thích, 0 không đạt, 10 test thủ công bị bỏ qua (ignored).
- `cargo test --locked -p raumakey-windows -p raumakey-tsf --target i686-pc-windows-msvc`: 346
  test đạt dưới WOW64, 0 không đạt, 10 test thủ công bị bỏ qua.
- `cargo xtask cross`: các build debug cho x86_64, i686 và aarch64 đều đạt.

Sau khi merge `raumakey-tests` (bản sửa đổi này; kế hoạch §14.6):

- `cargo fmt --all --check`: đạt.
- `cargo xtask ci`, nay có thêm bước fuzz smoke: Clippy với `-D warnings` đạt với cả hai bộ
  feature; 1,535 test đạt với các feature mặc định và 1,609 với các feature tương thích,
  0 không đạt, 13 bị bỏ qua (10 test thủ công và 3 test `BUG` của kế hoạch §14.4);
  `fuzz_smoke` đạt 10 test.
- `cargo test --locked -p raumakey-tests --target i686-pc-windows-msvc`: 93 test đạt dưới WOW64,
  3 test `BUG` bị bỏ qua.
- `cargo xtask cross`: các build debug cho x86_64, i686 và aarch64 đều đạt.
- `raumakey-test-runner vectors --json <file>`: 202 trên 202 vector đạt.

## Giai đoạn 2a: lớp COM của TIP và controller

Được xây dựng trong hai worktree từ `0bdfbcc` và được review theo cùng cách như Giai đoạn 1: hai
reviewer độc lập cho mỗi phần, người triển khai tự sửa, và một lần review lại đã duyệt các bản
sửa mà không còn phát hiện blocker hay major nào.

| Phần | Nội dung | Trạng thái trong kế hoạch |
|---|---|---|
| `raumakey-tsf` | Lớp COM của `raumakey_tip.dll`: build script kèm resource, bốn export, class factory, `DllRegisterServer`/`DllUnregisterServer`, `TextService` cùng vòng đời kích hoạt và các sink của nó, đường đi của phím qua các edit session đồng bộ (module `adapter` thuần được test bằng fake), các input scope, hook `WH_MOUSE` theo thread, và compartment open/close | §14.2 công đoạn 6 |
| `raumakey-controller` | Build script kèm resource, GUI subsystem, dòng lệnh (`install`/`uninstall` với chạy thử (dry run), hoàn tác (rollback) và nâng quyền; `startup`; `convert`; `update`; `--version`), và một chế độ chạy tray tối giản; các helper mới trong `raumakey-windows` (bộ lọc message UIPI, bộ giải mã callback của notify icon, tên logon task theo từng tài khoản) | §14.2 công đoạn 5 |

Các ghi chú mà người triển khai để lại cho người phụ trách (lead):

- Quyết định ở mức mã (`raumakey-tsf`): một bản ghi `OnEndEdit` không có thay đổi văn bản lẫn
  thay đổi vùng chọn thì không được báo cho `ServiceCore::selection_changed`. Kế hoạch §10.3
  chưa ghi nhận điều này.
- Sai lệch so với TDD (`raumakey-tsf`, bước 2): ba test COM đầu tiên về ô bảo mật trong
  `tests/com/secure.rs` được viết sau `text_service/scope.rs` và đã đạt ngay lần chạy đầu. Các
  đường đọc scope khi uninit, khi push và ở phím đầu tiên chỉ được ghim lại sau khi có đột biến
  sống sót; trong vòng sửa, mọi test mới đều được quan sát thấy fail trước.
- Đề xuất bị từ chối (`raumakey-tsf`): đọc lại input scope sau mỗi `OnEndEdit` sẽ thêm một
  session chỉ đọc đồng bộ vào gần như mọi phím. Thay đổi scope trong khi một ô vẫn giữ focus
  được nhận ra ở lần đổi focus kế tiếp (đã ghi trong `server.rs`).
- Câu hỏi còn mở (`raumakey-controller`): `install` đăng ký các TIP tại nơi package được giải
  nén và cảnh báo khi người dùng có quyền ghi vào thư mục đó; kế hoạch §14.4 mục 6.
- Phương án thay thế bị từ chối (`raumakey-controller`): thay vì kiểm tra principal đã đăng ký
  của logon task được nâng quyền, tên task mang hash SID của tài khoản và tiến trình được khởi
  động lại từ chối hành động thay cho một tài khoản khác.

### raumakey-tsf (lớp COM)

Triển khai, bước 1: 20 lần chạy đỏ và 9 lần chạy xanh.

- Đỏ: 1 vòng đời: các test được viết trước module `cargo test --locked -p raumakey-tsf --test lifecycle` fail ở `lifecycle (compile error)`. E0432 unresolved import raumakey_tip::activation: module chưa tồn tại (có chủ đích).
- Đỏ: 1 vòng đời: lỗi trong một helper của test `cargo test --locked -p raumakey-tsf --test lifecycle` fail ở `lifecycle (compile error)`. E0277/E0599: helper đã đảo ngược một RangeFrom không có cận trên.
- Đỏ: 1 vòng đời: khung API với advise_all/unadvise_all dạng stub `cargo test --locked -p raumakey-tsf --test lifecycle` fail ở `activation_advises_every_sink_once_in_order`, `a_failed_advise_undoes_what_succeeded_last_first_and_returns_its_error`, `a_failed_undo_neither_stops_the_undo_nor_replaces_the_advise_error`, `deactivation_unadvises_every_sink_last_first`, `deactivation_unadvises_every_sink_even_after_failures_and_returns_the_first_error`, `debug_output_names_the_advised_sinks`. Các stub không advise và cũng không unadvise gì, nên các assertion trên những lời gọi được ghi lại đều fail.
- Đỏ: 2 resource: các test được viết trước dev-dependency và build.rs `cargo test --locked -p raumakey-tsf --test resources` fail ở `resources (compile error)`. E0432 unresolved import raumakey_resources: khi đó nó mới chỉ là một build-dependency.
- Đỏ: 2 resource: đã thêm dev-dependency, chưa có build.rs `cargo test --locked -p raumakey-tsf --test resources` fail ở `win::only_the_dll_carries_the_version_information_of_the_tip`, `win::the_profile_icon_of_the_registration_loads_from_the_dll`. 'no FileDescription', và ExtractIconExW đếm được Some(0) icon thay vì Some(3).
- Đỏ: 3 vỏ COM: các test export/factory/service/artifact được viết trước các export `cargo test --locked -p raumakey-tsf --test com` fail ở `com (compile error)`. E0432 unresolved imports raumakey_tip::DllGetClassObject và raumakey_tip::DllCanUnloadNow.
- Đỏ: 3 vỏ COM: các export dạng stub (CLASS_E_CLASSNOTAVAILABLE mà không xoá *ppv, S_OK, E_NOTIMPL) `cargo test --locked -p raumakey-tsf --test com` fail ở `exports::the_tip_clsid_is_served_by_a_class_factory`, `exports::other_classes_are_not_available_and_the_result_is_cleared`, `exports::null_pointers_are_rejected`, `exports::the_class_object_answers_only_as_a_class_factory`, `exports::dll_can_unload_now_waits_for_every_object_and_server_lock`, `service::aggregation_is_rejected`, `service::one_object_answers_for_the_processor_and_every_sink`, `service::the_text_service_stays_in_its_apartment`, và 2 test khác. Chưa có class factory (support.rs:53 kỳ vọng S_OK), kết quả không được xoá, và E_POINTER/E_NOINTERFACE không bao giờ được trả về.
- Đỏ: 4 danh tính: test GUID ở dạng nhị phân được viết trước static của profile `cargo test --locked -p raumakey-tsf --test resources` fail ở `win::the_dll_carries_its_class_and_profile_identities_in_binary_form`. 'TIP_PROFILE_GUID {DB875058-BBF9-407C-85A8-950BE7D14C22} is not in the image'.
- Đỏ: 5 kích hoạt: các test với TSF thật và với shim chạy trên phần kích hoạt dạng stub (E_NOTIMPL) `cargo test --locked -p raumakey-tsf --test com` fail ở `activation::the_real_thread_manager_refuses_the_key_sink`, `activation::activation_advises_every_sink_and_deactivation_releases_every_one`, `activation::a_failed_step_leaves_nothing_advised_and_its_error_is_returned`, `activation::deactivation_is_idempotent_and_activation_can_follow_it`, `activation::a_second_activation_replaces_the_first_without_leaking_its_sinks`, `activation::a_null_thread_manager_is_an_invalid_argument`. ActivateEx trả về E_NOTIMPL (0x80004001) ở chỗ các test kỳ vọng E_INVALIDARG, S_OK, hoặc lỗi của bước được tiêm vào.
- Đỏ: 5 kích hoạt: các test đơn vị cho việc reset core và tính tái nhập (re-entrancy) được viết trước field activation `cargo test --locked -p raumakey-tsf --lib` fail ở `lib test (compile error)`. E0609 no field `activation` on ComObject<TextService>.
- Đỏ: 5 kích hoạt: đã thêm field, Deactivate vẫn là stub `cargo test --locked -p raumakey-tsf --lib` fail ở `text_service::tests::deactivation_leaves_a_fresh_core_for_the_next_activation`, `text_service::tests::an_activation_change_inside_another_is_refused_instead_of_panicking`. Core không được thay thế (assertion !core.is_connected()), và kết quả là None thay vì HRESULT 0x800700AA (ERROR_BUSY).
- Đỏ: đột biến M1: reset_core dùng borrow_mut (panic khi core đang bận) `bash <scratchpad>/mutate.sh crates/raumakey-tsf/src/text_service.rs <subst> --lib` fail ở `text_service::tests::a_core_in_use_is_left_alone_instead_of_panicking`. Panic bị bắt trở thành E_UNEXPECTED.
- Đỏ: đột biến M2: Deactivate drop phần kích hoạt mà không unadvise `bash <scratchpad>/mutate.sh crates/raumakey-tsf/src/text_service.rs <subst> --test com` fail ở `activation::activation_advises_every_sink_and_deactivation_releases_every_one`, `activation::deactivation_is_idempotent_and_activation_can_follow_it`, `activation::a_second_activation_replaces_the_first_without_leaking_its_sinks`, `(cascade: later object_count checks, because the leaked service stays alive)`. TSF vẫn giữ các tham chiếu tới sink của nó, nên số tham chiếu vẫn cao hơn mức nền.
- Đỏ: đột biến M4: lần kích hoạt thứ hai drop lần thứ nhất mà không unadvise `bash <scratchpad>/mutate.sh crates/raumakey-tsf/src/text_service.rs <subst> --test com` fail ở `activation::a_second_activation_replaces_the_first_without_leaking_its_sinks`, `(cascade: object_count checks)`. Các sink của lần kích hoạt thứ nhất bị rò rỉ.
- Đỏ: đột biến M5: take_activation dùng borrow_mut `bash <scratchpad>/mutate.sh crates/raumakey-tsf/src/text_service.rs <subst> --lib` fail ở `text_service::tests::an_activation_change_inside_another_is_refused_instead_of_panicking`. Nhận E_UNEXPECTED từ một panic bị bắt thay vì ERROR_BUSY.
- Đỏ: đột biến M3: advise_all không hoàn tác sau một lần advise thất bại `bash <scratchpad>/mutate.sh crates/raumakey-tsf/src/activation.rs <subst> --test lifecycle (and --test com)` fail ở `a_failed_advise_undoes_what_succeeded_last_first_and_returns_its_error`, `a_failed_undo_neither_stops_the_undo_nor_replaces_the_advise_error`, `activation::a_failed_step_leaves_nothing_advised_and_its_error_is_returned`, `activation::the_real_thread_manager_refuses_the_key_sink`. Với thread manager thật, hai sink đã được advise trước key sink bị từ chối vẫn bị TSF giữ.
- Đỏ: đột biến M6: unadvise_all dừng lại ở lỗi đầu tiên `bash <scratchpad>/mutate.sh crates/raumakey-tsf/src/activation.rs <subst> --test lifecycle` fail ở `a_failed_undo_neither_stops_the_undo_nor_replaces_the_advise_error`, `deactivation_unadvises_every_sink_even_after_failures_and_returns_the_first_error`. Các sink phía sau không bao giờ được unadvise.
- Đỏ: build cũ (stale) sau lần khôi phục đột biến cuối cùng (không phải lỗi mã) `cargo test --locked -p raumakey-tsf --test com` fail ở `13 com tests, starting with activation::a_failed_step_leaves_nothing_advised_and_its_error_is_returned (ThreadFocus: a sink is still held, 2 vs 1)`. Script đột biến đã khôi phục activation.rs bằng mv từ một bản sao lưu tạo bằng cp, có mtime cũ hơn build đã bị đột biến, nên Cargo dùng lại thư viện của M3.
- Đỏ: Clippy trên mã test `cargo clippy --locked -p raumakey-tsf --all-targets -- -D warnings` fail ở `clippy::bool_assert_comparison at tests/com/activation.rs:208`. Câu assert_eq!(.., true) vụng về được thay bằng một kiểm tra pattern đối với các advise của Activate.
- Đỏ: Clippy cho i686 `cargo clippy --locked -p raumakey-tsf --all-targets --target i686-pc-windows-msvc -- -D warnings` fail ở `clippy::boxed_local at src/session/service.rs:771 (pre-existing, 32-bit only)`. EngineResult nằm dưới ngưỡng kích thước của lint trên các target 32-bit.

Triển khai, bước 2: 14 lần chạy đỏ và 8 lần chạy xanh.

- Đỏ: các test adapter và test đơn vị về repeat của KeyStroke được viết trước mọi mã `cargo test --locked -p raumakey-tsf --lib --test adapter` fail ở `lib test (compile error)`, `adapter (compile error)`. E0560 struct KeyStroke has no field named repeat;
- Đỏ: khung API của adapter (kiểu và tài liệu, thân hàm stub); `cargo test --locked -p raumakey-tsf --lib --test adapter --no-fail-fast` fail ở `keys::a_first_key_down_is_decoded_from_wparam_and_lparam`, `keys::the_synthetic_left_ctrl_of_altgr_is_dropped_on_an_altgr_layout`, `keys::altgr_is_ctrl_and_alt_that_type_a_character_on_an_altgr_layout`, `edit::matrix_40_two_units_are_replaced_by_one`, `edit::matrix_41_a_shorter_text_before_the_caret_changes_nothing`, `edit::a_cut_through_a_surrogate_pair_never_matches`, `record::a_typed_key_is_its_text_inserted_before_the_caret`, `documents::the_least_recently_used_context_makes_room_and_is_reported`, và 5 test khác. Các stub không giải mã gì và cũng không loại bỏ gì.
- Đỏ: accessor Connections::connection, viết test trước `cargo test --locked -p raumakey-tsf --test lifecycle` fail ở `lifecycle (compile error)`. E0599: no method named connection on Connections<C>.
- Đỏ: các test COM trong tiến trình (fixture tsf, typing, focus) và việc chuyển tiếp GetFocus của shim được viết trước phần glue `cargo test --locked -p raumakey-tsf --test com` fail ở `com (compile error)`. E0432 unresolved import raumakey_tip::mouse_hook (có chủ đích).
- Đỏ: các test đơn vị của mouse_hook chạy với một install dạng stub luôn thất bại `cargo test --locked -p raumakey-tsf --lib mouse_hook` fail ở `mouse_hook::tests::every_button_press_reaches_the_handler_once`, `mouse_hook::tests::the_hook_belongs_to_the_thread_that_installed_it`, `mouse_hook::tests::one_hook_per_thread_and_only_its_owner_removes_it`, `mouse_hook::tests::a_panicking_handler_never_unwinds_into_windows`, `mouse_hook::tests::a_handler_may_remove_the_hook`. install trả về Err, nên expect("installed") fail.
- Đỏ: các test COM chạy với phần glue của bước 1 (phím chưa được nối vào) `cargo test --locked -p raumakey-tsf --test com --no-fail-fast` fail ở `activation::activation_advises_every_sink_and_deactivation_releases_every_one`, `activation::deactivation_is_idempotent_and_activation_can_follow_it`, `typing::telex_words_are_typed_through_real_tsf_ranges`, `typing::matrix_19_a_change_the_service_did_not_make_ends_the_word`, `typing::matrix_19_a_mouse_press_ends_the_word`, `typing::matrix_17_backspace_restores_the_previous_state`, `focus::activation_installs_the_mouse_hook_of_the_thread_and_deactivation_removes_it`, `focus::the_text_edit_sink_follows_the_focus`, và 2 test khác. Không có lời gọi GetFocus sau các advise ('unexpected activation calls'), không có chỉnh sửa nào (văn bản vẫn chưa được biến đổi), không có hook, không có edit sink (theo số tham chiếu), và compartment không bao giờ được ghi.
- Đỏ: các test đơn vị của input_scopes (ITfInputScope giả trong một VARIANT VT_UNKNOWN) chạy với một stub `cargo test --locked -p raumakey-tsf --lib text_service` fail ở `text_service::scope::tests::the_scopes_of_an_input_scope_object_are_read`, `text_service::scope::tests::a_long_list_is_read_up_to_the_limit`. Stub không trả về scope nào.
- Đỏ: làm chặt matrix_20 (chuyển focus đi rồi quay lại mà không có phím nào ở giữa): một lỗi trong test `cargo test --locked -p raumakey-tsf --test com focus::` fail ở `focus::matrix_20_a_focus_change_never_edits_across_documents (plus cascades)`. Nhận được 'aa aa aa' thay vì 'aa â aa'.
- Đỏ: lượt chạy đột biến 1, áp dụng - chạy - khôi phục kèm kiểm tra SHA-256 và tăng mtime (M1 đến M10) `python -I <scratchpad>/mutate.py <worktree> <mutation> test --locked -p raumakey-tsf --test com` fail ở `M1 end_edit never reports: typing::matrix_19_a_change_the_service_did_not_make_ends_the_word`, `M2 mouse press ignored: typing::matrix_19_a_mouse_press_ends_the_word`, `M5 mode never written: focus::activation_shows_the_vietnamese_mode_in_the_open_close_compartment`, `M6 signed shift count / M7 session never armed: typing::telex_words_are_typed_through_real_tsf_ranges`, `M8 extra info not read: typing::matrix_39_keys_injected_by_raumakey_reach_the_application_untouched`, `M9 every field secure: typing tests`, `M10 pop keeps no focus: focus::a_pushed_context_takes_the_focus_and_its_pop_gives_it_back`. Mọi đột biến trừ M4 đều bị bắt.
- Đỏ: sự cố ký tự xuống dòng (lỗi quy trình, không phải lỗi mã) `python check of b'\r' in crates/**/*.rs` fail ở `mutations M3 and M11 (pattern matched 0 times)`. Các chỉnh sửa inline bằng Python ở text mode đã ghi 12 file với CRLF, nên các pattern đột biến nhiều dòng không khớp.
- Đỏ: lượt chạy đột biến 2 (M3, M11 đến M22) `python -I <scratchpad>/mutate.py <worktree> <mutation> test --locked -p raumakey-tsf --test com` fail ở `M3 focus not told to core: focus::matrix_20_a_focus_change_never_edits_across_documents`, `M12 hook never removed: focus::activation_installs_the_mouse_hook_of_the_thread_and_deactivation_removes_it`, `M13 edit sink never advised: focus::the_text_edit_sink_follows_the_focus`, `M14 caret never at end / M15 no text changes: typing::telex_words_are_typed_through_real_tsf_ranges`, `M19 key path never follows: typing::matrix_19_a_change_the_service_did_not_make_ends_the_word`, `M22 activation focus ignored: focus::activation_advises_the_edit_sink_of_the_document_that_has_the_focus`. Các đột biến sống sót: M11 (uninit vẫn giữ focus), M16 (bộ đếm chỉnh sửa của chính service), M17 (bỏ qua SetSelection của TSF), M18 (push bị bỏ qua), M20 (không đọc scope ở phím đầu tiên), M21 (không đọc scope khi nhận focus).
- Đỏ: chạy lại các đột biến sống sót với các test mới `python -I <scratchpad>/mutate.py <worktree> M11|M18|M20 test ... --test com; A1 ... --test adapter` fail ở `M11: focus::an_uninitialized_document_manager_releases_the_edit_sink_of_its_context`, `M18: focus::a_pushed_context_takes_the_focus_and_ends_the_word_of_the_one_below`, `M20: typing::a_document_that_never_had_the_focus_reads_its_input_scope_at_its_first_key`, `A1: edit::the_caret_follows_the_insert_even_where_the_host_selects_it, edit::a_refused_selection_puts_the_old_text_back`. Mỗi đột biến làm fail đúng test dự định của nó.
- Đỏ: Clippy trên mã test `cargo clippy --locked -p raumakey-tsf --all-targets -- -D warnings` fail ở `clippy::type_complexity at tests/com/tsf.rs:167`, `clippy::let_and_return at tests/com/tsf.rs:291`. Đã sửa bằng một type alias SessionBody và một tail expression.
- Đỏ: lượt đột biến cuối cùng, 25 đột biến, một thread test `python -I <scratchpad>/mutate.py <worktree> <M1..M24|A1> test --locked -p raumakey-tsf --test com|adapter -- --test-threads=1` fail ở `23 of 25 caught, each by its intended test (checked by name in the log)`. Các đột biến sống sót đúng như dự kiến: M16 (bộ đếm chỉnh sửa của chính service không quan sát được trong khi core đang bị mượn trong các session của chính service) và M17 (store riêng của TSF giữ caret sau SetText tại caret;

Vòng sửa thứ nhất: 10 lần chạy đỏ và 10 lần chạy xanh.

- Đỏ: Phát hiện 2: các test focus mới chạy với refocus có lời gọi set_focus bị thay bằng stub (`let _ = document;`) `cargo test --locked -p raumakey-tsf --test com -- focus::losing` fail ở `focus::losing_the_thread_focus_ends_the_word`, `focus::losing_the_keyboard_focus_of_the_key_event_sink_ends_the_word`. left "cá", right "cas": không có set_focus, dấu thanh đã tác động ngược qua lần mất focus
- Đỏ: Phát hiện 1: test lock bị từ chối chạy với đột biến edit_session.rs:72 `unwrap_or(Ok(()))` `cargo test --locked -p raumakey-tsf --test com -- hosts::` fail ở `hosts::matrix_41_a_refused_edit_session_lets_the_key_through_and_ends_the_word`. 'the key reaches the application': phím bị nuốt và trạng thái bị commit dù TSF không chạy edit session nào
- Đỏ: Phát hiện 1 (không có context): test đơn vị chạy với đột biến `return Ok(())` trong nhánh không có context `cargo test --locked -p raumakey-tsf --lib -- edit_session` fail ở `text_service::edit_session::tests::a_key_without_a_context_is_denied_without_an_edit_session`. left Ok(()), right Err(Denied)
- Đỏ: Phát hiện 8: test Ctrl+S chạy với đột biến `ModifierKeys::read(|_| false, |_| false)` `cargo test --locked -p raumakey-tsf --test com -- typing::matrix_21` fail ở `typing::matrix_21_a_letter_typed_with_ctrl_held_is_a_shortcut_that_ends_the_word`. 'Ctrl+S reaches the application': không có các phím bổ trợ, S là phím dấu thanh và bị nuốt
- Đỏ: Phát hiện 6: test host chọn phần vừa chèn chạy với M17 (TsfRange::select_end bỏ qua set_selection) `cargo test --locked -p raumakey-tsf --test com -- hosts::` fail ở `hosts::matrix_40_the_edit_leaves_a_caret_after_the_new_text_where_the_host_selects_it`. left "tings", right "tiếng".
- Đỏ: Phát hiện 4: test ô mật khẩu chạy với mã production chưa sửa `cargo test --locked -p raumakey-tsf --test com -- secure::` fail ở `secure::a_field_that_became_a_password_field_is_secure_until_its_scope_is_read_again`. left "â â", right "â aa": giá trị 'không bảo mật' đã cũ trong cache khiến các phím trong ô mật khẩu bị biến đổi trong khi lần đọc lúc nhận focus bị hoãn lại
- Đỏ: Phát hiện 4, test thứ hai: test ô thường sau ô mật khẩu với text_service/mod.rs trước khi sửa được thay vào `cargo test --locked -p raumakey-tsf --test com -- secure::` fail ở `secure::a_field_whose_scope_read_waits_is_read_again_at_its_first_key`, `secure::a_field_that_became_a_password_field_is_secure_until_its_scope_is_read_again`. left "aa", right "â" (giá trị 'bảo mật' đã cũ khiến một ô thường vẫn bị bỏ qua), và "â â" như trước.
- Đỏ: nit về thứ tự đọc: test thẻ tiêm (injection tag) chạy với mã production chưa sửa `cargo test --locked -p raumakey-tsf --test com -- hosts::matrix_39` fail ở `hosts::matrix_39_the_injection_tag_of_a_key_is_read_before_tsf_runs_for_it`. left (true, false), right (false, false): extra info được đọc sau lần đọc scope đồng bộ, trong lúc đó nó đã thay đổi, nên phím được tiêm vào bị nhận xử lý (claim)
- Đỏ: nit về thứ tự đọc (phím bổ trợ): test chạy với một đột biến đọc ModifierKeys bên trong key_stroke sau enter() `cargo test --locked -p raumakey-tsf --test com -- hosts:: typing::matrix_21` fail ở `hosts::the_modifiers_of_a_key_are_read_before_tsf_runs_for_it`. 'the key was tested as the letter that was typed': Ctrl, được bật trong lúc đọc scope, đã biến chữ cái thành một phím tắt.
- Đỏ: nit về mouse hook: test drop handler chạy với procedure chưa sửa `cargo test --locked -p raumakey-tsf --lib -- mouse_hook::tests::dropping_a_handler` fail ở `mouse_hook::tests::dropping_a_handler_after_its_call_never_unwinds_into_windows`. tiến trình test bị abort với 'panic in a function that cannot unwind' trong raumakey_tip::mouse_hook::procedure (exit 0xc0000409): Rc của handler bị drop bên ngoài guard

Các review:

- Đặc tả và mức đầy đủ của test: không được duyệt (2 major, 4 minor, 1 nit).
- Tính đúng đắn, độ an toàn và chất lượng mã: không được duyệt (1 major, 4 minor, 2 nit).
- Review lại sau vòng sửa thứ nhất: được duyệt (1 minor).

Các phát hiện mức minor còn mở từ lần review lại cuối cùng:

- minor, `crates/raumakey-tsf/tests/com/hosts.rs:101`: Hai test mới về thứ tự đọc có thể đạt mà không thực sự kiểm tra được gì.

### raumakey-controller

Triển khai, bước 1: 12 lần chạy đỏ (red) và 8 lần chạy xanh (green).

- Đỏ: A1 tài nguyên và subsystem GUI: tests/binary.rs được viết trước build.rs và thuộc tính subsystem `cargo test --locked -p raumakey-controller --test binary` không đạt ở `the_controller_is_a_windows_gui_application`, `only_the_controller_binary_carries_its_windows_resources`. PE Subsystem là 3 (console) thay vì 2;
- Đỏ: B1 các test parser (tests/cli.rs) được viết trước khi có bất kỳ thư viện nào `cargo test --locked -p raumakey-controller --test cli` không đạt ở `cli (compile error E0433)`. không phân giải được crate raumakey_controller: package chưa có target thư viện
- Đỏ: B2 khung cli.rs với các kiểu thật, parse_args dạng stub trả về UnknownCommand, Display và USAGE rỗng `cargo test --locked -p raumakey-controller --test cli` không đạt ở `no_arguments_run_the_controller`, `version_help_and_update_take_no_arguments`, `install_options_default_to_the_package_layout`, `install_options_are_accepted_in_any_order`, `the_elevated_marker_is_an_option_of_the_privileged_commands`, `repeated_options_are_usage_errors`, `missing_values_are_usage_errors`, `unknown_options_and_stray_arguments_are_usage_errors`, và 10 test khác. 18 trên 20 test không đạt ở các assertion so với stub (the_command_comes_first và exit_codes_are_distinct đạt một cách hiển nhiên)
- Đỏ: C1 hệ thống giả (tests/support), các test install/uninstall, startup, convert, dispatch và các test cấp tiến trình được viết trước các module của chúng `cargo test --locked -p raumakey-controller --no-fail-fast` không đạt ở `binary (compile error)`, `commands (compile error)`, `convert (compile error)`, `install (compile error)`, `startup (compile error)`. E0432, không phân giải được các import raumakey_controller::{commands, convert, elevation, install, output, startup}
- Đỏ: C2 các module khung với các trait thật và phần thân không làm gì (no-op) trả về 0 `cargo test --locked -p raumakey-controller --no-fail-fast` không đạt ở `binary: 7 of 9 (e.g. version_prints_the_product_name_and_version, the_install_dry_run_accepts_its_options_in_any_order, the_uninstall_dry_run_meets_the_gate_contract, usage_errors_exit_with_2_and_print_the_usage, convert_writes_a_new_file_and_refuses_to_overwrite_it)`, `commands: 11 of 12`, `convert: 9 of 10`, `install: 32 of 33 (e.g. install_registers_both_tips_verifies_them_and_records_the_installation, a_failed_registration_undoes_the_registrations_made_before_it, without_administrator_rights_install_restarts_itself_elevated)`, `startup: 13 of 13`. các stub không in gì, không thay đổi gì và trả về 0;
- Đỏ: D1 test kho bản ghi (record store) dựa trên registry (HKCU\Software\Raumakey\Tests-controller-<pid>-<n>) được viết trước system.rs `cargo test --locked -p raumakey-controller --test record_store` không đạt ở `record_store (compile error)`. E0432, không phân giải được import raumakey_controller::system
- Đỏ: D2 RegistryRecordStore dạng stub (đọc trả về None, ghi và xoá không làm gì) `cargo test --locked -p raumakey-controller --test record_store` không đạt ở `a_record_round_trips_as_string_values`, `writing_replaces_the_values_of_architectures_the_new_record_lacks`, `values_of_another_type_are_ignored_when_reading`, `removing_deletes_the_record_and_its_empty_parent_only`, `a_record_outside_the_owned_registry_paths_is_refused`. 5 test không đạt ở các assertion so với stub
- Đỏ: E1 phần sót lại của một lần đăng ký thất bại (hoàn tác (rollback) riêng của TIP bỏ qua lỗi): Outcome::Partial giả và các ghi đè theo từng DLL; `cargo test --locked -p raumakey-controller --test install` không đạt ở `entries_left_by_a_failed_registration_are_removed`, `entries_left_by_a_failed_registration_that_stay_make_the_rollback_incomplete`, `a_failed_registration_over_another_dll_registers_that_dll_again`. view của TIP có regsvr32 thất bại không được đọc lại và cũng không được khôi phục
- Đỏ: F1 test chặn panic (panic guard) (cài một hook im lặng, binary test riêng) được viết trước khi có run_guarded `cargo test --locked -p raumakey-controller --test panic_guard` không đạt ở `panic_guard (compile error)`. E0432, không phân giải được import raumakey_controller::run_guarded
- Đỏ: G1 chạy i686 dưới WOW64 `cargo test --locked -p raumakey-controller -p raumakey-windows --target i686-pc-windows-msvc` không đạt ở `install (test binary did not start)`. 'The requested operation requires elevation.
- Đỏ: H1 một lần tra cứu task bị từ chối với lỗi access denied phải khởi động lại ở chế độ nâng quyền; `cargo test --locked -p raumakey-controller --test startup` không đạt ở `a_task_that_only_administrators_can_read_is_handled_elevated`. một lần tra cứu bị access denied lại bị báo là thất bại thay vì khởi động lại ở chế độ nâng quyền
- Đỏ: M các kiểm tra đột biến (mutation) (scratchpad/mutate.sh: thay thế bằng perl, chạy các test tương ứng, khôi phục rồi so sánh SHA-256) `bash <scratch> <file> <subst> --test <suite>` không đạt ở `M1 no second-restart guard: an_elevated_restart_that_is_still_not_elevated_never_restarts_again (install and startup)`, `M2 same-DLL branch disabled: a_registration_from_the_same_dll_is_kept_by_the_rollback`, `M3 rollback first-to-last: a_failed_verification_undoes_every_registration_in_reverse_order, a_failed_upgrade_restores_the_previous_installation`, `M5 shortcut removed before task creation: the_elevated_task_replaces_the_shortcut, a_failed_task_creation_keeps_the_shortcut`, `M6 verification always passes: a_failed_verification_undoes_every_registration_in_reverse_order`, `M8 no cleanup after a failed registration: the three entries_left/over_another_dll tests`, `M9 record deleted after a failed uninstall: a_failed_unregistration_keeps_the_record_for_the_next_attempt, uninstall_without_a_tip_dll_fails_and_keeps_the_record`, `M10 values may start with --: missing_values_are_usage_errors`. mọi đột biến đều bị test dự định của nó bắt được;

Triển khai, bước 2: 13 lần chạy đỏ và 6 lần chạy xanh.

- Đỏ: A1 wrapper UIPI và WM_CLOSE/WM_ENDSESSION thuần: các test của lớp window được viết trước `cargo test --locked -p raumakey-windows --test layer window::` không đạt ở `layer (compile error)`. E0432, không phân giải được các import raumakey_windows::window::{WM_CLOSE, WM_ENDSESSION};
- Đỏ: A2 bước tăng dần đầu tiên: các hằng số cùng một wrapper gọi ChangeWindowMessageFilterEx mà không kiểm tra message `cargo test --locked -p raumakey-windows --test layer window::` không đạt ở `window::win::system_and_class_messages_are_never_let_through_from_lower_integrity`. unwrap_err trên Ok: ChangeWindowMessageFilterEx đã chấp nhận WM_NULL trên một cửa sổ test không nâng quyền, nên các message hệ thống phải bị từ chối trước lời gọi
- Đỏ: B1 bộ giải mã callback NOTIFYICON_VERSION_4 thuần: các test của lớp shell được viết trước `cargo test --locked -p raumakey-windows --test layer shell::` không đạt ở `layer (compile error)`. E0432, không phân giải được các import NIN_SELECT, NIN_KEYSELECT, NINF_KEY, NotifyIconEvent, NotifyIconMessage, WM_CONTEXTMENU, decode_notify_icon_message
- Đỏ: B2 các kiểu thật, bộ giải mã dạng stub trả về Other(0) cho icon 0 `cargo test --locked -p raumakey-windows --test layer shell::` không đạt ở `shell::a_click_and_the_keyboard_select_the_icon`, `shell::the_shortcut_menu_is_anchored_at_signed_screen_coordinates`, `shell::other_notifications_keep_their_code_and_every_icon_id_is_decoded`, `shell::bits_above_the_two_words_are_ignored`. các assertion so với stub
- Đỏ: C1 máy trạng thái của tray: tests/tray.rs được viết trước module (raumakey-resources được thêm làm dependency thường; `cargo test --locked -p raumakey-controller --test tray` không đạt ở `tray (compile error)`. E0432, không phân giải được import raumakey_controller::tray
- Đỏ: C2 khung với các kiểu thật và phần thân dạng stub `cargo test --locked -p raumakey-controller --test tray` không đạt ở `the_mode_starts_from_the_engine_defaults_v1`, `toggling_switches_between_the_two_modes`, `each_mode_shows_its_own_icon_and_a_tooltip_naming_raumakey`, `the_menu_offers_vietnamese_mode_as_a_check_item_and_exit`, `command_ids_are_distinct_non_zero_and_round_trip`, `start_adds_the_icon_with_the_current_mode_once`, `a_click_or_the_keyboard_switches_the_mode_and_its_icon`, `a_right_click_opens_the_menu_at_its_anchor`, và 12 test khác. 20 trên 21 test không đạt ở các assertion;
- Đỏ: D1 trình tự chạy qua một port Desktop và dispatch Command::Run: FakeDesktop, tests/run.rs, các test commands đã thay đổi được viết trước `cargo test --locked -p raumakey-controller --no-fail-fast` không đạt ở `run (compile error)`, `commands (compile error)`, `binary, convert, panic_guard, registration, startup (compile error via tests/support)`. E0432, không phân giải được import raumakey_controller::run;
- Đỏ: D2 khung: trait Desktop, System::desktop, run_controller dạng stub trả về 0, dispatch giữ nguyên `cargo test --locked -p raumakey-controller --no-fail-fast --test run --test commands` không đạt ở `the_first_instance_runs_the_tray_in_the_initial_mode_and_then_releases_the_instance`, `a_second_instance_exits_quietly_with_0`, `an_instance_that_this_process_may_not_open_counts_as_running`, `a_failed_instance_check_is_reported_and_nothing_runs`, `a_tray_that_cannot_start_releases_the_instance_and_is_reported`, `the_exit_code_of_the_message_loop_is_passed_on`, `start_errors_name_raumakey_and_the_failed_operation_in_vietnamese`, `no_arguments_run_the_controller_on_the_desktop`, và 1 test khác. các assertion: không có lời gọi desktop nào, văn bản lỗi rỗng, và Command::Run vẫn in 'not available' với mã thoát 2
- Đỏ: F1 test cấp tiến trình cho lần khởi động thứ hai (test giữ mutex single-instance thật) với desktop Windows dạng stub `cargo test --locked -p raumakey-controller --test binary` không đạt ở `a_second_start_exits_at_once_with_0_and_says_nothing`. mã thoát Some(1) thay vì Some(0): WinDesktop dạng stub thất bại ở bước kiểm tra instance
- Đỏ: E1 các test phần kết nối (glue) của tray (tests/desktop.rs: cửa sổ ẩn và vòng lặp thật, shell giả, watchdog) và unit test nạp icon riêng của binary được viết trước `cargo test --locked -p raumakey-controller --no-fail-fast --test desktop --bin raumakey` không đạt ở `desktop (compile error)`, `raumakey bin test (compile error)`. E0432, không phân giải được các import desktop::{ModeIcons, TrayShell, create_tray_window}
- Đỏ: E2 khung: handler bỏ qua mọi message, không có START, ModeIcons::load trả về một lỗi stub `cargo test --locked -p raumakey-controller --no-fail-fast --test desktop --bin raumakey` không đạt ở `start_shows_the_icon_a_click_switches_it_and_exit_from_the_menu_ends_the_loop_with_0`, `matrix_58_taskbar_created_adds_the_icon_again_with_the_same_mode`, `wm_close_ends_the_loop_like_exit_and_does_not_destroy_the_window`, `the_end_of_the_session_ends_the_loop_like_exit`, `choosing_vietnamese_mode_in_the_menu_switches_the_icon`, `a_window_destroyed_while_the_menu_is_open_still_removes_the_icon_and_ends_the_loop`, `an_elevated_tray_window_lets_the_shell_messages_through`, `the_mode_icons_are_not_in_a_test_executable`, và 1 test khác. 7 test vòng lặp kết thúc qua watchdog 20 s (mã -77, không phải 0), nên một tray bị hỏng không thể làm treo bộ test;
- Đỏ: G1 test usage: phần mô tả khi không có tham số phải mô tả controller, không phải 'Not available' `cargo test --locked -p raumakey-controller --test cli usage` không đạt ở `the_usage_describes_the_controller_without_arguments`. USAGE vẫn ghi 'Runs the Raumakey controller (tray icon and panel).
- Đỏ: M các kiểm tra đột biến (scratchpad/mutate-step2.sh: thay thế bằng perl, chạy các test tương ứng với --no-fail-fast, khôi phục từ một bản sao lưu bên ngoài worktree,… `bash <scratch> <name> <file> <subst> <cargo test args>` không đạt ở `M1 TaskbarCreated shows the toggled mode: matrix_58 tests in tray and desktop`, `M2 exit twice: exit_from_the_menu_ends_the_tray_and_nothing_brings_the_icon_back`, `M3 any icon id: other_notifications_and_other_icons_change_nothing`, `M4 cancelled WM_ENDSESSION exits: a_cancelled_end_of_the_session_changes_nothing`, `M5 WM_CLOSE left to DefWindowProcW: wm_close_ends_the_loop_like_exit_and_does_not_destroy_the_window, close_and_the_end_of_the_session_exit_like_the_menu (+2)`, `M6 drop does not end the loop: a_window_destroyed_while_the_menu_is_open_still_removes_the_icon_and_ends_the_loop (watchdog)`, `M7 Exit keeps the icon: 6 desktop tests`, `M8 access denied not taken as running: an_instance_that_this_process_may_not_open_counts_as_running`, và 7 mục khác. mọi đột biến đều bị các test dự định của nó bắt được;

Vòng sửa đầu tiên: 10 lần chạy đỏ và 8 lần chạy xanh.

- Đỏ: phát hiện 9 đỏ: tái hiện tranh chấp (race) trên registry (record_store của controller) `for i in 1..150: target/debug/deps/record_store-<hash>.exe --test-threads=8` không đạt ở `removing_deletes_the_record_and_its_empty_parent_only`. 4/150 lần chạy không đạt: RegCreateKeyExW 0x80070002 / 0x800703FA (ERROR_KEY_DELETED) trong khi một TestKey::drop khác xoá Software\Raumakey rỗng
- Đỏ: phát hiện 9 đỏ: cùng race đó trong các test registry của layer raumakey-windows `for i in 1..150: target/debug/deps/layer-<hash>.exe registry:: --test-threads=8` không đạt ở `registry::values_round_trip_through_an_owned_key`. 1/150 lần chạy không đạt: RegCreateKeyExW 0x800703FA
- Đỏ: phát hiện 13 đỏ: lần khởi động lại nâng quyền bị hết thời gian chờ thì phải thoát với mã 3 `cargo test --locked -p raumakey-controller --test registration elevated_restart` không đạt ở `an_elevated_restart_that_did_not_finish_leaves_the_outcome_unknown`. mã thoát 1, mong đợi 3 (EXIT_INCOMPLETE)
- Đỏ: kiểm tra đột biến cho phát hiện 5: nhánh lỗi khởi động lại tổng quát `mutate elevation.rs generic Err branch to EXIT_INCOMPLETE; cargo test --locked -p raumakey-controller --test registration cannot_start; restore` không đạt ở `an_elevated_restart_that_cannot_start_changes_nothing`. mã 3 so với 1 dưới đột biến;
- Đỏ: lô B đỏ (các phát hiện 6, 11, 15 phần cảnh báo của install, 17, 18): các kết quả giả mới Removed/Unread, các đường dẫn không được bảo vệ, phương thức trait dạng stub giữ hành vi cũ `cargo test --locked -p raumakey-controller --test registration` không đạt ở `a_missing_tip_file_is_found_before_asking_for_administrator_rights`, `a_registration_again_that_changed_nothing_leaves_the_rollback_incomplete`, `a_registration_whose_exit_code_was_lost_is_read_back_and_undone`, `an_unregistration_that_changed_nothing_leaves_the_rollback_incomplete`, `install_warns_about_a_tip_outside_the_protected_folders`, `the_location_warning_comes_before_the_elevated_restart_and_only_once`, `uninstall_says_whether_there_was_an_installation_record`. khởi động lại qua UAC trước khi kiểm tra file (thoát 0 so với 1);
- Đỏ: các kiểm tra đột biến M1-M4 cho phát hiện 5 trên các test độ phủ của install.rs (các test này đạt với các nhánh hiện có) `python mutate.py install.rs <branch>; cargo test --locked -p raumakey-controller --test registration <test>; restore` không đạt ở `a_previous_registration_that_cannot_be_restored_leaves_the_rollback_incomplete`, `a_failed_registration_that_left_the_previous_one_intact_needs_no_cleanup`, `a_failed_registration_that_removed_the_same_dlls_registration_leaves_the_rollback_incomplete`, `an_unregistration_that_cannot_run_keeps_the_record`. mỗi đột biến (restore Ok(code) thành true;
- Đỏ: raumakey-windows đỏ: các test for_user và program_files_x86 được viết trước khi có các API `cargo test --locked -p raumakey-windows --test layer task_scheduler` không đạt ở `task_scheduler::each_account_has_its_own_task`, `task_scheduler::the_task_of_an_account_without_one_does_not_exist`, `known_folder::program_files_x86_resolves`. lỗi biên dịch: không có ElevatedLogonTask::for_user, không có known_folder::program_files_x86
- Đỏ: lô C đỏ (các phát hiện 1, 2, 10, 15 phần startup, 16, nửa startup của 5/13): API mới (trường for_user, các phương thức task theo từng tài khoản, kiểm tra chương trình, is_inside_folder… `cargo test --locked --no-fail-fast -p raumakey-controller --test startup --test cli --test commands --test location` không đạt ở `the_restarted_startup_names_the_account_that_asked`, `the_elevated_marker_is_an_option_of_the_privileged_commands`, `the_usage_text_names_every_command`, `the_usage_explains_an_unknown_outcome`, `startup_and_clipboard_conversion_are_dispatched`, `files_below_the_folder_are_inside`, `a_declined_elevation_for_the_task_keeps_the_new_shortcut`, `none_does_not_claim_success_when_the_task_could_not_be_looked_up`, và 9 test khác. --for-user chưa được parse;
- Đỏ: các kiểm tra đột biến M5-M6 cho phát hiện 5 trên các test độ phủ của startup.rs (cả hai test cũng đạt với luồng cũ) `python mutate.py startup.rs <branch>; cargo test --locked -p raumakey-controller --test startup <test>; restore` không đạt ở `a_shortcut_that_cannot_be_removed_after_the_task_was_created_is_reported`, `none_reports_a_task_that_cannot_be_deleted`. việc báo thất bại đó thành EXIT_OK khiến từng test chuyển sang đỏ;
- Đỏ: các phát hiện 3/4/14 đỏ dưới các đột biến của reviewer `M7: replace the `if elevated {..}` block with `let _ = (elevated, shell_messages);` / M8: Window::create_hidden -> create_message_only; cargo test --locked -p raumakey-controller --test desktop; restore` không đạt ở `an_elevated_tray_window_lets_the_shell_messages_through`, `the_tray_window_is_a_hidden_top_level_window`. M7: MSGFLTINFO_STATUS(0) so với ALREADYALLOWED_FORWND(1) cho TaskbarCreated 0xC10E;

Các lượt review:

- Đặc tả và mức đầy đủ của test: không được duyệt (1 major, 5 minor, 2 nit).
- Tính đúng đắn, an toàn và chất lượng mã: không được duyệt (2 major, 4 minor, 5 nit).
- Review lại sau vòng sửa đầu tiên: được duyệt (2 minor, 1 nit).

Các phát hiện minor còn mở từ lần review lại gần nhất:

- minor, `crates/raumakey-controller/tests/record_store.rs:59`: Race trong các test registry chỉ được sửa bên trong một tiến trình test.
- minor, `crates/raumakey-controller/src/cli.rs:39`: EXIT_FAILED được mô tả trong tài liệu là 'the command failed;
- nit, `crates/raumakey-windows/src/task_scheduler.rs:82`: `ElevatedLogonTask::NAME`, `new()` và impl `Default` vẫn được mô tả trong tài liệu là 'the product's task'.

### Kiểm chứng giai đoạn 2a

Sau khi merge `raumakey-tsf` và `raumakey-controller` lên trên `0bdfbcc` (kế hoạch §14.6):

- `cargo fmt --all --check`: đạt.
- `cargo xtask ci`: Clippy với `-D warnings` đạt với cả hai bộ feature; 1,887 test đạt với các
  feature mặc định và 1,961 test với các feature tương thích, 0 không đạt, 20 bị bỏ qua (các test
  thủ công và 3 test `BUG`); `fuzz_smoke` đạt 10 test.
- `cargo test --locked -p raumakey-tsf -p raumakey-controller -p raumakey-windows --target
  i686-pc-windows-msvc`: 697 test đạt dưới WOW64, 17 test thủ công bị bỏ qua.
- `cargo xtask cross` và `cross --release`: đạt cho x86_64, i686 và aarch64.
- `cargo xtask inspect` và `inspect --release`: tất cả 57 và tất cả 66 kiểm tra đều đạt.
- `cargo xtask package`, sau đó `raumakey.exe` đã đóng gói với `install --dry-run` (cả hai thứ
  tự tuỳ chọn), `uninstall --dry-run`, `--version` và `update`: thoát với mã 0, và các plan chạy
  thử nêu tên CLSID và GUID profile của Raumakey.

Chưa có ứng dụng chủ TSF nào kích hoạt TIP và chưa quan sát thấy tray nào trên desktop; các danh
sách kiểm tra thủ công trong `raumakey_tip::server` và trong module `desktop` của controller vẫn
chưa được chạy.

## Các quyết định D25, D27, D28–D30, D34

Được triển khai ngày 2026-10-17 trong hai worktree tách từ `d378f71` sau các quyết định của chủ
sản phẩm (kế hoạch §15), mỗi worktree được một reviewer độc lập review và được sửa cho tới khi
được duyệt.

### Engine, macro, lõi session của TIP, vector (D25, D28, D29, D30, D34)

- Đỏ, D29: bốn test không đạt vì trigger dài 33 scalar vẫn được chấp nhận
  (`d29_a_trigger_has_at_most_32_scalars`, `d29_a_trigger_longer_than_32_scalars_is_an_import_issue`,
  `d29_creating_a_macro_with_a_trigger_longer_than_32_scalars_fails`,
  `d29_the_longest_trigger_expands_with_the_default_context_and_a_longer_one_is_rejected`).
- Đỏ, D30: sáu test không đạt, trong đó có
  `d30_a_replacement_fits_one_insert_in_nfc_and_in_nfd_with_a_delimiter` (chấp nhận 4,096 emoji),
  `d30_adaptation_that_would_exceed_the_insert_limit_falls_back_to_the_stored_text`, và
  `d47_d53_every_accepted_replacement_expands_at_every_boundary`.
- Đỏ, D28: lỗi biên dịch do thiếu các truy vấn token, sau đó không đạt ở 12 trên 15 test trong
  `crates/raumakey-engine/tests/tokens.rs`, `table::d28_token_triggers_and_trigger_prefixes`,
  `macro_engine::d47_an_accepted_trigger_with_boundary_punctuation_expands`, và
  `macros::d28_a_trigger_across_punctuation_expands_as_one_token`.
- D34: `d34_circumflex_without_spell_check_marks_the_rightmost_a_e_or_o` cố định hành vi hiện có;
  việc tạm thời đảo ngược thứ tự ứng viên đã làm nó không đạt (`vni oa6` cho ra `ôa`).
- Đỏ, D25: `matrix::b29_secure_fields_never_expand_macros`,
  `options::secure_field_keys_are_processed_without_macros`,
  `config::d25_restricted_mode_transforms_secure_fields_without_macros`,
  `macro_engine::d25_no_macro_expands_for_a_key_in_a_secure_field`, và gate của session
  `gates::d25_bypass_never_lets_the_engine_edit_a_secure_field` (một Backspace đã sửa ô nhập).
- Review: được duyệt với ba phát hiện minor. Một lần chạy đối chiếu (differential) 6,000 script
  ngẫu nhiên trên 15 bộ tuỳ chọn với một bảng không có trigger dấu câu cho ra các trace giống hệt
  từng byte so với engine trước đó, và 24,000 script với trigger token giữ được mọi bất biến về
  quyền sở hữu. Vòng sửa đã thêm các test mà ba đột biến sống sót làm lộ ra, làm cho một Backspace
  xoá dấu câu của một token đang được theo dõi khôi phục từ đứng trước nó (đỏ:
  `d28_backspace_over_a_tracked_tokens_punctuation_restores_the_word_before_it` và
  `macros::d28_a_backspace_over_the_dot_restores_the_word_before_it`), ghi tài liệu cho việc nạp
  dễ dãi (lenient) dữ liệu được lưu theo các giới hạn cũ, và mở rộng tham chiếu fuzz `macro_files`
  sang các truy vấn token. Lần review lại thứ hai đã duyệt thay đổi về Backspace: một oracle khôi
  phục chính xác trên 3,656 trường hợp đều khớp, và bốn đột biến của phần khôi phục đều bị bắt.
  Phát hiện minor mới duy nhất của lần review đó, rằng một kho được nạp dễ dãi có thể cấp cho
  một macro mới mã định danh của một mục đã bị bỏ qua, đã được sửa trong checkout chính (đỏ:
  `d29_d30_new_ids_never_reuse_an_entry_skipped_by_lenient_loading`, `next_id` trả về 4 thay vì 6).
- Các kỳ vọng được cố ý thay đổi: ba test `BUG` đã trở thành test đạt; các test matrix #29 riêng
  của engine giờ khẳng định "không có macro trong ô bảo mật" (phần bypass đã chuyển sang các
  adapter); các đầu vào mà D30 làm cho không hợp lệ đã được rút ngắn.

### Bảo mật pipe điều khiển (D27)

- Đỏ: các test descriptor mới không biên dịch được; test định danh tiến trình không đạt với
  `E_ACCESSDENIED` từ `OpenProcessToken` khi đang mạo danh (impersonate) một token mức toàn vẹn
  thấp (low integrity); các test pipe không đạt vì các client mức toàn vẹn thấp và AppContainer
  bị từ chối và descriptor không có ACE cho AppContainer.
- Review: ban đầu không được duyệt. Nhãn bắt buộc mức thấp (low mandatory label) được yêu cầu đã
  trao cho mọi tiến trình mức toàn vẹn thấp của người dùng toàn quyền điều khiển pipe (một probe
  đã thay DACL của pipe và giả làm controller), trong khi các client AppContainer kết nối được mà
  không cần nhãn đó. Bản sửa đã bỏ nhãn, thêm việc phía client từ chối các pipe có nhãn thấp hơn
  mức trung bình (medium) (đỏ: cả hai test chiếm chỗ (squatter) đều kết nối được), làm cho một
  test harness dừng server khi client panic (đỏ: test bị treo cho tới khi bị kill), và cố định
  việc parse nhãn bằng một pipe có nhãn mức trung bình. D27 trong kế hoạch §15 ghi lại sửa đổi
  này. Lần review lại đã duyệt nó.

## Bản dùng thử, README tiếng Việt và sửa script cài đặt

Ngày 2026-10-17, chủ dự án yêu cầu một bản build để dùng thử, một file hướng dẫn build, và chuyển
mọi tài liệu sang tiếng Việt. Phần mã thay đổi `xtask`, bộ test COM của TSF và một comment của
controller.

### README của package (D33)

- Đỏ: `readmes_state_unsigned_and_verification_level` được sửa để đòi README tiếng Việt (UTF-8 có
  BOM, CRLF, các cụm "chưa được ký số", "chưa kiểm chứng khi chạy thật", "Ctrl+Shift",
  "GUIDE.md") và fail tại `xtask\src\package.rs:588` với `UTF-8 byte order mark`.
- Xanh: `readme` sinh văn bản tiếng Việt có BOM; `assert_windows_text` (ASCII) vẫn chỉ áp cho
  script.

### Script cài đặt (§14.4 mục 8)

- Lỗi khi chạy thật: trên package release, `powershell -NoProfile -ExecutionPolicy Bypass -File
  install.ps1 -DryRun` (Windows PowerShell 5.1.26100) thoát với mã 1 và báo `The variable
  '$LASTEXITCODE' cannot be retrieved because it has not been set.` Nguyên nhân: script gọi
  `raumakey.exe`, một chương trình subsystem GUI, bằng `& $controller`; PowerShell 5.1 không chờ
  chương trình GUI và không đặt mã thoát. `raumakey.exe install --dry-run` chạy trực tiếp thì đạt,
  và `installer-dry-run` của cổng phát hành chỉ chạy controller trực tiếp, nên lỗi lọt qua.
- Đỏ: `dry_runs_also_run_the_packaged_scripts_with_windows_powershell` không biên dịch được
  (`E0425`: chưa có `script_dry_run_commands`). Sau khi thêm hàm đó,
  `install_script_self_elevates_and_passes_both_tips` và
  `uninstall_script_self_elevates_without_tip_paths` fail tại `xtask\src\package.rs:596` (script
  thiếu `System.Diagnostics.ProcessStartInfo`).
- Xanh: script khởi động controller bằng `ProcessStartInfo` (không dùng shell, đầu ra chuẩn và
  lỗi chuẩn được chuyển hướng theo UTF-8), đọc lỗi chuẩn bất đồng bộ để tránh deadlock, chờ
  controller kết thúc rồi lấy `ExitCode`; `Join-Arguments` đặt dấu ngoặc kép cho từng tham số theo
  quy tắc của `CommandLineToArgvW`. `installer-dry-run` chạy thêm `install.ps1 -DryRun` và
  `uninstall.ps1 -DryRun` bằng `powershell.exe` với cùng hợp đồng đầu ra. 103 test của `xtask` đạt.
- Kiểm tra bằng tay trên package release: `install.ps1 -DryRun` và `uninstall.ps1 -DryRun` thoát
  với mã 0 và in đầy đủ kế hoạch, kể cả khi package nằm trong thư mục có dấu cách và chữ tiếng
  Việt (`Thư mục thử`).
- Chưa chạy: lần cài thật qua UAC (cần chủ dự án), và `cargo xtask release-gate` (cần file review
  toàn branch).

### Review

- Một reviewer độc lập đã duyệt, không có phát hiện blocker hay major, và nêu 11 phát hiện
  minor/nit. Reviewer đã chạy lại hai script `-DryRun` bằng Windows PowerShell 5.1, thử mã thoát
  khác 0, thử trường hợp thiếu `raumakey.exe`, và cho `Join-Arguments` chạy khứ hồi qua
  `CommandLineToArgvW` (đường dẫn có dấu cách, chữ tiếng Việt, dấu `\` ở cuối, dấu ngoặc kép bên
  trong, chuỗi rỗng).
- Đã sửa:
  1. Script kiểm tra `raumakey.exe` trước khi xin quyền quản trị. Lỗi khi chạy controller được in
     ra, và cửa sổ quản trị viên vẫn chờ Enter (`try`/`catch`/`finally`).
  2. Có test chốt `ReadToEndAsync()`.
  3. `installer_dry_run_commands` trả về cả bốn lần chạy và có test riêng.
  4. `powershell.exe` được lấy từ `%SystemRoot%\System32\WindowsPowerShell\v1.0`, không tìm theo
     search path (search path bắt đầu bằng thư mục của `xtask.exe`).
  5. Regex dùng `\z` thay cho `$` (`$` của .NET còn khớp trước ký tự xuống dòng cuối).
  6. Comment trong `crates/raumakey-controller/src/lib.rs` không còn nói rằng
     `$out = & raumakey.exe` chờ được controller.
  7. `GUIDE.md` được commit cùng thay đổi này.
  8. README ghi thêm đường dẫn Settings của Windows 10, và một bước kiểm tra hàng phím số (D36).
- Chấp nhận, không sửa:
  - Lỗi chuẩn được in sau đầu ra chuẩn nên thứ tự giữa hai luồng bị mất. Điều này được ghi trong
    comment của script.
  - Khi đầu ra của `powershell.exe` bị chuyển hướng, `Write-Host` mã hoá theo code page của console.
    Ký tự không phải ASCII trong đường dẫn có thể thành `?` trong log của gate, nhưng hợp đồng GUID
    không bị ảnh hưởng.
  - Reviewer đề xuất gate chạy script từ một thư mục có dấu cách và chữ tiếng Việt. Việc này chưa
    làm; trường hợp đó chỉ được kiểm tra bằng tay.
- Đỏ lần hai: `the_installer_dry_run_runs_the_controller_then_both_scripts_with_windows_powershell`
  không biên dịch được (`E0425`: chưa có `installer_dry_run_commands`). Sau khi thêm hàm đó, hai
  test script fail tại `xtask\src\package.rs:631` và test README fail tại
  `xtask\src\package.rs:685`. Xanh: 103 test của `xtask`, Clippy sạch.
- Kiểm tra bằng tay sau khi sửa: hai script `-DryRun` thoát với mã 0. Chạy từ `Thư mục thử`, đầu ra
  có đủ 20 dòng CLSID. Khi xoá `raumakey.exe`, `install.ps1` (không có `-DryRun`) báo
  `raumakey.exe was not found next to this script` và thoát với mã 1 trước khi hỏi quyền quản trị.

### Bộ test COM của TSF và Caps Lock (§14.4 mục 9)

- Lỗi: lần chạy `cargo xtask ci` thứ hai fail 37 test trong `crates/raumakey-tsf/tests/com`, dù
  crate TSF không đổi so với lần chạy đạt trước đó. Commit `56980c7` cũng fail y hệt, chạy trong
  một worktree riêng, nên lỗi do trạng thái của máy. Chạy riêng từng test thì 14 test fail; ví dụ
  `typing::telex_words_are_typed_through_real_tsf_ranges` cho ra `tieengs vieetj nguƯoƯif`. Các
  test còn lại fail theo dây chuyền, vì một test panic để lại một object của server.
- Nguyên nhân: Caps Lock của desktop đang bật (`GetKeyState(VK_CAPITAL)` = 1). Một luồng mới nhận
  trạng thái các phím bật/tắt của desktop. Text service dịch phím bằng `ToUnicodeEx` với trạng
  thái đó nên ra chữ hoa, còn `Document::app_types` chèn chữ thường như test viết. Hai văn bản
  không khớp, nên text service không sửa văn bản và cho phím đi qua. Trong ứng dụng thật, ứng dụng
  cũng chèn chữ hoa khi Caps Lock bật, nên đây là giả định của bộ test, không phải lỗi của TIP.
- Sửa: `Tip::activate_after` gọi `release_every_key`, hàm này dùng `SetKeyboardState` để thả mọi
  phím và tắt mọi phím bật/tắt của luồng test. Hàm chỉ đổi trạng thái của luồng gọi, không gửi
  input nào. Đỏ: 14 test fail khi chạy riêng (Caps Lock bật). Xanh: cả 47 test đạt hai lần liên
  tiếp, trong khi Caps Lock của desktop vẫn bật.

## Bản portable một tệp (D38–D42)

Ngày 2026-10-17 và 2026-10-18, chủ dự án quyết định bỏ cách cài đặt qua TIP và chuyển sang một tệp
`raumakey.exe` portable (thiết kế §21, kế hoạch §15 D38–D42 và §16). Mỗi task được làm trong một Git
worktree riêng, được một reviewer độc lập review, sửa theo review, rồi merge.

### P1: crate `raumakey-session`

- Chuyển nguyên `crates/raumakey-tsf/src/session/` và `tests/session/` sang crate mới; TIP re-export
  crate đó làm `session`. Đây là một lần chuyển chỗ, không thêm hành vi, nên không có lần chạy đỏ;
  thay vào đó, số test trước và sau được so: 1,955 / 2,029 cả hai lần.
- `EditRequest::new` thành `pub` vì một test của TIP dùng nó. Crate mới có feature `evkey-compat`
  riêng, nằm trong danh sách feature của `xtask ci`.

### P2: cấu hình cạnh exe (D40)

- Đỏ: `data_root` không biên dịch được (chưa có `resolve_with`, `is_exe_dir`, `is_writable_dir`);
  bốn test tên tệp fail (`config.toml` khác `raumakey.toml`, …); 7 test `atomic_store`, 1 test
  `store_robustness` và 9 test của `raumakey-migrate` fail vì tên tệp; `settings` không biên dịch
  được.
- Review: cần sửa, 2 lỗi lớn.
  1. Thư mục được chọn theo việc lần chạy có quyền quản trị hay không, nên một bản trong Program
     Files dùng hai cấu hình khác nhau.
  2. Một lần đọc thất bại tạm thời trong lúc poll làm trạng thái kẹt ở `Unreadable`, và
     `recover()` khi đó chuyển một tệp tốt sang `.bak`.

  Ngoài ra còn các lỗi nhỏ: exe không rõ thư mục thì chạy chỉ trong bộ nhớ; tệp thăm dò có thể sót
  lại; trạng thái cũ sau khi commit; tranh chấp ở lần chạy đầu.
- Sửa:
  - Quy tắc chọn thư mục mới: có sẵn `raumakey.toml` thì dùng thư mục exe; nếu không, chỉ dùng thư
    mục exe khi ghi được và không thuộc Program Files, thư mục Windows hay `%TEMP%`.
  - Đọc lại sau lỗi đọc.
  - `Recovery` thay cho kiểu trả về cũ của `recover()`.
  - Tệp thăm dò có tên cố định.

  Đỏ: 3 test `data_root`, 21 lỗi biên dịch E0061, 3 test `settings`, 1 test unit tranh chấp lần
  chạy đầu. Xanh: 2,001 / 2,075 trong worktree.

### P3: package portable (D38)

- Đỏ: E0425 (`package_paths`); 7 test package; 3 test gate, evidence và CLI; 18 lỗi biên dịch của
  các test gate mới.
- `wix.rs` bị xoá. `installer-dry-run` được thay bằng `portable-check`, bước này kiểm tra danh sách
  tệp, chạy `raumakey.exe --version`, rồi quét lại package.

### P4: gõ bằng hook (D39)

- Đỏ:
  - `raumakey-session` không biên dịch được (`Role`, `with_role`); sau khi thêm khung API, 6/10 test
    vai trò hook fail;
  - `raumakey-windows` không biên dịch được (`foreground_window`, `key_down_input`, `key_is_down`);
  - test `typing` không biên dịch được.

  Hai lần chạy đột biến (mutation) làm fail 11 và 15 test.
- Review: cần sửa, 2 lỗi lớn, cả hai đã được tái hiện.
  1. Sau Win+L hoặc Ctrl+Alt+Del, bộ phân loại phím tắt vẫn coi phím bổ trợ đang bị giữ, nên
     Ctrl+Shift không còn tác dụng.
  2. Caps Lock đổi trong lúc hook không được gọi (màn hình khoá, cửa sổ quyền cao hơn) không được
     đọc lại, nên ra `xin TIếng`.

  Ngoài ra còn các lỗi nhỏ: phím bổ trợ bị lỡ làm Backspace thành Ctrl+Backspace; Windows âm thầm
  gỡ hook; Alt nhấn một mình đưa cửa sổ vào chế độ menu; bộ gõ CJK; phát lại một phím khác.
- Sửa:
  - `ServiceCore::transitions_missed`;
  - đồng bộ phím bổ trợ từ `GetAsyncKeyState`;
  - đọc lại Caps Lock khi đổi tài liệu và sau 1 giây không có sự kiện;
  - `TypingHook::ensure_alive`;
  - Alt nhấn một mình và F10 kết thúc từ;
  - CJK cho đi thẳng.

  Đỏ: 3 lỗi biên dịch E0599; 2 test session; lỗi biên dịch của `Heartbeat` và `last_input_tick`;
  3 test lib và 19 test `typing` với stub. Xanh: 2,078 / 2,152 trong worktree.

### P5: chế độ chạy portable (D40–D42, D41)

- Đỏ:
  - API timer và Notepad của `raumakey-windows` chưa có;
  - test `worker`, `notice` và `tray` không biên dịch được (138 lỗi);
  - `run.rs` không build được với trình tự cũ;
  - 2 test help của CLI fail;
  - 3 test `worker` fail vì một câu trả lời không mang settings làm mất lần nạp lại.
- Người phụ trách (lead) thêm `TrayServices::keep_typing_alive`, gọi `ensure_alive` ở mỗi nhịp
  timer. Đỏ: `E0407` (phương thức không thuộc trait). Xanh: 12 test `desktop`.
- Smoke test của exe thật (package release, Windows 11 26H2), với hook mức thấp chạy trên máy phát
  triển trong vài giây:
  - Chạy với một `raumakey.toml` có sẵn ở chế độ tiếng Anh: tiến trình sống, tệp không đổi, và
    `taskkill /PID` (WM_CLOSE) cho mã thoát 0.
  - Lần chạy đầu trong một thư mục trống: `raumakey.toml` và `raumakey.toml.lock` được tạo cạnh exe
    với cấu hình mặc định, còn `%LOCALAPPDATA%\Raumakey` không được tạo.
  - Lỗi phát hiện: trong lúc hộp thoại chào đang mở, WM_CLOSE không làm tiến trình thoát.
- Review: cần sửa, 3 lỗi lớn.
  1. Tiến trình có thể chạy mà không có icon. Lần thêm icon thất bại không được thử lại, và
     `TaskbarCreated` bị mất khi một hộp thoại đang mở.
  2. Lệnh `startup` và menu khay không thống nhất `[system] startup`, nên tray làm ngược lại lệnh.
  3. Shortcut có thể trỏ tới một exe trong `%TEMP%`, và lần sửa shortcut có thể lấy shortcut của
     một bản khác.

  Ngoài ra còn các lỗi nhỏ: `WM_ENDSESSION` bị mất khi hộp thoại mở; lưu thất bại không được thử
  lại; câu báo lỗi lưu khởi động; sửa tay `startup`; tệp đang ghi dở khi poll.
- Sửa: hộp thoại chạy trên luồng riêng (`spawn_message_box`), câu trả lời đi qua inbox, và thoát
  thì đóng các hộp thoại đang mở; thêm lại icon ở mỗi nhịp timer khi lần thêm hoặc đổi thất bại;
  `raumakey startup <mode>` ghi chế độ qua `Settings`; không tạo shortcut cho exe trong `%TEMP%`,
  chỉ sửa shortcut khi nó mất hoặc trỏ tới một exe không còn, không bao giờ trỏ lại shortcut của một
  bản khác; sửa tay `startup` cũng tạo hoặc xoá shortcut; thay đổi chưa lưu được thì thử lại ở mỗi
  lần poll và khi thoát; đọc lại tệp hỏng sau 300 ms; gộp các sự kiện phím tắt và lưu chúng trước
  khi thoát. Đỏ: API hộp thoại của `raumakey-windows`; 14 lỗi biên dịch của `settings`
  (`has_unsaved`, `save_unsaved`, `is_temporary_path`); 14 lỗi của `worker`; `run.rs` không build
  được; `temporary_exe_notice`; 18 lỗi của `tray` (`coalesce`, `flush_hotkeys`,
  `Event::Answered`); 2 assertion của `commands` (chế độ chưa được ghi, không có thông báo stderr).
- Smoke test sau khi sửa: lần chạy đầu tạo `raumakey.toml` cạnh exe; khi hộp thoại chào còn mở,
  WM_CLOSE gửi tới cửa sổ khay cho mã thoát 0. `taskkill /PID` (không `/F`) chỉ gửi WM_CLOSE tới
  cửa sổ đang hiện, tức là hộp thoại, nên lần smoke test trước đó chỉ đóng hộp thoại; đây là cách
  `taskkill` hoạt động, không phải lỗi của Raumakey.
- Bằng chứng của bản sửa đổi `4278a32` nằm ở kế hoạch §16.2.

## Bảng cấu hình và danh sách gõ tắt (D43–D45)

Ngày 2026-10-18, chủ dự án yêu cầu một hộp thoại để chọn cấu hình thay vì chỉ sửa tệp, kèm ảnh
chụp giao diện EVKey để minh hoạ. Ảnh chỉ được dùng để hiểu nhu cầu; bố cục, câu chữ và icon do
Raumakey tự thiết kế (thiết kế §22, kế hoạch §15 D43–D45 và §17). Mỗi task được làm trong một Git
worktree riêng, được một reviewer độc lập review, sửa theo review, rồi merge. Trong suốt đợt này,
bản `raumakey.exe` của chủ dự án đang chạy trên máy phát triển và giữ mutex single-instance, nên
test `tests/binary.rs::a_second_start_exits_at_once_with_0_and_says_nothing` không chạy được; không
agent nào tắt tiến trình đó.

### S1: khối dựng hộp thoại (`raumakey-windows`)

- Đỏ: E0432 cho `dialog_template`, `dialog_box`, `names::message_name`,
  `window::is_broadcastable`, `register_message`, `register_user_message`,
  `post_to_top_level_windows`, `allow_any_foreground`, `icon::icon_size_for_window`.
- Lần chạy thật cho thấy Windows **cho phép** mọi tiến trình post `WM_NOTIFY`. Một `WM_NOTIFY` giả
  mạo với con trỏ hỏng làm sập tiến trình test (`0xC000041D`). Thủ tục hộp thoại giờ chép `NMHDR`
  bằng `ReadProcessMemory` và chỉ chuyển tiếp khi nó chỉ đúng một control con của hộp thoại.
- Review: cần sửa. Không có lỗi an toàn bộ nhớ.
  1. Một `WM_DESTROY` giả mạo biến hộp thoại thành "zombie": vẫn hiện nhưng không đóng được, và
     `dismiss()` không còn tác dụng.
  2. Template nhận chữ bắt đầu bằng U+FFFF, khiến Windows đọc nó như một ordinal và đọc lố bộ đệm.
  3. Nạp 20.000 dòng vào list view mất khoảng 3 giây (bản release).

  Ngoài ra còn các lỗi nhỏ: thông điệp dành riêng (đóng, đưa lên trước) nhận từ bất kỳ tiến trình
  nào; test đòi tuyệt đối không có notification nào; một test thật sự đổi quyền foreground của
  desktop; chỉ số lớn bị cắt khi chọn mục.
- Sửa: chỉ coi `WM_DESTROY` là thật khi hộp thoại đã ẩn, và giữ handle tới khi
  `DialogBoxIndirectParamW` trả về; cookie ngẫu nhiên cho từng hộp thoại trong `wParam` của thông
  điệp dành riêng; `build()` từ chối chữ bắt đầu bằng U+FFFF và cỡ font 0x7FFF; `i32::try_from`
  cho chỉ số; giữ kết quả khi `Drop` của handler panic. Đỏ: 3 test `WM_DESTROY` giả mạo (handle
  vẫn được công bố, `dismiss` hết giờ, hộp thoại cha không đóng), 2 test template (`build` trả về
  `Ok`), test cookie (hộp thoại bị đóng bởi thông điệp giả), test chỉ số lớn, test kết quả khi
  `Drop` panic. Vấn đề tốc độ của list view được chuyển cho S3b.
- Còn lại: một `WM_NCDESTROY` giả mạo vẫn đi vào xử lý mặc định của dialog manager, cùng loại với
  giới hạn mà `raumakey_windows::window` đã ghi cho cửa sổ thường.

### S2: bảng cấu hình (`raumakey-controller`)

- Đỏ: 12 × E0599 (các `Change` mới); 2 test `with_change` fail với nhánh rỗng; 3 × E0609
  (`SettingsState::show_panel_on_start`); 25 × E0061 (`start_notice` vẫn nhận một tham số); E0432
  (`panel`); 70 lỗi biên dịch của test `tray`; E0407 và E0560/E0609 của test `run`; E0599
  `Dialog::move_to`; 4 × E0407 của test `desktop`; phần nối danh sách gõ tắt: 11 lỗi của `tray`,
  5 lỗi của `desktop`.
- Quyết định khác với brief: lần mở bảng được đánh số (`OpenPanel { serial }`,
  `PanelClosed(serial)`), để một lần đóng tới muộn không làm tray ngừng cập nhật bảng mới; mục "Kiểu
  gõ riêng" hiện khi phương thức đang dùng là kiểu riêng hoặc snapshot mang key map của
  `user_keymap`; `Change::Startup` gửi từ bảng không làm gì, chỉ `PanelRequest::Startup` mới đổi
  shortcut.
- Review: cần sửa.
  1. Dòng cuối của bảng cắt mất tên tệp khi cấu hình nằm ở `%LOCALAPPDATA%`: `SS_PATHELLIPSIS` giữ
     phần sau dấu `\` cuối cùng, và đó là `\Raumakey)` trong ghi chú vị trí.
  2. Hai mục của danh sách kiểm tra thủ công mô tả sai kết quả của `taskkill` không có `/F`.

  Ngoài ra còn các lỗi nhỏ: phím mũi tên đi từ một ô chọn vào danh sách kiểu gõ và đổi kiểu gõ;
  mở lại bảng đúng lúc nó đang đóng thì không có gì hiện ra; câu hỏi "Mặc định" chưa nhắc mục "Mở
  bảng này khi Raumakey chạy".
- Sửa: nhãn đường dẫn chỉ chứa đường dẫn, còn vị trí (kèm lý do dùng `%LOCALAPPDATA%`) có nhãn
  riêng; mỗi nhóm `WS_GROUP` chỉ gồm một loại control; một bảng hay hộp thoại danh sách đang đóng
  được coi là đã đóng khi mở lại; câu hỏi "Mặc định" đủ các mục được giữ; khi bảng không mở được
  lúc khởi động, nội dung chào hiện trong một hộp thông báo; danh sách kiểm tra thủ công nói đúng
  việc `taskkill` làm. Đỏ: 3 × E0432 và 8 × E0599; sau khi thêm khung, 8 test `panel`, 1 test
  `notice` và 4 test `desktop` fail (phím mũi tên từ ô 100 ra `[-1, 101]`; lần đóng tới muộn báo
  `PanelClosed(2)` thay vì `PanelClosed(1)`; hộp thoại danh sách mới không hiện trong 10 giây; nhãn
  vị trí thiếu); chỉ sửa nhóm thì dòng cuối vẫn hiện `…\raumakey.toml (trong...\Raumakey)`.
- Sau khi merge, test `the_real_macro_list_editor_opens_once_reports_saves_and_closes` fail một lần.
  Chạy song song sáu bản của bộ test `desktop` thì 17/24 lượt fail. Nguyên nhân: handle của một
  hộp thoại tách luồng vẫn được công bố tới khi `DialogBoxIndirectParamW` trả về, lâu hơn khi máy
  bận, và `PostMessageW` tới cửa sổ đã huỷ đó vẫn có thể thành công (đo được: `IsWindow` sai mà
  `PostMessageW` đúng), nên lần mở lại tưởng hộp thoại cũ còn sống. Sửa: `post`, `dismiss` và
  `bring_to_front` kiểm tra `IsWindow` trước. Sau khi sửa: 0/24 lượt fail. Không dựng được trạng
  thái này một cách xác định trong một test đơn lẻ, nên lần chạy song song là bằng chứng đỏ.

### S3a: mô hình danh sách gõ tắt (`macro_list`)

- Đỏ: E0432 (`macro_list`); 5 test giữ id, khoá lạ và hết id fail; 6 × E0599 (`overwrite`);
  E0432 cho các hàm câu chữ; một unit test cho thấy lần lưu thử lại sau một lần ghi đè thất bại
  đã ghi đè một lần sửa tay.
- Review: cần sửa, 1 lỗi đáng kể. Lần lưu đầu âm thầm làm mất những gì bộ giải mã đã bỏ qua hoặc
  đặt lại: `[macro]` viết thành một bảng đơn (danh sách mở ra rỗng rồi lần lưu xoá mục đó), một phần
  tử không phải bảng, hay `enabled = "no"`. Ngoài ra còn các lỗi nhỏ: sửa một mục mà hook không dùng
  (trùng id, hoặc trùng từ gõ tắt với mục có id nhỏ hơn); lỗi vượt giới hạn 16 MiB báo như lỗi
  đọc/ghi; vài câu chữ.
- Sửa: `lossy_issues()` cùng `accept_loss()`, và lần lưu bị từ chối (`SaveError::Lossy`) cho tới khi
  người dùng đồng ý; `Row::status()` (đang dùng, đang tắt, không dùng); `put` sửa đúng mục mà hook
  dùng; mục sửa có id trùng được cấp id mới; `SaveError::TooLarge` trước khi commit. Đỏ: E0599 cho
  `lossy_issues`, `accept_loss`, `SaveError::Lossy`, `SaveError::TooLarge`, `Row::status`; 2 test
  hành vi (trạng thái, mục được sửa); test giới hạn kích thước nhận `IoFailure`; 2 test câu chữ.

### S3b: hộp thoại danh sách gõ tắt (`macro_dialog`)

- List view ảo (`LVS_OWNERDATA`) được trả lời ngay trong thủ tục hộp thoại, nên 20.000 dòng mở và
  cập nhật gần như tức thì; tệp được đọc và ghi trên luồng của hộp thoại.
- Review: cần sửa, 1 lỗi đã tái hiện dưới Common Controls 6. Gõ một chữ cái vào danh sách chọn
  dòng 0 và ghi đè nội dung đang gõ dở trong hai ô nhập, vì `LVN_ODFINDITEMW` không được trả lời.
  Ngoài ra còn các lỗi nhỏ: `WriteProcessMemory` ghi được cả vào trang `PAGE_EXECUTE_READ`; vòng
  văn bản cho mượn dùng chung giữa các list; test đọc con trỏ đã cho mượn; đọc ô dài bị panic dưới
  Common Controls 6.
- Sửa: trả lời `LVN_ODFINDITEMW` bằng tìm theo tiền tố, không phân biệt hoa thường, có quay vòng,
  đọc chuỗi tìm qua bản chép có giới hạn; chỉ ghi vào vùng nhớ đã commit có quyền đọc-ghi
  (`VirtualQuery`); mỗi list một vòng văn bản; test chạy dưới Common Controls 6 bằng activation
  context. Đỏ: test gõ chữ (chọn 0, cần 2), yêu cầu tìm giả mạo (trả 0, cần 2), test trong
  controller (chọn 0, cần 2), test trang không ghi được (`pszText` bị đổi), test vòng văn bản
  riêng, test ô dài (panic khi cắt slice).

### S5: ngắt dòng tiếng Việt

- Ảnh chụp bảng cấu hình thật cho thấy nhãn "Giữ nguyên phím đã gõ khi từ không phải tiếng Việt"
  bị ngắt thành "…không phải ti" / "ếng Việt". Phép đo bằng `DrawTextW` (`DT_WORDBREAK`, Segoe UI
  9 pt, khung hẹp hơn chữ 4 px) xác nhận: chữ ASCII `tieng` giữ một dòng, còn `tiếng`, `Việt`,
  `không`, `nguyên`, `được`, `thư`, `phải` đều bị ngắt giữa chữ. GDI coi chỗ trước một chữ cái
  ngoài ASCII là chỗ ngắt được, như chữ Hán. Một `MessageBoxW` thật với một dòng tiếng Việt dài
  cũng ngắt "Biểu t" / "ượng". Lỗi này có từ bản portable đầu tiên, ở mọi hộp thông báo.
- Sửa: Raumakey tự ngắt dòng, chỉ ở dấu cách, đo bằng đúng font của control hoặc của hộp thông
  báo (`wrap_words`, `Dialog::set_text_wrapped`, `message_box`), nên Windows không còn gì phải tự
  ngắt. Độ rộng dòng của hộp thông báo được đo trên máy: hộp có icon giữ nguyên dòng tới 318 px và
  ngắt dòng 321 px ở 96 DPI. Raumakey dùng tối đa 300 px theo DPI, 50 ký tự trung bình, và một phần
  tư vùng làm việc của màn hình. Các mức phóng to khác chưa được đo. Ví dụ trong ngoặc của nhãn và
  hai chữ cuối của gợi ý dùng dấu cách không ngắt (U+00A0), để một dòng không kết thúc bằng "→".
- Đỏ: 6/8 test `wrap_words` và test `message_text_width` với khung rỗng; cả 7 test Windows (số
  dòng vẽ ra dưới Common Controls 5 và 6, câu tiếng Việt "274 > 180", chỗ cho ô chọn, chỉ nhận nhãn
  và ô chọn, ngắt lại sau khi đổi DPI, hộp thông báo thật); test controller "Windows không còn gì
  phải tự ngắt" (ô 114: 274/231 px, ô 122: 270/224 px, ghi chú vị trí: 612/553 px, gợi ý của danh
  sách: 548/536 px); 2 test dấu cách không ngắt.
- Agent cũng tìm ra một lỗi cũ: hộp thông báo chỉ có nút OK không đóng khi nhận `WM_COMMAND(IDOK)`,
  vì nút duy nhất của nó có id `IDCANCEL`, nên lúc thoát Raumakey không đóng được các hộp đó. Sửa:
  gửi `IDCANCEL` (như phím Esc; hộp vẫn trả lời `IDOK`), còn câu hỏi thì gửi `IDNO`. Đỏ: hộp
  Information, Warning và Error "still open" trong cả hai test (hộp tách luồng và hộp của một hộp
  thoại).
- Review: chấp thuận, kèm vài điểm nhỏ. Không có lỗi về tính đúng. Lề của độ rộng hộp thông báo
  chỉ khoảng 20 px ở 96 DPI (comment cũ ghi sai là một phần sáu); các mức phóng to khác và màn hình
  có DPI khác nhau cần kiểm tra thủ công. Đã sửa comment, dùng hằng `IDNO`/`IDCANCEL`, và ẩn seam
  test khỏi tài liệu.

## Nháy đúp và loại trừ ứng dụng (D46, D47)

Ngày 2026-10-18, chủ dự án yêu cầu thêm: nháy đúp vào biểu tượng ở khay thì mở bảng cấu hình; và
loại trừ ứng dụng, ví dụ ô nhập trong RDP luôn gõ tiếng Anh, với danh sách nằm trong tệp cấu hình
(thiết kế §22.6–22.7, kế hoạch D46–D47, task S6 và S7). Hai task chạy song song trong hai worktree,
mỗi task được review độc lập rồi sửa theo review.

### S6: nháy đúp và biểu tượng khi loại trừ

- Đỏ: `WM_LBUTTONDOWN` giải mã thành `Other(513)`; `tick_count`, `double_click_time` chưa có; 6
  test nháy đúp; test unit của watcher (`[]` thay vì `[0x1234, 0]`); các test tray đang chạy (START
  giờ đọc cửa sổ foreground) và các test D47; test glue "biểu tượng theo ứng dụng ở trước" (không
  có lời gọi `Update`).
- Review: cần sửa, đã tái hiện bằng probe.
  1. Giữ lâu cú nhấn thứ hai thì `NIN_SELECT` cuối không bị bỏ qua, nên chế độ vẫn bị đổi sau một
     lần nháy đúp.
  2. Một `WM_LBUTTONDBLCLK` tới muộn (Explorer bận) không trả lại chế độ.
  3. Hai cú nháy chậm có lần nhấn đầu giữ lâu bị coi là nháy đúp.
  4. Biểu tượng xét lớp của cửa sổ cấp cao nhất, còn hook xét lớp của cửa sổ có focus, nên với một
     quy tắc theo `window_class` hai bên không khớp nhau.

  Ngoài ra còn các lỗi nhỏ: chính sách được dựng lại ở mỗi lần đổi foreground; thời gian nháy đúp
  chỉ đọc lúc khởi động.
- Sửa: lấy các lần nhấn mà shell báo làm mốc. Khi shell đã báo một lần nhấn hay một lần nháy đúp,
  hai `NIN_SELECT` không còn được coi là nháy đúp. `NIN_SELECT` thứ hai bị bỏ qua tính từ lúc thả
  cú nhấn thứ hai. Một `WM_LBUTTONDBLCLK` muộn vẫn trả lại chế độ, trong một biên rộng. Tray xét lớp
  của cửa sổ có focus như hook (`describe_window`, `GetGUIThreadInfo`), giữ một chính sách cho mỗi
  snapshot, và đọc lại thời gian nháy đúp ở mỗi nhịp. Đỏ: 7 test tray (3 probe của review, tray
  bận, nhịp quên cú nháy cũ, lớp của focus, cú nháy ngay sau nháy đúp); lỗi biên dịch của
  `describe_window`, `ButtonUp`, `double_click_ms`.

### S7: loại trừ ứng dụng

- Cơ chế có sẵn được kiểm chứng: `CompatibilityPolicy::resolve` cho `vietnamese_allowed = false`,
  lõi phiên đặt `enabled = mode && vietnamese_allowed` và `macros_enabled = macros &&
  macro_allowed`, nên một loại trừ phải tắt cả gõ tắt; hook đọc tên exe của foreground và phím tắt
  được phân loại trước chính sách.
- Đỏ: 11 lỗi biên dịch rồi 7 test `config_document`; 8 test `settings`; 3 test gõ (`mstsc.exe`
  biến đổi phím, gõ tắt mở rộng trong `msrdc.exe`, sau `apps = []` không gõ lại được); 1 test
  `notice`. Phần chuẩn hoá tên: 4 test `config_document`, 1 proptest (giữ nguyên `mstsc`), 1 test
  `settings` (đường dẫn bị báo sai thay vì thành quy tắc); bỏ dấu `"`: 1 test (`mstsc.exe"`).
- Review: chấp thuận, kèm vài điểm nhỏ. Một tên chứa `İ` (viết thường thành hai ký tự) qua được
  giới hạn 255 ký tự, không đọc lại được chính nó, và làm runtime bỏ mọi loại trừ, kể cả
  `mstsc.exe`. Ngoài ra: tên có dấu chấm hay dấu cách ở cuối không bao giờ khớp; báo cáo khi vượt
  4.096 quy tắc ghi sai tệp.
- Sửa: `ExeName::new` kiểm tra giới hạn trên tên đã viết thường, nên mọi `ExeName` đọc lại được
  chính nó (ba test IPC dựng tên `İ` × 200 được viết lại); runtime chỉ bỏ loại trừ nào lõi phiên từ
  chối; bỏ dấu chấm và dấu cách ở cuối tên như Windows; một dòng thông báo chung cho các quy tắc vượt
  giới hạn. Đỏ: test `settings` (cả danh sách bị bỏ), tệp quy tắc ứng dụng `Unusable`, test
  `config` (không có issue), proptest của model và của config, test dấu chấm cuối, test báo cáo khi
  tệp quy tắc không đọc được, 2 test `notice`.
