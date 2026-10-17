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
    use raumakey_windows::com::ComApartment;
    use raumakey_windows::shell::{
        ShortcutSpec, create_shortcut, read_shortcut, remove_shortcut, taskbar_created_message,
    };

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

    /// Adds a real tray icon for a moment. Manual test only.
    #[test]
    #[ignore = "manual: shows a notification-area icon"]
    fn manual_notify_icon() {
        use raumakey_windows::shell::NotifyIcon;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{IDI_APPLICATION, LoadIconW};
        // SAFETY: loads a stock icon.
        let icon = unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap();
        let mut tray = NotifyIcon::add(HWND::default(), 1, 0x8001, icon, "Raumakey").unwrap();
        tray.set_tooltip("Raumakey test").unwrap();
        tray.delete().unwrap();
    }
}
