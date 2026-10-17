//! `menu`: item flags (pure); on Windows, building popup menus and reading their items back.
//! Tracking a menu shows it on the desktop, so that is a manual test.

use raumakey_windows::error::WinErrorKind;
use raumakey_windows::menu::{MenuItem, flags};

#[test]
fn item_flags_match_the_appendmenu_bits() {
    assert_eq!(flags::MF_STRING, 0x0000);
    assert_eq!(flags::MF_GRAYED, 0x0001);
    assert_eq!(flags::MF_CHECKED, 0x0008);
    assert_eq!(flags::MF_SEPARATOR, 0x0800);

    let plain = MenuItem::command(1, "Bật tiếng Việt");
    assert_eq!(plain.flags(), flags::MF_STRING);
    assert_eq!(plain.checked(true).flags(), flags::MF_CHECKED);
    assert_eq!(plain.disabled(true).flags(), flags::MF_GRAYED);
    assert_eq!(
        plain.checked(true).disabled(true).flags(),
        flags::MF_CHECKED | flags::MF_GRAYED
    );
    assert_eq!(plain.checked(true).checked(false).flags(), flags::MF_STRING);
    assert_eq!(MenuItem::Separator.flags(), flags::MF_SEPARATOR);
    // A separator has no state to set.
    assert_eq!(
        MenuItem::Separator.checked(true).disabled(true),
        MenuItem::Separator
    );
}

#[test]
fn debug_output_never_shows_item_text() {
    let item = MenuItem::command(7, "gõ tắt: xin chào").checked(true);
    let shown = format!("{item:?}");
    assert!(!shown.contains("xin chào"), "{shown}");
    assert!(shown.contains("id: 7"), "{shown}");
    assert!(shown.contains("checked: true"), "{shown}");
    assert_eq!(format!("{:?}", MenuItem::Separator), "Separator");
}

#[test]
fn command_ids_must_be_non_zero_and_texts_free_of_nul() {
    assert!(MenuItem::command(1, "Thoát").validate().is_ok());
    assert!(MenuItem::command(u16::MAX, "").validate().is_ok());
    assert!(MenuItem::Separator.validate().is_ok());
    // TrackPopupMenuEx returns 0 for "no selection", so 0 cannot be a command.
    let error = MenuItem::command(0, "Thoát").validate().unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::InvalidInput);
    let error = MenuItem::command(2, "a\0b").validate().unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::InvalidInput);
}

#[cfg(windows)]
mod win {
    use raumakey_windows::error::WinErrorKind;
    use raumakey_windows::menu::{MenuItem, PopupMenu};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetMenuItemCount, GetMenuItemID, GetMenuState, GetMenuStringW, MF_BYPOSITION, MF_CHECKED,
        MF_GRAYED, MF_SEPARATOR,
    };

    use crate::support::on_fresh_thread;

    fn state(menu: &PopupMenu, position: u32) -> u32 {
        // SAFETY: a live menu handle; position-based query without side effects.
        unsafe { GetMenuState(menu.handle(), position, MF_BYPOSITION) }
    }

    fn text(menu: &PopupMenu, position: u32) -> String {
        let mut buffer = [0_u16; 64];
        // SAFETY: a live menu handle and a writable buffer.
        let len =
            unsafe { GetMenuStringW(menu.handle(), position, Some(&mut buffer), MF_BYPOSITION) };
        String::from_utf16(&buffer[..len as usize]).unwrap()
    }

    fn id(menu: &PopupMenu, position: i32) -> u32 {
        // SAFETY: a live menu handle.
        unsafe { GetMenuItemID(menu.handle(), position) }
    }

    #[test]
    fn appended_items_keep_their_ids_texts_and_states() {
        on_fresh_thread(|| {
            let mut menu = PopupMenu::new().unwrap();
            assert_eq!(menu.item_count().unwrap(), 0);
            menu.append(MenuItem::command(10, "Tiếng Việt").checked(true))
                .unwrap();
            menu.append(MenuItem::command(11, "Telex")).unwrap();
            menu.append(MenuItem::Separator).unwrap();
            menu.append(MenuItem::command(12, "Cập nhật").disabled(true))
                .unwrap();
            menu.append(MenuItem::command(13, "Thoát")).unwrap();
            assert_eq!(menu.item_count().unwrap(), 5);
            // SAFETY: a live menu handle.
            assert_eq!(unsafe { GetMenuItemCount(Some(menu.handle())) }, 5);

            assert_ne!(state(&menu, 0) & MF_CHECKED.0, 0);
            assert_eq!(state(&menu, 0) & MF_GRAYED.0, 0);
            assert_eq!(state(&menu, 1) & (MF_CHECKED.0 | MF_GRAYED.0), 0);
            assert_ne!(state(&menu, 2) & MF_SEPARATOR.0, 0);
            assert_ne!(state(&menu, 3) & MF_GRAYED.0, 0);
            assert_eq!(state(&menu, 3) & MF_CHECKED.0, 0);
            assert_eq!(state(&menu, 4) & (MF_CHECKED.0 | MF_GRAYED.0), 0);

            assert_eq!(text(&menu, 0), "Tiếng Việt");
            assert_eq!(text(&menu, 3), "Cập nhật");
            assert_eq!(
                [id(&menu, 0), id(&menu, 1), id(&menu, 3), id(&menu, 4)],
                [10, 11, 12, 13]
            );
        });
    }

    #[test]
    fn invalid_items_are_rejected_without_changing_the_menu() {
        on_fresh_thread(|| {
            let mut menu = PopupMenu::new().unwrap();
            let error = menu.append(MenuItem::command(0, "x")).unwrap_err();
            assert_eq!(error.kind(), WinErrorKind::InvalidInput);
            let error = menu.append(MenuItem::command(1, "x\0y")).unwrap_err();
            assert_eq!(error.kind(), WinErrorKind::InvalidInput);
            assert_eq!(menu.item_count().unwrap(), 0);
        });
    }

    #[test]
    fn menus_are_destroyed_on_drop() {
        on_fresh_thread(|| {
            let menu = PopupMenu::new().unwrap();
            let handle = menu.handle();
            drop(menu);
            // SAFETY: GetMenuItemCount validates the handle and fails for a destroyed menu.
            assert_eq!(unsafe { GetMenuItemCount(Some(handle)) }, -1);
        });
    }

    /// Shows a menu at the cursor and prints nothing; pick an item or press Esc. Manual test.
    #[test]
    #[ignore = "manual: shows a popup menu on the desktop and waits for a choice"]
    fn manual_track_returns_the_chosen_command() {
        use raumakey_windows::window::{Window, WindowClass};
        use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

        on_fresh_thread(|| {
            let class = WindowClass::register("test-menu-owner").unwrap();
            let owner = Window::create_hidden(
                &class,
                "Raumakey menu test",
                Box::new(|_: HWND, _: u32, _: WPARAM, _: LPARAM| -> Option<LRESULT> { None }),
            )
            .unwrap();
            let mut menu = PopupMenu::new().unwrap();
            menu.append(MenuItem::command(1, "Một")).unwrap();
            menu.append(MenuItem::command(2, "Hai").checked(true))
                .unwrap();
            menu.append(MenuItem::Separator).unwrap();
            menu.append(MenuItem::command(3, "Ba (tắt)").disabled(true))
                .unwrap();
            let mut point = POINT::default();
            // SAFETY: a valid out pointer.
            unsafe { GetCursorPos(&mut point) }.unwrap();
            let chosen = menu.track(owner.hwnd(), point);
            assert!(matches!(chosen, None | Some(1 | 2)), "{chosen:?}");
        });
    }
}
