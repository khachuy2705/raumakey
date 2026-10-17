//! `shell`: tooltips (pure), shortcuts in a temporary directory, and the TaskbarCreated
//! message. The tray icon itself is a manual test.

use raumakey_windows::shell::tooltip_units;

#[test]
fn tooltips_are_nul_terminated_and_truncated_at_scalar_boundaries() {
    let units = tooltip_units("Raumakey — Tiếng Việt");
    let expected: Vec<u16> = "Raumakey — Tiếng Việt".encode_utf16().collect();
    assert_eq!(&units[..expected.len()], expected.as_slice());
    assert_eq!(units[expected.len()], 0);

    let long = "a".repeat(200);
    let units = tooltip_units(&long);
    assert_eq!(units[126], u16::from(b'a'));
    assert_eq!(units[127], 0);

    // A surrogate pair that would straddle the limit is dropped whole.
    let tricky = format!("{}😀", "a".repeat(126));
    let units = tooltip_units(&tricky);
    assert_eq!(units[125], u16::from(b'a'));
    assert_eq!(units[126], 0);
}

#[cfg(windows)]
mod win {
    use std::rc::Rc;

    use raumakey_windows::com::ComApartment;
    use raumakey_windows::error::WinResult;
    use raumakey_windows::icon::Icon;
    use raumakey_windows::shell::{
        NotifyIcon, ShortcutSpec, create_shortcut, read_shortcut, remove_shortcut,
        taskbar_created_message,
    };
    use windows::Win32::Foundation::HWND;

    /// A notification icon sends its icon handle to the shell again with every later change
    /// and every re-add after `TaskbarCreated`, so it shares ownership of the icon: the icon
    /// cannot be destroyed while the notification icon may still send it. Adding one shows it,
    /// so this checks the signatures only; `manual_notify_icon` runs them.
    #[test]
    fn notify_icons_share_ownership_of_their_icon() {
        let _add: fn(HWND, u32, u32, Rc<Icon>, &str) -> WinResult<NotifyIcon> = NotifyIcon::add;
        let _set_icon: fn(&mut NotifyIcon, Rc<Icon>) -> WinResult<()> = NotifyIcon::set_icon;
    }

    #[test]
    fn taskbar_created_is_a_registered_message() {
        let message = taskbar_created_message().unwrap();
        assert!(message >= 0xC000, "{message:#x}");
        assert_eq!(taskbar_created_message().unwrap(), message);
    }

    #[test]
    fn shortcuts_are_created_read_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let lnk = dir.path().join("Raumakey ế.lnk");
        let target = std::env::current_exe().unwrap();
        let spec = ShortcutSpec {
            target: target.clone(),
            arguments: "--startup \"quoted arg\"".to_owned(),
            working_dir: Some(dir.path().to_path_buf()),
            description: "Raumakey — bộ gõ tiếng Việt".to_owned(),
            icon: Some((target.clone(), 1)),
        };
        std::thread::spawn(move || {
            let _apartment = ComApartment::sta().unwrap();
            create_shortcut(&lnk, &spec).unwrap();
            assert!(lnk.is_file());
            let read = read_shortcut(&lnk).unwrap();
            assert_eq!(read.target, spec.target);
            assert_eq!(read.arguments, spec.arguments);
            assert_eq!(read.working_dir, spec.working_dir);
            assert_eq!(read.description, spec.description);
            assert_eq!(read.icon, spec.icon);
            assert!(remove_shortcut(&lnk).unwrap());
            assert!(!lnk.exists());
            assert!(!remove_shortcut(&lnk).unwrap());
        })
        .join()
        .unwrap();
    }

    #[test]
    fn shortcut_files_must_end_in_lnk() {
        let dir = tempfile::tempdir().unwrap();
        let spec = ShortcutSpec {
            target: std::env::current_exe().unwrap(),
            arguments: String::new(),
            working_dir: None,
            description: String::new(),
            icon: None,
        };
        assert!(create_shortcut(&dir.path().join("x.txt"), &spec).is_err());
        assert!(remove_shortcut(&dir.path().join("x.txt")).is_err());
    }

    /// Adds a real tray icon for a moment, showing the first icon group of `shell32.dll`.
    /// Manual test only.
    #[test]
    #[ignore = "manual: shows a notification-area icon"]
    fn manual_notify_icon() {
        use raumakey_windows::icon::{IconKind, icon_size};

        use crate::support::{ResourceFile, group_icon_ids};

        let file = ResourceFile::system("shell32.dll");
        let first = *group_icon_ids(&file)
            .first()
            .expect("an integer icon group");
        let size = icon_size(IconKind::Small).unwrap();
        let icon = Rc::new(Icon::load(file.module(), first, size).unwrap());
        let mut tray =
            NotifyIcon::add(HWND::default(), 1, 0x8001, Rc::clone(&icon), "Raumakey").unwrap();
        assert_eq!(
            Rc::strong_count(&icon),
            2,
            "the notification icon keeps the icon"
        );
        tray.set_tooltip("Raumakey test").unwrap();
        tray.set_icon(Rc::clone(&icon)).unwrap();
        tray.delete().unwrap();
        assert_eq!(Rc::strong_count(&icon), 1);
    }
}
