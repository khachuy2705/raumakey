//! What the run mode tells the user, in message boxes (D40–D42): the first run and the
//! problems found at the start, the problems a reload finds, save errors, the Startup-folder
//! shortcut, recovery of a damaged `raumakey.toml`, opening the configuration, and "Giới
//! thiệu" (About).
//!
//! Every text is a new Raumakey string in Vietnamese, built from paths, schema paths, counts,
//! error kinds, and HRESULTs only: never from file content or macro text (a schema path such
//! as `input.method` is built from field names and indices, `raumakey_config::FieldIssue`).
//! Pure: the run mode (`crate::run`) and the tray (`crate::tray`) choose when to show what;
//! the glue shows it with `MessageBoxW`.

use std::path::Path;

use raumakey_config::{ConfigError, FieldIssue};
use raumakey_model::identity::PRODUCT_NAME;
use raumakey_windows::error::WinError;

use crate::install::PRODUCT_VERSION;
use crate::settings::{DocumentStatus, Location, OpenReport, Recovery, Status};
use crate::worker::{OpenError, StartupError};

/// The licence named by "Giới thiệu".
pub const LICENSE: &str = "GPL-3.0-or-later";
/// The repository named by "Giới thiệu".
pub const REPOSITORY_URL: &str = "https://github.com/khachuy2705/raumakey";

/// Schema paths listed in one problem before the rest is counted.
const LISTED_PATHS: usize = 6;

/// How a notice looks: its icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoticeKind {
    /// Information (OK).
    Information,
    /// A warning (OK).
    Warning,
    /// An error (OK).
    Error,
}

/// A yes/no question that a notice asks; the answer "yes" makes the tray act.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Question {
    /// Move the damaged `raumakey.toml` aside and write the settings in effect
    /// (`WorkerCommand::Recover`).
    Recover,
}

/// A message for the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// Its icon.
    pub kind: NoticeKind,
    /// The text (Vietnamese).
    pub text: String,
    /// A question at the end of the text: the box offers yes and no instead of OK.
    pub question: Option<Question>,
}

impl Notice {
    fn new(kind: NoticeKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            question: None,
        }
    }
}

/// When a problem is found: at the start (the defaults are in effect) or by a reload (the
/// last good settings stay).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum When {
    Start,
    Reload,
}

/// Why a store operation failed, as a clause.
fn reason(error: ConfigError) -> &'static str {
    match error {
        ConfigError::NotFound => "không tìm thấy tệp hoặc thư mục chứa nó",
        ConfigError::Corrupt => "tệp bị hỏng hoặc sai cú pháp TOML",
        ConfigError::UnsupportedVersion => "tệp thuộc một phiên bản Raumakey mới hơn",
        ConfigError::PermissionDenied => "Windows từ chối quyền truy cập",
        ConfigError::Conflict { .. } => "tệp liên tục bị chương trình khác thay đổi",
        ConfigError::IoFailure => "lỗi đọc hoặc ghi tệp, có thể tệp đang bị chương trình khác mở",
    }
}

/// `lỗi 0x80004005 tại SetTimer`: the HRESULT and the static name of the failed operation.
pub fn win_error_text(error: &WinError) -> String {
    format!("lỗi 0x{:08X} tại {}", error.code() as u32, error.context())
}

/// The path of `raumakey.toml`, or its name when there is none.
fn config_name(config_path: Option<&Path>) -> String {
    config_path.map_or_else(
        || "raumakey.toml".to_owned(),
        |path| path.display().to_string(),
    )
}

/// Where the configuration lives, as a parenthesis ("Giới thiệu").
fn location_text(location: Location) -> &'static str {
    match location {
        Location::ExeDir => "(cạnh raumakey.exe)",
        Location::LocalAppData => "(trong %LOCALAPPDATA%\\Raumakey)",
        Location::Unavailable => "(không có thư mục để lưu)",
    }
}

/// Where the configuration lives, as a parenthesis that for `%LOCALAPPDATA%` also says why:
/// the folder of `raumakey.exe` cannot be written or is protected (a ZIP opened in the
/// temporary folder, Program Files). The welcome of a first run says it, and so do the
/// settings panel (`crate::panel`), which replaces that welcome, and [`panel_error_notice`].
pub(crate) fn location_explained(location: Location) -> &'static str {
    match location {
        Location::LocalAppData => {
            "(trong %LOCALAPPDATA%\\Raumakey, vì thư mục của raumakey.exe không ghi được hoặc là \
             thư mục được bảo vệ)"
        }
        Location::ExeDir | Location::Unavailable => location_text(location),
    }
}

/// The configuration line without a folder ("Giới thiệu", [`panel_error_notice`]).
const NO_CONFIG_FILE: &str = "Tệp cấu hình: không có (Raumakey không tìm được thư mục để lưu cấu \
hình, nên các thay đổi không được lưu)";

/// How the icon shows and switches the mode, with `toggle_hotkey` (`Ctrl+Shift`) when there
/// is one.
fn icon_help(toggle_hotkey: Option<&str>) -> String {
    let switch = match toggle_hotkey {
        Some(hotkey) => format!("Nhấp vào biểu tượng hoặc nhấn {hotkey} để đổi chế độ"),
        None => "Nhấp vào biểu tượng để đổi chế độ".to_owned(),
    };
    format!(
        "Biểu tượng V/E ở khay hệ thống cho biết chế độ gõ: V là tiếng Việt, E là tiếng Anh. \
         {switch}; nhấp chuột phải vào biểu tượng để mở menu."
    )
}

/// `a, b, c`, or the first [`LISTED_PATHS`] and how many more.
fn paths(issues: &[FieldIssue]) -> String {
    let mut distinct: Vec<&str> = Vec::new();
    for issue in issues {
        if !distinct.contains(&issue.path.as_str()) {
            distinct.push(&issue.path);
        }
    }
    let mut text = distinct
        .iter()
        .take(LISTED_PATHS)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    if distinct.len() > LISTED_PATHS {
        text.push_str(&format!(" và {} mục khác", distinct.len() - LISTED_PATHS));
    }
    text
}

/// Whether `now` is a problem that `before` did not have already.
fn is_new(before: Option<&DocumentStatus>, now: &DocumentStatus) -> bool {
    let problem = match now {
        DocumentStatus::Missing => false,
        DocumentStatus::Loaded { issues } => !issues.is_empty(),
        DocumentStatus::Unreadable(_) | DocumentStatus::Unusable(_) => true,
    };
    problem && before != Some(now)
}

/// The problems of `now` that `before` did not have (all of them without `before`), one line
/// each, and whether `raumakey.toml` is damaged.
fn problems(
    config_path: Option<&Path>,
    before: Option<&Status>,
    now: &Status,
    when: When,
) -> (Vec<String>, bool) {
    let config = config_name(config_path);
    let mut lines = Vec::new();
    let mut damaged = false;
    let keeps = match when {
        When::Start => "dùng cấu hình mặc định",
        When::Reload => "giữ cấu hình đang dùng",
    };
    if is_new(before.map(|status| &status.config), &now.config) {
        match &now.config {
            DocumentStatus::Unreadable(ConfigError::Corrupt) => {
                damaged = true;
                lines.push(format!(
                    "Không đọc được {config}: tệp bị hỏng hoặc sai cú pháp TOML. Raumakey {keeps} \
                     và không ghi đè tệp này; sửa tệp rồi lưu lại thì Raumakey nạp lại sau vài \
                     giây."
                ));
            }
            DocumentStatus::Unreadable(ConfigError::UnsupportedVersion) => lines.push(format!(
                "{config} thuộc một phiên bản Raumakey mới hơn. Raumakey {keeps} và không ghi \
                 đè tệp này."
            )),
            DocumentStatus::Unreadable(error) => lines.push(format!(
                "Không đọc được {config}: {}. Raumakey {keeps} và sẽ thử đọc lại sau vài giây.",
                reason(*error)
            )),
            DocumentStatus::Unusable(_) => lines.push(format!(
                "{config} có giá trị mà bộ gõ không dùng được; Raumakey dùng cấu hình mặc định."
            )),
            // A field takes its default; a list element (`exclusions.apps[2]`) is left out,
            // and so are the elements beyond the limit of a list that is cut.
            DocumentStatus::Loaded { issues } => lines.push(format!(
                "Một số giá trị trong raumakey.toml không dùng được và được thay bằng giá trị \
                 mặc định hoặc bị bỏ qua: {}.",
                paths(issues)
            )),
            DocumentStatus::Missing => {}
        }
    }
    let macros_before = before.map(|status| &status.macros);
    if is_new(macros_before, &now.macros) {
        match &now.macros {
            DocumentStatus::Unreadable(error) => lines.push(match when {
                When::Start => format!(
                    "Không đọc được raumakey-macros.toml: {}. Chưa nạp được mục gõ tắt nào.",
                    reason(*error)
                ),
                When::Reload => format!(
                    "Không đọc được raumakey-macros.toml: {}. Raumakey giữ các mục gõ tắt đang \
                     dùng.",
                    reason(*error)
                ),
            }),
            DocumentStatus::Loaded { issues } => lines.push(format!(
                "Một số giá trị trong raumakey-macros.toml không dùng được và bị bỏ qua: {}.",
                paths(issues)
            )),
            DocumentStatus::Unusable(_) | DocumentStatus::Missing => {}
        }
    }
    let skipped = now.skipped_macros.len();
    let skipped_before = before.map(|status| &status.skipped_macros);
    if skipped > 0 && skipped_before != Some(&now.skipped_macros) {
        lines.push(format!(
            "{skipped} mục trong raumakey-macros.toml không hợp lệ và bị bỏ qua; các mục còn lại \
             vẫn dùng được."
        ));
    }
    if is_new(before.map(|status| &status.app_rules), &now.app_rules) {
        let rules = match when {
            When::Start => "dùng quy tắc ứng dụng mặc định",
            When::Reload => "giữ quy tắc ứng dụng đang dùng",
        };
        match &now.app_rules {
            DocumentStatus::Unreadable(error) => lines.push(format!(
                "Không đọc được raumakey-app-rules.toml: {}. Raumakey {rules}.",
                reason(*error)
            )),
            DocumentStatus::Unusable(_) => lines.push(
                "raumakey-app-rules.toml có quy tắc mà bộ gõ không dùng được; Raumakey dùng quy \
                 tắc ứng dụng mặc định."
                    .to_owned(),
            ),
            DocumentStatus::Loaded { issues } => lines.push(format!(
                "Một số giá trị trong raumakey-app-rules.toml không dùng được và bị bỏ qua: {}.",
                paths(issues)
            )),
            DocumentStatus::Missing => {}
        }
    }
    (lines, damaged)
}

/// The question that offers to move a damaged `raumakey.toml` aside.
const RECOVER_QUESTION: &str = "Bạn có muốn Raumakey đổi tên tệp hỏng thành raumakey.toml.bak-… \
rồi ghi cấu hình đang dùng vào một tệp mới không?";

/// The problem lines under `header`.
fn problem_section(header: &str, lines: &[String]) -> String {
    let mut text = header.to_owned();
    for line in lines {
        text.push_str("\n\n- ");
        text.push_str(line);
    }
    text
}

/// What the start tells the user once, from what [`Settings::open`](crate::settings::Settings)
/// found: the first run (where `raumakey.toml` was created, or why it could not be), and every
/// problem of the files. A damaged `raumakey.toml` asks whether Raumakey may move it aside
/// ([`Question::Recover`]). `None` for a later run without problems.
///
/// Without `welcome` the welcome of a first run that created the file is left out: the
/// settings panel, which opens at that start, shows where the file is and how to switch the
/// mode (D43). The warnings (a file that could not be created, no folder at all, problems of
/// the files) and the recovery question are the same either way.
pub fn start_notice(report: &OpenReport, welcome: bool) -> Option<Notice> {
    let path = report.config_path.as_deref();
    let mut sections = Vec::new();
    let mut kind = NoticeKind::Information;
    if report.location == Location::Unavailable {
        kind = NoticeKind::Warning;
        sections.push(
            "Raumakey không tìm được thư mục để lưu cấu hình: thư mục của raumakey.exe và \
             %LOCALAPPDATA%\\Raumakey đều không dùng được. Raumakey chạy với cấu hình mặc định, \
             và các thay đổi không được lưu."
                .to_owned(),
        );
    } else if report.first_run {
        match report.create_error {
            None if !welcome => {}
            None => {
                sections.push(format!(
                    "Chào mừng bạn đến với Raumakey.\n\nRaumakey đã tạo tệp cấu hình {} {}.\n\n{}",
                    config_name(path),
                    location_explained(report.location),
                    icon_help(Some("Ctrl+Shift"))
                ));
            }
            Some(error) => {
                kind = NoticeKind::Warning;
                sections.push(format!(
                    "Raumakey không tạo được tệp cấu hình {}: {}. Raumakey chạy với cấu hình mặc \
                     định; các thay đổi chưa được lưu, và Raumakey thử ghi lại tệp ở lần thay đổi \
                     kế tiếp.",
                    config_name(path),
                    reason(error)
                ));
            }
        }
    }
    let (lines, damaged) = problems(path, None, &report.status, When::Start);
    if !lines.is_empty() {
        kind = NoticeKind::Warning;
        sections.push(problem_section(
            "Raumakey gặp vấn đề với tệp cấu hình:",
            &lines,
        ));
    }
    if sections.is_empty() {
        return None;
    }
    let mut notice = Notice::new(kind, sections.join("\n\n"));
    if damaged {
        notice.text.push_str("\n\n");
        notice.text.push_str(RECOVER_QUESTION);
        notice.question = Some(Question::Recover);
    }
    Some(notice)
}

/// What a reload found that is new since `before`: a file that became unreadable, values
/// that took their defaults, macros left out. `None` when nothing new is wrong. It asks
/// nothing: the user may be editing the file.
pub fn status_notice(config_path: Option<&Path>, before: &Status, now: &Status) -> Option<Notice> {
    let (lines, _) = problems(config_path, Some(before), now, When::Reload);
    (!lines.is_empty()).then(|| {
        Notice::new(
            NoticeKind::Warning,
            problem_section("Raumakey gặp vấn đề khi nạp lại tệp cấu hình:", &lines),
        )
    })
}

/// A change from the menu that could not be saved: it stays in effect until Raumakey exits.
/// A damaged file asks whether Raumakey may move it aside ([`Question::Recover`]).
pub fn save_error_notice(config_path: Option<&Path>, error: ConfigError) -> Notice {
    let stays = "Thay đổi vẫn có hiệu lực cho đến khi bạn thoát Raumakey.";
    let Some(path) = config_path else {
        return Notice::new(
            NoticeKind::Warning,
            format!("Không lưu được thay đổi: Raumakey không có thư mục để lưu cấu hình. {stays}"),
        );
    };
    let path = path.display();
    match error {
        ConfigError::Corrupt => Notice {
            kind: NoticeKind::Warning,
            text: format!(
                "Không lưu được thay đổi vào {path}: tệp hiện có bị hỏng hoặc sai cú pháp TOML, \
                 và Raumakey không ghi đè nó. {stays}\n\n{RECOVER_QUESTION}"
            ),
            question: Some(Question::Recover),
        },
        other => Notice::new(
            NoticeKind::Warning,
            format!(
                "Không lưu được thay đổi vào {path}: {}. {stays}",
                reason(other)
            ),
        ),
    }
}

/// "Khởi động cùng Windows" could not be turned on (`on`) or off.
pub fn startup_error_notice(config_path: Option<&Path>, on: bool, error: StartupError) -> Notice {
    match error {
        StartupError::Temporary => temporary_exe_notice(),
        StartupError::Save(error) => {
            let config = config_name(config_path);
            let text = if on {
                format!(
                    "Raumakey đã tạo shortcut Raumakey.lnk trong thư mục Startup, nhưng không lưu \
                     được lựa chọn này vào {config}: {}. Raumakey vẫn khởi động cùng Windows nhờ \
                     shortcut đó, và sẽ thử lưu lại lựa chọn.",
                    reason(error)
                )
            } else {
                format!(
                    "Raumakey đã xoá shortcut Raumakey.lnk trong thư mục Startup, nhưng không lưu \
                     được lựa chọn này vào {config}: {}. Raumakey sẽ không tự khởi động cùng \
                     Windows, và sẽ thử lưu lại lựa chọn.",
                    reason(error)
                )
            };
            Notice::new(NoticeKind::Warning, text)
        }
        StartupError::Shortcut(error) if on => Notice::new(
            NoticeKind::Error,
            format!(
                "Không tạo được shortcut Raumakey.lnk trong thư mục Startup ({}). Raumakey sẽ \
                 không tự khởi động cùng Windows.",
                win_error_text(&error)
            ),
        ),
        StartupError::Shortcut(error) => Notice::new(
            NoticeKind::Error,
            format!(
                "Không xoá được shortcut Raumakey.lnk trong thư mục Startup ({}). Raumakey vẫn \
                 khởi động cùng Windows.",
                win_error_text(&error)
            ),
        ),
    }
}

/// `raumakey.exe` runs from the temporary folder (opened inside a ZIP), which Windows deletes
/// later: no Startup shortcut is made to it, and the user is told to extract Raumakey first.
pub fn temporary_exe_notice() -> Notice {
    Notice::new(
        NoticeKind::Warning,
        "raumakey.exe đang chạy từ thư mục tạm của Windows (có thể bạn mở nó ngay trong tệp ZIP). \
         Windows sẽ xoá thư mục này, nên Raumakey không tạo shortcut khởi động cùng Windows tới \
         đó.\n\nHãy giải nén Raumakey vào một thư mục cố định, ví dụ D:\\Tools\\Raumakey, chạy \
         raumakey.exe ở đó, rồi bật lại mục \"Khởi động cùng Windows\".",
    )
}

/// "Khởi động cùng Windows" is on through the elevated logon task, which only an elevated
/// command line changes (design §21.4).
pub fn elevated_task_notice() -> Notice {
    Notice::new(
        NoticeKind::Information,
        "Raumakey đang khởi động cùng Windows bằng tác vụ chạy với quyền quản trị viên \
         (elevated-task), được đặt từ dòng lệnh. Menu không đổi được tác vụ này. Để tắt, mở \
         Command Prompt hoặc PowerShell bằng Run as administrator rồi chạy:\n\n\
         raumakey.exe startup none",
    )
}

/// What [`Settings::recover`](crate::settings::Settings::recover) did; `None` when nothing was
/// needed.
pub fn recovery_notice(
    config_path: Option<&Path>,
    result: &Result<Recovery, ConfigError>,
) -> Option<Notice> {
    let config = config_name(config_path);
    let notice = match result {
        Ok(Recovery::NotNeeded) => return None,
        Ok(Recovery::Adopted) => Notice::new(
            NoticeKind::Information,
            format!("{config} đọc được trở lại; Raumakey dùng cấu hình trong tệp."),
        ),
        Ok(Recovery::Replaced { backup, saved }) => {
            let moved = backup.as_ref().map_or_else(String::new, |backup| {
                format!("Raumakey đã đổi tên tệp hỏng thành {}. ", backup.display())
            });
            match saved {
                Ok(()) => Notice::new(
                    NoticeKind::Information,
                    format!("{moved}Cấu hình đang dùng đã được ghi vào {config}."),
                ),
                Err(error) => Notice::new(
                    NoticeKind::Warning,
                    format!(
                        "{moved}Không ghi được tệp mới {config}: {}. Raumakey thử ghi lại ở lần \
                         thay đổi kế tiếp.",
                        reason(*error)
                    ),
                ),
            }
        }
        Err(error) => Notice::new(
            NoticeKind::Error,
            format!(
                "Không đổi tên được tệp hỏng {config}: {}. Tệp được giữ nguyên.",
                reason(*error)
            ),
        ),
    };
    Some(notice)
}

/// "Mở file cấu hình" did not open the file.
pub fn open_error_notice(config_path: Option<&Path>, error: OpenError) -> Notice {
    match (error, config_path) {
        (OpenError::NoFile, Some(path)) => Notice::new(
            NoticeKind::Information,
            format!(
                "Chưa có tệp cấu hình {}. Raumakey tạo tệp này khi lưu thay đổi kế tiếp.",
                path.display()
            ),
        ),
        (OpenError::NoFile, None) => Notice::new(
            NoticeKind::Information,
            "Không có tệp cấu hình: Raumakey không tìm được thư mục để lưu cấu hình.",
        ),
        (OpenError::Failed(error), _) => Notice::new(
            NoticeKind::Error,
            format!("Không mở được Notepad ({}).", win_error_text(&error)),
        ),
    }
}

/// The text of "Giới thiệu": the product and its version, what it is, where `raumakey.toml`
/// lives, the licence, and the repository.
pub fn about_text(location: Location, config_path: Option<&Path>) -> String {
    let config = match config_path {
        Some(path) if location != Location::Unavailable => {
            format!(
                "Tệp cấu hình: {}\n{}",
                path.display(),
                location_text(location)
            )
        }
        _ => NO_CONFIG_FILE.to_owned(),
    };
    format!(
        "{PRODUCT_NAME} {PRODUCT_VERSION}\nBộ gõ tiếng Việt cho Windows.\n\n{config}\n\nGiấy phép: \
         {LICENSE}\n{REPOSITORY_URL}"
    )
}

/// The settings panel could not be opened (its thread could not start). It tells what the
/// panel would have shown, which matters at the start: the panel that opens there replaces
/// the welcome of a first run ([`start_notice`]). So it names `raumakey.toml` and where it
/// lives (with the reason for `%LOCALAPPDATA%`), and how the icon and `toggle_hotkey` (the
/// first toggle hotkey, if any) switch the mode.
pub fn panel_error_notice(
    location: Location,
    config_path: Option<&Path>,
    toggle_hotkey: Option<&str>,
    error: &WinError,
) -> Notice {
    let config = match config_path {
        Some(path) if location != Location::Unavailable => format!(
            "Tệp cấu hình: {}\n{}",
            path.display(),
            location_explained(location)
        ),
        _ => NO_CONFIG_FILE.to_owned(),
    };
    Notice::new(
        NoticeKind::Warning,
        format!(
            "Raumakey không mở được bảng cấu hình ({}). Raumakey vẫn chạy, và menu của biểu \
             tượng vẫn đổi được các lựa chọn.\n\n{config}\n\n{}",
            win_error_text(error),
            icon_help(toggle_hotkey)
        ),
    )
}

/// "Giới thiệu" as a notice.
pub fn about_notice(location: Location, config_path: Option<&Path>) -> Notice {
    Notice::new(NoticeKind::Information, about_text(location, config_path))
}

/// The start could not make the Startup-folder shortcut point at this `raumakey.exe` (D41).
pub fn heal_notice(error: &WinError) -> Notice {
    Notice::new(
        NoticeKind::Warning,
        format!(
            "Không sửa được shortcut Raumakey.lnk trong thư mục Startup để trỏ tới raumakey.exe \
             này ({}). Raumakey có thể không tự chạy khi bạn đăng nhập; hãy tắt rồi bật lại mục \
             \"Khởi động cùng Windows\" trong menu.",
            win_error_text(error)
        ),
    )
}
