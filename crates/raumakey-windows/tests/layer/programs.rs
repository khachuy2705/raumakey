//! `programs`: which windows count as an application's, one program per executable, and how
//! the description of a program is read from its version resource (pure); on Windows, the
//! applications with a window of this desktop, the running processes, and the description of
//! a system DLL, of a file without one, and of a child `cmd.exe` that has no window and waits
//! on its standard input until the test ends it.

use raumakey_model::host::ExeName;
use raumakey_windows::programs::{
    RunningProgram, TRANSLATION_QUERY, description_query, description_text, distinct_programs,
    first_translation, is_application_window,
};

/// `WS_VISIBLE`.
const VISIBLE: u32 = 0x1000_0000;
/// `WS_EX_TOOLWINDOW`.
const TOOL_WINDOW: u32 = 0x0000_0080;
/// `WS_EX_NOACTIVATE`.
const NO_ACTIVATE: u32 = 0x0800_0000;
/// `WS_EX_APPWINDOW`.
const APP_WINDOW: u32 = 0x0004_0000;

fn program(name: &str, process_id: u32) -> RunningProgram {
    RunningProgram {
        exe: ExeName::new(name).unwrap(),
        process_id,
    }
}

/// An application's window is visible, has no owner, and is no tool window; its other styles
/// do not matter.
#[test]
fn only_visible_unowned_windows_that_are_no_tool_windows_are_applications() {
    assert!(is_application_window(VISIBLE, 0, false));
    assert!(is_application_window(
        VISIBLE | 0x00C0_0000,
        NO_ACTIVATE,
        false
    ));
    assert!(is_application_window(VISIBLE, APP_WINDOW, false));
    assert!(!is_application_window(0, 0, false), "hidden");
    assert!(!is_application_window(VISIBLE, 0, true), "owned");
    assert!(!is_application_window(VISIBLE, APP_WINDOW, true), "owned");
    assert!(
        !is_application_window(VISIBLE, TOOL_WINDOW, false),
        "a tool window"
    );
}

/// Each executable once (its first window's process), sorted by name, and the skipped process
/// (the calling one) left out.
#[test]
fn programs_are_listed_once_by_executable_sorted_without_the_skipped_process() {
    let found = vec![
        program("notepad.exe", 10),
        program("mstsc.exe", 11),
        program("notepad.exe", 12),
        program("raumakey.exe", 5),
        program("explorer.exe", 7),
    ];
    assert_eq!(
        distinct_programs(found.clone(), Some(5)),
        [
            program("explorer.exe", 7),
            program("mstsc.exe", 11),
            program("notepad.exe", 10),
        ]
    );
    assert_eq!(distinct_programs(found, None).len(), 4);
    assert_eq!(distinct_programs(Vec::new(), Some(5)), []);
}

/// The version resource is read in its first translation: language and code page, as the
/// little-endian words of `\VarFileInfo\Translation`, name the `StringFileInfo` table.
#[test]
fn the_description_is_read_in_the_first_translation() {
    assert_eq!(TRANSLATION_QUERY, r"\VarFileInfo\Translation");
    assert_eq!(
        description_query(0x0409, 0x04B0),
        r"\StringFileInfo\040904b0\FileDescription"
    );
    assert_eq!(
        description_query(0x042A, 0x04E3),
        r"\StringFileInfo\042a04e3\FileDescription"
    );
    assert_eq!(
        first_translation(&[0x09, 0x04, 0xB0, 0x04, 0x2A, 0x04, 0xE3, 0x04]),
        Some((0x0409, 0x04B0))
    );
    assert_eq!(first_translation(&[0x09, 0x04, 0xB0]), None);
    assert_eq!(first_translation(&[]), None);
}

/// The description is the text up to its NUL, without the spaces around it; an empty one is
/// none.
#[test]
fn a_description_is_trimmed_and_an_empty_one_is_none() {
    let units = |text: &str| text.encode_utf16().collect::<Vec<u16>>();
    assert_eq!(
        description_text(&units("Remote Desktop Connection\0")),
        Some("Remote Desktop Connection".to_owned())
    );
    assert_eq!(
        description_text(&units("  Trình soạn thảo  \0rác")),
        Some("Trình soạn thảo".to_owned())
    );
    assert_eq!(description_text(&units("PuTTY")), Some("PuTTY".to_owned()));
    assert_eq!(description_text(&units(" \0")), None);
    assert_eq!(description_text(&[]), None);
}

#[cfg(windows)]
mod win {
    use std::io::Write as _;
    use std::os::windows::process::CommandExt as _;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use raumakey_windows::dialog_box::{Dialog, DialogHandler, spawn_dialog};
    use raumakey_windows::dialog_template::{
        Control, DialogRect, DialogTemplate, IDCANCEL, styles,
    };
    use raumakey_windows::foreground::process_image_basename;
    use raumakey_windows::known_folder::system_dir;
    use raumakey_windows::programs::{
        applications_with_windows, file_description, process_description, running_applications,
        running_processes,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        IsWindowVisible, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };

    const WAIT: Duration = Duration::from_secs(10);
    /// `CREATE_NO_WINDOW`: a console process without a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    #[test]
    fn a_system_dll_has_a_description_and_other_files_have_none() {
        let kernel32 = system_dir().unwrap().join("kernel32.dll");
        let description = file_description(&kernel32).expect("kernel32.dll is described");
        assert!(!description.trim().is_empty(), "{description:?}");
        let folder = tempfile::tempdir().unwrap();
        let text = folder.path().join("không phải chương trình.exe");
        std::fs::File::create(&text)
            .unwrap()
            .write_all(b"no version resource")
            .unwrap();
        assert_eq!(file_description(&text), None);
        assert_eq!(file_description(&folder.path().join("missing.exe")), None);
    }

    /// A child process without a window: listed among the running processes by its
    /// executable, described by its version resource, and never among the applications with
    /// a window.
    #[test]
    fn a_running_child_process_is_found_with_its_description() {
        let cmd = system_dir().unwrap().join("cmd.exe");
        let mut child = Command::new(&cmd)
            .args(["/d", "/q"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .unwrap();
        let pid = child.id();
        let processes = running_processes();
        let found = processes.iter().find(|program| program.process_id == pid);
        let description = process_description(pid);
        let applications = running_applications();
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(
            found.map(|program| program.exe.as_str()),
            Some("cmd.exe"),
            "{} processes",
            processes.len()
        );
        let description = description.expect("cmd.exe is described");
        assert!(!description.trim().is_empty(), "{description:?}");
        assert!(applications.iter().all(|program| program.process_id != pid));
        // A process that does not exist has no description.
        assert_eq!(process_description(u32::MAX - 2), None);
    }

    /// Moves the dialog far off-screen before Windows shows it, without activating it.
    struct OffScreen(mpsc::Sender<isize>);

    impl DialogHandler for OffScreen {
        fn init(&mut self, dialog: &Dialog) {
            // SAFETY: a window of the calling thread; no pointers.
            let _ = unsafe {
                SetWindowPos(
                    dialog.hwnd(),
                    None,
                    -32_000,
                    -32_000,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
            let _ = self.0.send(dialog.hwnd().0 as isize);
        }
    }

    /// A visible dialog of this process (off every monitor, never activated) is an
    /// application's window: listed with this process when nothing is skipped, and left out
    /// of the applications of other processes, which every list is sorted and holds once.
    #[test]
    fn the_applications_with_windows_include_this_process_unless_it_is_skipped() {
        let template = DialogTemplate::new("Raumakey thử nghiệm", 120, 40)
            .at(-30_000, -30_000)
            .with_ex_style(styles::WS_EX_NOACTIVATE)
            .with_control(Control::push_button(
                IDCANCEL,
                "Đóng",
                DialogRect::new(60, 20, 50, 14),
            ))
            .build()
            .unwrap();
        let (sender, created) = mpsc::channel();
        let dialog = spawn_dialog(template, OffScreen(sender), |_| {}).unwrap();
        let hwnd = HWND(created.recv_timeout(WAIT).unwrap() as *mut core::ffi::c_void);
        let deadline = Instant::now() + WAIT;
        // SAFETY: only reads the window state.
        while !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            assert!(Instant::now() < deadline, "the dialog was shown");
            std::thread::sleep(Duration::from_millis(10));
        }
        // SAFETY: no preconditions.
        let own = unsafe { GetCurrentProcessId() };
        let exe = process_image_basename(own).unwrap();
        let all = applications_with_windows(None);
        let others = running_applications();
        dialog.dismiss();
        assert!(
            all.iter()
                .any(|program| program.exe == exe && program.process_id == own),
            "{exe} among {} applications",
            all.len()
        );
        assert!(others.iter().all(|program| program.process_id != own));
        for list in [&all, &others] {
            let names: Vec<&str> = list.iter().map(|program| program.exe.as_str()).collect();
            let mut sorted = names.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(names, sorted);
        }
    }
}
