//! `shell`: tooltips and the callback messages of a notification icon (pure), shortcuts in a
//! temporary directory, and the TaskbarCreated message. The tray icon itself is a manual test.

use raumakey_windows::shell::{
    NIN_KEYSELECT, NIN_SELECT, NINF_KEY, NotifyIconEvent, NotifyIconMessage, WM_CONTEXTMENU,
    decode_notify_icon_message, tooltip_units,
};

/// `wParam` of a version-4 callback: the anchor point, x in the low word and y in the high
/// word (`MAKEWPARAM`), each a signed 16-bit screen coordinate.
fn anchor(x: i16, y: i16) -> usize {
    usize::from(x as u16) | (usize::from(y as u16) << 16)
}

/// `lParam` of a version-4 callback: the notification in the low word and the icon id in the
/// high word, as the 32-bit `LPARAM` the shell passes, sign-extended on 64-bit Windows.
fn notification(code: u32, icon_id: u16) -> isize {
    ((u32::from(icon_id) << 16) | (code & 0xFFFF)) as i32 as isize
}

fn decoded(wparam: usize, lparam: isize) -> NotifyIconMessage {
    decode_notify_icon_message(wparam, lparam)
}

#[test]
fn the_notification_codes_are_those_of_the_windows_sdk() {
    assert_eq!(NIN_SELECT, 0x0400, "WM_USER");
    assert_eq!(NINF_KEY, 0x0001);
    assert_eq!(NIN_KEYSELECT, NIN_SELECT | NINF_KEY);
    assert_eq!(WM_CONTEXTMENU, 0x007B);
}

#[test]
fn a_click_and_the_keyboard_select_the_icon() {
    assert_eq!(
        decoded(anchor(10, 20), notification(NIN_SELECT, 1)),
        NotifyIconMessage {
            icon_id: 1,
            event: NotifyIconEvent::Select
        }
    );
    assert_eq!(
        decoded(anchor(10, 20), notification(NIN_KEYSELECT, 1)),
        NotifyIconMessage {
            icon_id: 1,
            event: NotifyIconEvent::KeySelect
        }
    );
}

#[test]
fn the_shortcut_menu_is_anchored_at_signed_screen_coordinates() {
    assert_eq!(
        decoded(anchor(1830, 1050), notification(WM_CONTEXTMENU, 1)),
        NotifyIconMessage {
            icon_id: 1,
            event: NotifyIconEvent::ContextMenu { x: 1830, y: 1050 }
        }
    );
    // Monitors left of and above the primary monitor have negative coordinates.
    assert_eq!(
        decoded(anchor(-1920, -40), notification(WM_CONTEXTMENU, 3)).event,
        NotifyIconEvent::ContextMenu { x: -1920, y: -40 }
    );
    assert_eq!(
        decoded(anchor(i16::MIN, i16::MAX), notification(WM_CONTEXTMENU, 3)).event,
        NotifyIconEvent::ContextMenu {
            x: i32::from(i16::MIN),
            y: i32::from(i16::MAX)
        }
    );
}

/// On 64-bit Windows the parameters are wider than the 32 bits the shell fills (32-bit
/// Windows has no other bits).
#[cfg(target_pointer_width = "64")]
#[test]
fn bits_above_the_two_words_are_ignored() {
    let wide_wparam = anchor(-5, 7) | !0xFFFF_FFFF_usize;
    let wide_lparam = notification(WM_CONTEXTMENU, 2) | !0xFFFF_FFFF_usize as isize;
    assert_eq!(
        decoded(wide_wparam, wide_lparam),
        NotifyIconMessage {
            icon_id: 2,
            event: NotifyIconEvent::ContextMenu { x: -5, y: 7 }
        }
    );
}

#[test]
fn other_notifications_keep_their_code_and_every_icon_id_is_decoded() {
    // WM_MOUSEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_RBUTTONUP, WM_LBUTTONDBLCLK, and
    // NIN_POPUPOPEN.
    for code in [0x0200_u16, 0x0201, 0x0202, 0x0205, 0x0203, 0x0406] {
        assert_eq!(
            decoded(anchor(1, 2), notification(u32::from(code), 9)),
            NotifyIconMessage {
                icon_id: 9,
                event: NotifyIconEvent::Other(code)
            },
            "{code:#x}"
        );
    }
    // Icon ids with the top bit set make the 32-bit LPARAM negative.
    assert_eq!(decoded(0, notification(NIN_SELECT, 0xFFFF)).icon_id, 0xFFFF);
    assert_eq!(decoded(0, notification(NIN_SELECT, 0x8000)).icon_id, 0x8000);
    assert_eq!(decoded(0, notification(NIN_SELECT, 0)).icon_id, 0);
}

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
    fn the_pure_notification_codes_match_the_windows_crate() {
        use windows::Win32::UI::Shell;
        use windows::Win32::UI::WindowsAndMessaging;

        assert_eq!(raumakey_windows::shell::NIN_SELECT, Shell::NIN_SELECT);
        assert_eq!(raumakey_windows::shell::NINF_KEY, Shell::NINF_KEY);
        assert_eq!(
            raumakey_windows::shell::WM_CONTEXTMENU,
            WindowsAndMessaging::WM_CONTEXTMENU
        );
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
