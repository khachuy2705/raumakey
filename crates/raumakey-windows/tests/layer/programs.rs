//! `programs`: which windows count as an application's, one program per executable, and how
//! the description of a program is read from its version resource (pure); on Windows, the
//! applications with a window of this desktop, the facts of a test dialog before and after it
//! is cloaked, the running processes, and the description of system programs, of a system
//! DLL, of a file without one, and of a child `cmd.exe` that has no window and waits on its
//! standard input until the test ends it.

use std::collections::BTreeMap;

use raumakey_model::host::ExeName;
use raumakey_windows::programs::{
    FALLBACK_TRANSLATIONS, RunningProgram, TRANSLATION_QUERY, WindowFacts, description_from,
    description_queries, description_query, description_text, distinct_programs,
    is_application_window, translations,
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

/// A window with `style` and `ex_style`, without an owner, not cloaked.
fn window(style: u32, ex_style: u32) -> WindowFacts {
    WindowFacts {
        style,
        ex_style,
        owned: false,
        cloaked: false,
    }
}

/// An application's window is visible, has no owner, and is no tool window; its other styles
/// do not matter.
#[test]
fn only_visible_unowned_windows_that_are_no_tool_windows_are_applications() {
    let owned = |facts: WindowFacts| WindowFacts {
        owned: true,
        ..facts
    };
    assert!(is_application_window(&window(VISIBLE, 0)));
    assert!(is_application_window(&window(
        VISIBLE | 0x00C0_0000,
        NO_ACTIVATE
    )));
    assert!(is_application_window(&window(VISIBLE, APP_WINDOW)));
    assert!(!is_application_window(&window(0, 0)), "hidden");
    assert!(!is_application_window(&owned(window(VISIBLE, 0))), "owned");
    assert!(
        !is_application_window(&owned(window(VISIBLE, APP_WINDOW))),
        "owned"
    );
    assert!(
        !is_application_window(&window(VISIBLE, TOOL_WINDOW)),
        "a tool window"
    );
}

/// A cloaked window (the shell keeps Settings, the frame host of apps, and the text input host
/// waiting that way) is no application's window, whatever its styles.
#[test]
fn a_cloaked_window_is_no_application_window() {
    for ex_style in [0, APP_WINDOW] {
        let cloaked = WindowFacts {
            cloaked: true,
            ..window(VISIBLE, ex_style)
        };
        assert!(!is_application_window(&cloaked), "{cloaked:?}");
    }
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

/// The translations of `\VarFileInfo\Translation` are pairs of little-endian words, language
/// then code page, each naming a `StringFileInfo` table.
#[test]
fn the_translations_name_the_string_tables() {
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
        translations(&[0x09, 0x04, 0xB0, 0x04, 0x2A, 0x04, 0xE3, 0x04]),
        [(0x0409, 0x04B0), (0x042A, 0x04E3)]
    );
    assert_eq!(
        translations(&[0x09, 0x04, 0xB0, 0x04, 0x2A]),
        [(0x0409, 0x04B0)]
    );
    assert_eq!(translations(&[0x09, 0x04, 0xB0]), []);
    assert_eq!(translations(&[]), []);
}

/// The paths of the `FileDescription` in the tables `codes`, in order.
fn queries(codes: &[&str]) -> Vec<String> {
    codes
        .iter()
        .map(|code| format!(r"\StringFileInfo\{code}\FileDescription"))
        .collect()
}

/// The description is looked for in each translation the resource lists, in its order, then
/// in US English in Unicode, in Windows Latin 1, and without a code page; each table once.
#[test]
fn the_description_is_looked_for_in_each_translation_then_in_us_english() {
    assert_eq!(
        FALLBACK_TRANSLATIONS,
        [(0x0409, 0x04B0), (0x0409, 0x04E4), (0x0409, 0x0000)]
    );
    // Language-neutral, as a program lists it whose strings are under 040904b0.
    assert_eq!(
        description_queries(Some(&[0x00, 0x00, 0xB0, 0x04])),
        queries(&["000004b0", "040904b0", "040904e4", "04090000"])
    );
    // Vietnamese, then US English in Unicode, which is tried once.
    assert_eq!(
        description_queries(Some(&[0x2A, 0x04, 0xB0, 0x04, 0x09, 0x04, 0xB0, 0x04])),
        queries(&["042a04b0", "040904b0", "040904e4", "04090000"])
    );
    // No list of translations at all.
    assert_eq!(
        description_queries(None),
        queries(&["040904b0", "040904e4", "04090000"])
    );
}

/// A version resource with these descriptions, by table.
fn resource(values: &[(&str, &str)]) -> BTreeMap<String, Vec<u16>> {
    values
        .iter()
        .map(|(code, text)| {
            let units = text.encode_utf16().chain([0]).collect();
            (queries(&[code]).remove(0), units)
        })
        .collect()
}

/// The description of `values` listed as language-neutral (`0000`, Unicode), and the paths
/// read for it.
fn described(values: &BTreeMap<String, Vec<u16>>) -> (Option<String>, Vec<String>) {
    let mut asked = Vec::new();
    let found = description_from(Some(&[0x00, 0x00, 0xB0, 0x04]), |query| {
        asked.push(query.to_owned());
        values.get(query).cloned()
    });
    (found, asked)
}

/// The first table that holds a description gives it: one whose description is missing or
/// only spaces is passed over; a resource where none holds one has no description.
#[test]
fn the_first_table_that_holds_a_description_gives_it() {
    // Listed as language-neutral, kept under US English, as the Snipping Tool does.
    let (found, asked) = described(&resource(&[("040904b0", "Snipping Tool")]));
    assert_eq!(found.as_deref(), Some("Snipping Tool"));
    assert_eq!(asked, queries(&["000004b0", "040904b0"]));

    // A blank description in the listed table: the next table with one gives it.
    let (found, asked) = described(&resource(&[("000004b0", "  "), ("04090000", "Công cụ")]));
    assert_eq!(found.as_deref(), Some("Công cụ"));
    assert_eq!(asked.len(), 4, "{asked:?}");

    // The listed table comes first.
    let (found, asked) = described(&resource(&[
        ("000004b0", "Listed"),
        ("040904b0", "Fallback"),
    ]));
    assert_eq!(found.as_deref(), Some("Listed"));
    assert_eq!(asked.len(), 1);

    let (found, asked) = described(&BTreeMap::new());
    assert_eq!(found, None);
    assert_eq!(asked.len(), 4);
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

    use raumakey_windows::dialog_box::{DetachedDialog, Dialog, DialogHandler, spawn_dialog};
    use raumakey_windows::dialog_template::{
        Control, DialogRect, DialogTemplate, IDCANCEL, styles,
    };
    use raumakey_windows::foreground::process_image_basename;
    use raumakey_windows::known_folder::system_dir;
    use raumakey_windows::programs::{
        applications_with_windows, file_description, is_application_window, process_description,
        running_applications, running_processes, window_facts,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmSetWindowAttribute};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        IsWindowVisible, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    };
    use windows::core::BOOL;

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

    /// Programs that every Windows has are described.
    #[test]
    fn the_programs_of_windows_are_described() {
        let system = system_dir().unwrap();
        for program in ["notepad.exe", "cmd.exe", "mstsc.exe"] {
            let description = file_description(&system.join(program));
            let description = description.unwrap_or_else(|| panic!("{program} is described"));
            assert!(!description.trim().is_empty(), "{program}: {description:?}");
        }
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

    /// A visible dialog of this process, off every monitor and never activated, once Windows
    /// showed it.
    fn shown_dialog() -> (DetachedDialog, HWND) {
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
        (dialog, hwnd)
    }

    /// A visible dialog of this process (off every monitor, never activated) is an
    /// application's window: listed with this process when nothing is skipped, and left out
    /// of the applications of other processes, which every list is sorted and holds once.
    #[test]
    fn the_applications_with_windows_include_this_process_unless_it_is_skipped() {
        let (dialog, _hwnd) = shown_dialog();
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

    /// The test dialog is an application's window until it is cloaked (`DWMWA_CLOAK`, as the
    /// shell cloaks the windows it keeps waiting): then it is no longer one.
    #[test]
    fn a_window_that_is_cloaked_is_no_longer_an_applications_window() {
        let (dialog, hwnd) = shown_dialog();
        let before = window_facts(hwnd.0 as isize);
        let cloak = BOOL::from(true);
        // SAFETY: a top-level window of this process; the value is a `BOOL` that outlives the
        // call, of the size passed.
        let cloaked = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_CLOAK,
                (&raw const cloak).cast(),
                size_of::<BOOL>() as u32,
            )
        };
        let after = window_facts(hwnd.0 as isize);
        dialog.dismiss();
        cloaked.unwrap();
        assert!(!before.cloaked, "{before:?}");
        assert!(is_application_window(&before), "{before:?}");
        assert!(after.cloaked, "{after:?}");
        assert!(!is_application_window(&after), "{after:?}");
        // A window that is gone reads as hidden.
        let gone = window_facts(0);
        assert!(!is_application_window(&gone), "{gone:?}");
    }
}
