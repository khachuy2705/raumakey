//! What the run mode tells the user (`notice`): the first-run and problem messages of the
//! start, the problems a reload finds, save errors, the Startup shortcut, recovery, opening the
//! configuration, and "Giới thiệu" (About). Pure: the texts are new Vietnamese strings, and
//! none holds file content or macro text.

use std::fs;
use std::path::{Path, PathBuf};

use raumakey_config::{ConfigError, FieldIssue, IssueReason};
use raumakey_controller::notice::{
    LICENSE, Notice, NoticeKind, Question, REPOSITORY_URL, about_notice, about_text,
    elevated_task_notice, heal_notice, open_error_notice, panel_error_notice, recovery_notice,
    save_error_notice, start_notice, startup_error_notice, status_notice, temporary_exe_notice,
};
use raumakey_controller::settings::{
    DocumentStatus, Location, OpenReport, Recovery, Settings, Status,
};
use raumakey_controller::worker::{OpenError, StartupError};
use raumakey_ipc::error::{IpcError, MalformedReason};
use raumakey_macro::{EntryIssue, MacroError};
use raumakey_model::macros::MacroId;
use raumakey_windows::error::{E_ACCESSDENIED, WinError};

fn path() -> PathBuf {
    PathBuf::from(r"D:\Tools\Raumakey\raumakey.toml")
}

fn clean() -> Status {
    Status {
        config: DocumentStatus::Loaded { issues: Vec::new() },
        macros: DocumentStatus::Missing,
        app_rules: DocumentStatus::Missing,
        skipped_macros: Vec::new(),
    }
}

fn report(location: Location, first_run: bool, status: Status) -> OpenReport {
    OpenReport {
        location,
        config_path: (location != Location::Unavailable).then(path),
        first_run,
        create_error: None,
        status,
    }
}

fn issue(path: &str) -> FieldIssue {
    FieldIssue {
        path: path.to_owned(),
        reason: IssueReason::InvalidValue,
    }
}

/// Every notice is shown in a message box: no NUL, which would cut it short.
fn shown(notice: &Notice) -> &str {
    assert!(!notice.text.contains('\0'), "{}", notice.text);
    assert!(!notice.text.is_empty());
    &notice.text
}

#[test]
fn a_second_run_without_problems_says_nothing() {
    assert_eq!(
        start_notice(&report(Location::ExeDir, false, clean()), true),
        None
    );
    assert_eq!(
        start_notice(&report(Location::LocalAppData, false, clean()), true),
        None
    );
}

/// The first run says where the file was created and how the icon and the menu work.
#[test]
fn the_first_run_tells_where_the_configuration_was_created() {
    let notice = start_notice(&report(Location::ExeDir, true, clean()), true).unwrap();
    assert_eq!(notice.kind, NoticeKind::Information);
    assert_eq!(notice.question, None);
    let text = shown(&notice);
    assert!(text.contains(r"D:\Tools\Raumakey\raumakey.toml"), "{text}");
    assert!(text.contains("cạnh raumakey.exe"), "{text}");
    assert!(text.contains("Ctrl+Shift"), "{text}");
    assert!(text.contains("chuột phải"), "{text}");

    let notice = start_notice(&report(Location::LocalAppData, true, clean()), true).unwrap();
    let text = shown(&notice);
    assert!(
        text.contains(
            "(trong %LOCALAPPDATA%\\Raumakey, vì thư mục của raumakey.exe không ghi được hoặc \
             là thư mục được bảo vệ)"
        ),
        "{text}"
    );
    assert!(!text.contains("cạnh raumakey.exe"), "{text}");
}

/// A settings panel that cannot be opened tells what it would have shown (the welcome is left
/// out when the panel opens at the start): where `raumakey.toml` lives, with the reason for
/// `%LOCALAPPDATA%`, and how to switch the mode with the toggle hotkey and the icon.
#[test]
fn a_panel_that_cannot_open_tells_where_the_file_is_and_how_to_switch() {
    let error = WinError::new(E_ACCESSDENIED, "dialog thread");
    let notice = panel_error_notice(Location::ExeDir, Some(&path()), Some("Ctrl+Shift"), &error);
    assert_eq!(notice.kind, NoticeKind::Warning);
    assert_eq!(notice.question, None);
    assert_eq!(
        shown(&notice),
        "Raumakey không mở được bảng cấu hình (lỗi 0x80070005 tại dialog thread). Raumakey vẫn \
         chạy, và menu của biểu tượng vẫn đổi được các lựa chọn.\n\nTệp cấu hình: \
         D:\\Tools\\Raumakey\\raumakey.toml\n(cạnh raumakey.exe)\n\nBiểu tượng V/E ở khay hệ \
         thống cho biết chế độ gõ: V là tiếng Việt, E là tiếng Anh. Nhấp vào biểu tượng hoặc \
         nhấn Ctrl+Shift để đổi chế độ; nhấp chuột phải vào biểu tượng để mở menu."
    );

    let local = Path::new(r"C:\Users\An\AppData\Local\Raumakey\raumakey.toml");
    let notice = panel_error_notice(Location::LocalAppData, Some(local), None, &error);
    let text = shown(&notice);
    assert!(
        text.contains(
            "Tệp cấu hình: C:\\Users\\An\\AppData\\Local\\Raumakey\\raumakey.toml\n(trong \
             %LOCALAPPDATA%\\Raumakey, vì thư mục của raumakey.exe không ghi được hoặc là thư \
             mục được bảo vệ)"
        ),
        "{text}"
    );
    assert!(text.contains("Nhấp vào biểu tượng để đổi chế độ"), "{text}");

    let notice = panel_error_notice(Location::Unavailable, None, Some("Ctrl+Shift"), &error);
    let text = shown(&notice);
    assert!(text.contains("không được lưu"), "{text}");
}

/// When the settings panel opens at the start (D43), it tells where the file is and how to
/// switch the mode: the first run then has no welcome box. Problems are still told, and a
/// damaged file still asks about recovery.
#[test]
fn without_the_welcome_the_first_run_tells_only_problems() {
    assert_eq!(
        start_notice(&report(Location::ExeDir, true, clean()), false),
        None
    );
    assert_eq!(
        start_notice(&report(Location::LocalAppData, true, clean()), false),
        None
    );
    let status = Status {
        macros: DocumentStatus::Unreadable(ConfigError::Corrupt),
        ..clean()
    };
    let notice = start_notice(&report(Location::ExeDir, true, status.clone()), false).unwrap();
    assert_eq!(notice.kind, NoticeKind::Warning);
    let text = shown(&notice);
    assert!(text.contains("raumakey-macros.toml"), "{text}");
    assert!(!text.contains("Chào mừng"), "{text}");
    let welcomed = start_notice(&report(Location::ExeDir, true, status), true).unwrap();
    assert!(welcomed.text.contains("Chào mừng"), "{}", welcomed.text);
    // A file that could not be written, or no folder at all, is still a warning.
    let mut opened = report(
        Location::ExeDir,
        true,
        Status {
            config: DocumentStatus::Missing,
            ..clean()
        },
    );
    opened.create_error = Some(ConfigError::PermissionDenied);
    assert_eq!(
        start_notice(&opened, false),
        start_notice(&opened, true),
        "no welcome in it"
    );
    let mut nowhere = report(Location::Unavailable, true, clean());
    nowhere.create_error = Some(ConfigError::NotFound);
    assert_eq!(start_notice(&nowhere, false), start_notice(&nowhere, true));
    assert!(start_notice(&nowhere, false).is_some());
    // A later run: the same either way.
    let damaged = Status {
        config: DocumentStatus::Unreadable(ConfigError::Corrupt),
        ..clean()
    };
    let notice = start_notice(&report(Location::ExeDir, false, damaged.clone()), false);
    assert_eq!(
        notice,
        start_notice(&report(Location::ExeDir, false, damaged), true)
    );
    assert_eq!(notice.unwrap().question, Some(Question::Recover));
}

#[test]
fn a_first_run_that_could_not_write_the_file_warns_that_changes_are_not_saved() {
    let mut opened = report(
        Location::ExeDir,
        true,
        Status {
            config: DocumentStatus::Missing,
            ..clean()
        },
    );
    opened.create_error = Some(ConfigError::PermissionDenied);
    let notice = start_notice(&opened, true).unwrap();
    assert_eq!(notice.kind, NoticeKind::Warning);
    let text = shown(&notice);
    assert!(text.contains(r"D:\Tools\Raumakey\raumakey.toml"), "{text}");
    assert!(text.contains("từ chối"), "{text}");
    assert!(text.contains("mặc định"), "{text}");
}

#[test]
fn no_folder_at_all_warns_that_nothing_is_saved() {
    let mut opened = report(
        Location::Unavailable,
        true,
        Status {
            config: DocumentStatus::Missing,
            ..clean()
        },
    );
    opened.create_error = Some(ConfigError::NotFound);
    let notice = start_notice(&opened, true).unwrap();
    assert_eq!(notice.kind, NoticeKind::Warning);
    let text = shown(&notice);
    assert!(text.contains(r"%LOCALAPPDATA%\Raumakey"), "{text}");
    assert!(text.contains("không được lưu"), "{text}");
}

/// A damaged file is kept; the user is asked whether Raumakey may move it aside.
#[test]
fn a_damaged_file_at_start_is_kept_and_the_user_is_asked_about_recovery() {
    let status = Status {
        config: DocumentStatus::Unreadable(ConfigError::Corrupt),
        ..clean()
    };
    let notice = start_notice(&report(Location::ExeDir, false, status), true).unwrap();
    assert_eq!(notice.question, Some(Question::Recover));
    let text = shown(&notice);
    assert!(text.contains(r"D:\Tools\Raumakey\raumakey.toml"), "{text}");
    assert!(text.contains("hỏng"), "{text}");
    assert!(text.contains("không ghi đè"), "{text}");
    assert!(text.contains("raumakey.toml.bak-"), "{text}");
}

/// A file of a newer version is not offered for recovery: it is not damaged.
#[test]
fn a_file_of_a_newer_version_is_kept_without_a_question() {
    let status = Status {
        config: DocumentStatus::Unreadable(ConfigError::UnsupportedVersion),
        ..clean()
    };
    let notice = start_notice(&report(Location::ExeDir, false, status), true).unwrap();
    assert_eq!(notice.kind, NoticeKind::Warning);
    assert_eq!(notice.question, None);
    assert!(shown(&notice).contains("mới hơn"), "{}", notice.text);
}

/// Values that took their defaults are named by their schema path, never by their value.
#[test]
fn defaulted_values_and_skipped_macros_are_listed_by_path_and_count() {
    let status = Status {
        config: DocumentStatus::Loaded {
            issues: vec![issue("input.method"), issue("hotkeys.toggle_mode")],
        },
        macros: DocumentStatus::Loaded { issues: Vec::new() },
        app_rules: DocumentStatus::Unusable(IpcError::Malformed(MalformedReason::InvalidOptions)),
        skipped_macros: vec![
            EntryIssue {
                index: 0,
                error: MacroError::DuplicateId(MacroId(1)),
            },
            EntryIssue {
                index: 4,
                error: MacroError::DuplicateId(MacroId(1)),
            },
        ],
    };
    let notice = start_notice(&report(Location::ExeDir, false, status), true).unwrap();
    assert_eq!(notice.kind, NoticeKind::Warning);
    let text = shown(&notice);
    assert!(text.contains("input.method"), "{text}");
    assert!(text.contains("hotkeys.toggle_mode"), "{text}");
    assert!(text.contains("2 mục"), "{text}");
    assert!(text.contains("raumakey-macros.toml"), "{text}");
    assert!(text.contains("raumakey-app-rules.toml"), "{text}");
}

/// An excluded application whose name cannot be used is left out, not replaced (D47), and a
/// list beyond the snapshot limit is cut: the line about `raumakey.toml` says that values are
/// replaced by their defaults or left out, and names the schema paths.
#[test]
fn excluded_applications_left_out_are_named_by_path() {
    let status = Status {
        config: DocumentStatus::Loaded {
            issues: vec![issue("exclusions.apps[2]"), issue("exclusions.apps")],
        },
        app_rules: DocumentStatus::Loaded {
            issues: vec![FieldIssue {
                path: "rule".to_owned(),
                reason: IssueReason::OutOfRange,
            }],
        },
        ..clean()
    };
    let notice = status_notice(Some(&path()), &clean(), &status).unwrap();
    assert_eq!(
        shown(&notice),
        "Raumakey gặp vấn đề khi nạp lại tệp cấu hình:\n\n- Một số giá trị trong raumakey.toml \
         không dùng được và được thay bằng giá trị mặc định hoặc bị bỏ qua: exclusions.apps[2], \
         exclusions.apps.\n\n- Một số giá trị trong raumakey-app-rules.toml không dùng được và \
         bị bỏ qua: rule."
    );
}

#[test]
fn unreadable_macros_and_rules_are_reported() {
    let status = Status {
        macros: DocumentStatus::Unreadable(ConfigError::Corrupt),
        app_rules: DocumentStatus::Unreadable(ConfigError::IoFailure),
        ..clean()
    };
    let notice = start_notice(&report(Location::ExeDir, false, status), true).unwrap();
    let text = shown(&notice);
    assert!(text.contains("raumakey-macros.toml"), "{text}");
    assert!(text.contains("raumakey-app-rules.toml"), "{text}");
    assert_eq!(notice.question, None, "only raumakey.toml is recovered");
}

/// The notices of real damaged files never show what the files hold.
#[test]
fn notices_never_show_file_content_or_macro_text() {
    let exe = tempfile::tempdir().unwrap();
    fs::write(
        exe.path().join("raumakey.toml"),
        "secret-config-text = [[[ \"bí mật\"",
    )
    .unwrap();
    fs::write(
        exe.path().join("raumakey-macros.toml"),
        "version = 1\nrevision = 1\n[[macro]]\nid = 1\ntrigger = \"secret-trigger\"\nreplacement = \"secret replacement\"\n[[macro]]\nid = 1\ntrigger = \"secret-two\"\nreplacement = \"secret\"\n",
    )
    .unwrap();
    let (settings, opened) = Settings::open(exe.path(), None, &|_| true, &|_| false);
    let notice = start_notice(&opened, true).unwrap();
    let text = shown(&notice);
    assert!(!text.contains("secret"), "{text}");
    assert!(!text.contains("bí mật"), "{text}");
    let later = status_notice(settings.config_path(), &clean(), settings.status()).unwrap();
    assert!(!shown(&later).contains("secret"), "{}", later.text);
}

/// A reload reports only what is new, and asks nothing: the user may be editing the file.
#[test]
fn a_reload_reports_new_problems_once() {
    let broken = Status {
        config: DocumentStatus::Unreadable(ConfigError::Corrupt),
        ..clean()
    };
    let notice = status_notice(Some(&path()), &clean(), &broken).unwrap();
    assert_eq!(notice.kind, NoticeKind::Warning);
    assert_eq!(notice.question, None);
    let text = shown(&notice);
    assert!(text.contains("hỏng"), "{text}");
    assert!(
        text.contains("đang dùng"),
        "the last good settings stay: {text}"
    );
    // The same problem again, or a problem that went away: nothing to say.
    assert_eq!(status_notice(Some(&path()), &broken, &broken), None);
    assert_eq!(status_notice(Some(&path()), &broken, &clean()), None);
    assert_eq!(status_notice(Some(&path()), &clean(), &clean()), None);
    // Another value that took its default is new.
    let one = Status {
        config: DocumentStatus::Loaded {
            issues: vec![issue("input.method")],
        },
        ..clean()
    };
    let two = Status {
        config: DocumentStatus::Loaded {
            issues: vec![issue("input.method"), issue("input.context_limit")],
        },
        ..clean()
    };
    assert!(status_notice(Some(&path()), &clean(), &one).is_some());
    assert_eq!(status_notice(Some(&path()), &one, &one), None);
    let text = status_notice(Some(&path()), &one, &two).unwrap().text;
    assert!(text.contains("input.context_limit"), "{text}");
}

#[test]
fn save_errors_say_the_change_stays_in_effect() {
    for error in [
        ConfigError::PermissionDenied,
        ConfigError::IoFailure,
        ConfigError::Conflict {
            expected: 1,
            actual: 2,
        },
        ConfigError::UnsupportedVersion,
        ConfigError::NotFound,
    ] {
        let notice = save_error_notice(Some(&path()), error);
        assert_eq!(notice.kind, NoticeKind::Warning, "{error:?}");
        assert_eq!(notice.question, None, "{error:?}");
        let text = shown(&notice);
        assert!(text.contains("Không lưu được"), "{text}");
        assert!(text.contains("vẫn có hiệu lực"), "{text}");
    }
    // A damaged file: the user may let Raumakey move it aside.
    let notice = save_error_notice(Some(&path()), ConfigError::Corrupt);
    assert_eq!(notice.question, Some(Question::Recover));
    assert!(
        shown(&notice).contains("raumakey.toml.bak-"),
        "{}",
        notice.text
    );
    // No folder at all.
    let notice = save_error_notice(None, ConfigError::NotFound);
    assert!(
        shown(&notice).contains("vẫn có hiệu lực"),
        "{}",
        notice.text
    );
}

#[test]
fn startup_errors_say_whether_raumakey_still_starts_with_windows() {
    let error = WinError::new(E_ACCESSDENIED, "IPersistFile::Save");
    let on = startup_error_notice(Some(&path()), true, StartupError::Shortcut(error));
    assert_eq!(on.kind, NoticeKind::Error);
    let text = shown(&on);
    assert!(text.contains("Raumakey.lnk"), "{text}");
    assert!(text.contains("0x80070005"), "{text}");
    assert!(text.contains("IPersistFile::Save"), "{text}");
    assert!(
        text.contains("Raumakey sẽ không tự khởi động cùng Windows"),
        "{text}"
    );
    let off = startup_error_notice(Some(&path()), false, StartupError::Shortcut(error));
    assert!(
        shown(&off).contains("vẫn khởi động cùng Windows"),
        "{}",
        off.text
    );
}

/// The shortcut was created or removed, but the choice could not be saved: the text says
/// both, and that the start with Windows follows the shortcut.
#[test]
fn a_startup_choice_that_was_not_saved_says_what_happened_to_the_shortcut() {
    let on = startup_error_notice(
        Some(&path()),
        true,
        StartupError::Save(ConfigError::PermissionDenied),
    );
    assert_eq!(on.kind, NoticeKind::Warning);
    assert_eq!(on.question, None);
    let text = shown(&on);
    assert!(text.contains("đã tạo shortcut Raumakey.lnk"), "{text}");
    assert!(text.contains("không lưu được"), "{text}");
    assert!(text.contains(r"D:\Tools\Raumakey\raumakey.toml"), "{text}");
    assert!(text.contains("từ chối"), "the reason: {text}");
    assert_ne!(
        on,
        save_error_notice(Some(&path()), ConfigError::PermissionDenied)
    );
    let off = startup_error_notice(
        Some(&path()),
        false,
        StartupError::Save(ConfigError::IoFailure),
    );
    let text = shown(&off);
    assert!(text.contains("đã xoá shortcut Raumakey.lnk"), "{text}");
    assert!(text.contains("không lưu được"), "{text}");
}

/// Raumakey run from inside a ZIP lies in the temporary folder, which Windows deletes: the
/// user is told to extract it to a fixed folder first.
#[test]
fn a_shortcut_from_the_temporary_folder_asks_to_extract_raumakey_first() {
    let notice = temporary_exe_notice();
    assert_eq!(notice.kind, NoticeKind::Warning);
    let text = shown(&notice);
    assert!(text.contains("thư mục tạm"), "{text}");
    assert!(text.contains("ZIP"), "{text}");
    assert!(text.contains("giải nén"), "{text}");
    assert!(text.contains("thư mục cố định"), "{text}");
    assert_eq!(
        startup_error_notice(Some(&path()), true, StartupError::Temporary),
        notice
    );
}

/// A path that repeats is listed once, and only distinct paths are counted.
#[test]
fn repeated_schema_paths_are_listed_once() {
    let mut issues = vec![issue("input.method"); 3];
    issues.extend(
        ["a", "b", "c", "d", "e", "f"]
            .into_iter()
            .map(|path| issue(&format!("keymaps.{path}"))),
    );
    let status = Status {
        config: DocumentStatus::Loaded { issues },
        ..clean()
    };
    let text = start_notice(&report(Location::ExeDir, false, status), true)
        .unwrap()
        .text;
    assert_eq!(text.matches("input.method").count(), 1, "{text}");
    assert!(text.contains("keymaps.e"), "{text}");
    assert!(!text.contains("keymaps.f"), "{text}");
    assert!(text.contains("và 1 mục khác"), "{text}");
    let same = Status {
        config: DocumentStatus::Loaded {
            issues: vec![issue("input.method"); 9],
        },
        ..clean()
    };
    let text = start_notice(&report(Location::ExeDir, false, same), true)
        .unwrap()
        .text;
    assert!(!text.contains("mục khác"), "{text}");
}

/// The macros file cannot be read at the start: no macro was loaded.
#[test]
fn unreadable_macros_at_the_start_say_that_no_macro_was_loaded() {
    let status = Status {
        macros: DocumentStatus::Unreadable(ConfigError::Corrupt),
        ..clean()
    };
    let text = start_notice(&report(Location::ExeDir, false, status), true)
        .unwrap()
        .text;
    assert!(text.contains("Chưa nạp được mục gõ tắt nào"), "{text}");
}

/// The elevated logon task is changed only from an elevated command line.
#[test]
fn the_elevated_task_is_turned_off_from_an_elevated_command_line() {
    let notice = elevated_task_notice();
    assert_eq!(notice.kind, NoticeKind::Information);
    let text = shown(&notice);
    assert!(text.contains("raumakey.exe startup none"), "{text}");
    assert!(text.contains("quản trị"), "{text}");
    assert!(text.contains("elevated-task"), "{text}");
}

#[test]
fn recovery_results_are_reported() {
    let backup = PathBuf::from(r"D:\Tools\Raumakey\raumakey.toml.bak-1700000000000");
    assert_eq!(
        recovery_notice(Some(&path()), &Ok(Recovery::NotNeeded)),
        None
    );
    let adopted = recovery_notice(Some(&path()), &Ok(Recovery::Adopted)).unwrap();
    assert_eq!(adopted.kind, NoticeKind::Information);
    let replaced = recovery_notice(
        Some(&path()),
        &Ok(Recovery::Replaced {
            backup: Some(backup.clone()),
            saved: Ok(()),
        }),
    )
    .unwrap();
    assert_eq!(replaced.kind, NoticeKind::Information);
    assert!(
        shown(&replaced).contains("raumakey.toml.bak-1700000000000"),
        "{}",
        replaced.text
    );
    let unsaved = recovery_notice(
        Some(&path()),
        &Ok(Recovery::Replaced {
            backup: Some(backup),
            saved: Err(ConfigError::PermissionDenied),
        }),
    )
    .unwrap();
    assert_eq!(unsaved.kind, NoticeKind::Warning);
    assert!(
        shown(&unsaved).contains("raumakey.toml.bak-1700000000000"),
        "{}",
        unsaved.text
    );
    let failed = recovery_notice(Some(&path()), &Err(ConfigError::PermissionDenied)).unwrap();
    assert_eq!(failed.kind, NoticeKind::Error);
    assert!(shown(&failed).contains("giữ nguyên"), "{}", failed.text);
}

#[test]
fn opening_the_configuration_reports_a_missing_file_or_notepad() {
    let missing = open_error_notice(Some(&path()), OpenError::NoFile);
    assert_eq!(missing.kind, NoticeKind::Information);
    assert!(
        shown(&missing).contains(r"D:\Tools\Raumakey\raumakey.toml"),
        "{}",
        missing.text
    );
    let nowhere = open_error_notice(None, OpenError::NoFile);
    assert!(
        shown(&nowhere).contains("Không có tệp cấu hình"),
        "{}",
        nowhere.text
    );
    let failed = open_error_notice(
        Some(&path()),
        OpenError::Failed(WinError::new(E_ACCESSDENIED, "CreateProcessW(notepad)")),
    );
    assert_eq!(failed.kind, NoticeKind::Error);
    assert!(shown(&failed).contains("Notepad"), "{}", failed.text);
}

/// "Giới thiệu": the product and version, what it is, where the configuration lives, the
/// licence, and the repository.
#[test]
fn about_names_the_version_the_configuration_the_licence_and_the_repository() {
    let text = about_text(Location::ExeDir, Some(&path()));
    let first = text.lines().next().unwrap();
    assert_eq!(first, format!("Raumakey {}", env!("CARGO_PKG_VERSION")));
    assert!(text.contains("Bộ gõ tiếng Việt cho Windows"), "{text}");
    assert!(text.contains(r"D:\Tools\Raumakey\raumakey.toml"), "{text}");
    assert!(text.contains("cạnh raumakey.exe"), "{text}");
    assert!(text.contains(LICENSE), "{text}");
    assert!(text.contains(REPOSITORY_URL), "{text}");
    assert_eq!(LICENSE, "GPL-3.0-or-later");
    assert_eq!(REPOSITORY_URL, "https://github.com/khachuy2705/raumakey");

    let local = about_text(
        Location::LocalAppData,
        Some(Path::new(
            r"C:\Users\u\AppData\Local\Raumakey\raumakey.toml",
        )),
    );
    assert!(local.contains(r"%LOCALAPPDATA%\Raumakey"), "{local}");
    let nowhere = about_text(Location::Unavailable, None);
    assert!(nowhere.contains("không được lưu"), "{nowhere}");

    let notice = about_notice(Location::ExeDir, Some(&path()));
    assert_eq!(notice.kind, NoticeKind::Information);
    assert_eq!(notice.text, text);
    assert_eq!(notice.question, None);
}

#[test]
fn a_shortcut_that_could_not_be_repaired_is_reported() {
    let notice = heal_notice(&WinError::new(E_ACCESSDENIED, "IPersistFile::Save"));
    assert_eq!(notice.kind, NoticeKind::Warning);
    let text = shown(&notice);
    assert!(text.contains("Raumakey.lnk"), "{text}");
    assert!(text.contains("Khởi động cùng Windows"), "{text}");
    assert!(text.contains("0x80070005"), "{text}");
}
