//! `icon`: icon sizes (pure); on Windows, the system icon metrics and loading an icon group of
//! a module at an explicit size. The module is a system DLL loaded as an image resource (no code
//! runs), and its first icon group is found by enumerating its resources.

use raumakey_windows::icon::{IconSize, MAX_ICON_DIMENSION};

#[test]
fn icon_sizes_are_positive_and_bounded() {
    let size = IconSize::new(20, 24).unwrap();
    assert_eq!((size.width(), size.height()), (20, 24));
    assert_eq!(IconSize::square(16), IconSize::new(16, 16));
    assert_eq!(IconSize::new(0, 16), None);
    assert_eq!(IconSize::new(16, 0), None);
    assert_eq!(MAX_ICON_DIMENSION, 1024);
    assert!(IconSize::square(MAX_ICON_DIMENSION).is_some());
    assert_eq!(IconSize::new(MAX_ICON_DIMENSION + 1, 16), None);
    assert_eq!(IconSize::square(u32::MAX), None);
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    use raumakey_windows::error::WinErrorKind;
    use raumakey_windows::icon::{
        Icon, IconKind, IconSize, icon_size, icon_size_for_dpi, icon_size_for_window,
    };
    use raumakey_windows::window::{Window, WindowClass};
    use windows::Win32::Foundation::{ERROR_INVALID_CURSOR_HANDLE, HWND};
    use windows::Win32::Graphics::Gdi::{BITMAP, DeleteObject, GetObjectW, HGDIOBJ};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetIconInfo, GetSystemMetrics, HICON, ICONINFO, SM_CXICON, SM_CXSMICON, SM_CYICON,
        SM_CYSMICON,
    };
    use windows::core::HRESULT;

    use crate::support::{ResourceFile, group_icon_ids, on_fresh_thread};

    /// Width and height of an icon's image.
    fn dimensions(icon: HICON) -> (u32, u32) {
        let mut info = ICONINFO::default();
        // SAFETY: a live icon; the bitmaps returned in `info` are deleted below.
        unsafe { GetIconInfo(icon, &mut info) }.unwrap();
        let color = !info.hbmColor.is_invalid();
        let source = if color { info.hbmColor } else { info.hbmMask };
        let mut bitmap = BITMAP::default();
        // SAFETY: `bitmap` is writable for the given size.
        let read = unsafe {
            GetObjectW(
                HGDIOBJ(source.0),
                size_of::<BITMAP>() as i32,
                Some((&mut bitmap as *mut BITMAP).cast::<c_void>()),
            )
        };
        // SAFETY: GetIconInfo created these bitmaps for the caller; each is deleted once.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
        assert!(read > 0);
        // A monochrome icon's mask holds the AND and XOR masks on top of each other.
        let height = if color {
            bitmap.bmHeight
        } else {
            bitmap.bmHeight / 2
        };
        (bitmap.bmWidth as u32, height as u32)
    }

    fn shell32() -> ResourceFile {
        ResourceFile::system("shell32.dll")
    }

    /// The error of `GetIconInfo` for `icon`, or `None` when it still names an icon (the
    /// bitmaps returned then are deleted).
    fn icon_info_error(icon: HICON) -> Option<HRESULT> {
        let mut info = ICONINFO::default();
        // SAFETY: GetIconInfo validates the handle and writes only to `info`.
        match unsafe { GetIconInfo(icon, &mut info) } {
            Ok(()) => {
                // SAFETY: GetIconInfo created these bitmaps for the caller; each is deleted once.
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
                    let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
                }
                None
            }
            Err(error) => Some(error.code()),
        }
    }

    #[test]
    fn each_load_creates_an_icon_of_its_own_that_drop_destroys() {
        let file = shell32();
        let ids = group_icon_ids(&file);
        let first = *ids.first().expect("shell32.dll has integer icon groups");
        let size = IconSize::square(32).unwrap();
        let kept = Icon::load(file.module(), first, size).unwrap();
        let dropped = Icon::load(file.module(), first, size).unwrap();
        // With LR_SHARED, the second load would return the cached icon of the first.
        assert_ne!(
            kept.handle(),
            dropped.handle(),
            "each load creates its own icon"
        );
        let handle = dropped.handle();
        assert_eq!(icon_info_error(handle), None);
        drop(dropped);
        assert_eq!(
            icon_info_error(handle),
            Some(HRESULT::from_win32(ERROR_INVALID_CURSOR_HANDLE.0)),
            "the dropped icon was destroyed"
        );
        // Destroying one icon leaves the other intact.
        assert_eq!(dimensions(kept.handle()), (32, 32));
    }

    #[test]
    fn the_first_icon_group_of_a_system_dll_loads_at_each_requested_size() {
        let file = shell32();
        let ids = group_icon_ids(&file);
        let first = *ids.first().expect("shell32.dll has integer icon groups");
        let small = icon_size(IconKind::Small).unwrap();
        for size in [
            IconSize::square(16).unwrap(),
            IconSize::square(32).unwrap(),
            IconSize::square(48).unwrap(),
            IconSize::new(24, 40).unwrap(),
            small,
        ] {
            let icon = Icon::load(file.module(), first, size).unwrap();
            assert!(!icon.handle().is_invalid());
            assert_eq!(
                dimensions(icon.handle()),
                (size.width(), size.height()),
                "{size:?}"
            );
        }
    }

    #[test]
    fn missing_or_invalid_resource_ids_are_errors() {
        let file = shell32();
        let ids = group_icon_ids(&file);
        let missing = (1..=u16::MAX).rev().find(|id| !ids.contains(id)).unwrap();
        let size = IconSize::square(16).unwrap();
        let error = Icon::load(file.module(), missing, size).unwrap_err();
        assert_eq!(error.context(), "LoadImageW");
        let error = Icon::load(file.module(), 0, size).unwrap_err();
        assert_eq!(error.kind(), WinErrorKind::InvalidInput);
    }

    #[test]
    fn system_icon_sizes_match_the_metrics() {
        let small = icon_size(IconKind::Small).unwrap();
        let large = icon_size(IconKind::Large).unwrap();
        // SAFETY: no preconditions.
        unsafe {
            assert_eq!(small.width(), GetSystemMetrics(SM_CXSMICON) as u32);
            assert_eq!(small.height(), GetSystemMetrics(SM_CYSMICON) as u32);
            assert_eq!(large.width(), GetSystemMetrics(SM_CXICON) as u32);
            assert_eq!(large.height(), GetSystemMetrics(SM_CYICON) as u32);
        }
        assert!(small.width() < large.width());
    }

    /// GetSystemMetricsForDpi exists from Windows 10 1607 on, which every supported test host
    /// runs.
    #[test]
    fn per_dpi_icon_sizes_scale_with_the_dpi() {
        let small_96 = icon_size_for_dpi(IconKind::Small, 96).expect("Windows 10 1607 or later");
        let small_192 = icon_size_for_dpi(IconKind::Small, 192).unwrap();
        let large_96 = icon_size_for_dpi(IconKind::Large, 96).unwrap();
        let large_144 = icon_size_for_dpi(IconKind::Large, 144).unwrap();
        assert_eq!(small_192.width(), small_96.width() * 2);
        assert_eq!(small_192.height(), small_96.height() * 2);
        assert_eq!(large_144.width(), large_96.width() * 3 / 2);
        assert!(small_96.width() < large_96.width());
        assert_eq!(icon_size_for_dpi(IconKind::Small, 0), None);
    }

    /// The test executable has no DPI awareness, so its windows run at 96 DPI and their icon
    /// sizes are the system's (scaled by Windows); a handle that names no window falls back to
    /// the system DPI too.
    #[test]
    fn window_icon_sizes_follow_the_windows_dpi() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-icon-dpi").unwrap();
            let window = Window::create_hidden(
                &class,
                "Raumakey test",
                Box::new(|_: HWND, _: u32, _, _| None),
            )
            .unwrap();
            for kind in [IconKind::Small, IconKind::Large] {
                let expected = icon_size_for_dpi(kind, 96).unwrap();
                assert_eq!(icon_size_for_window(kind, window.hwnd()).unwrap(), expected);
                assert_eq!(icon_size(kind).unwrap(), expected);
                assert_eq!(
                    icon_size_for_window(kind, HWND::default()).unwrap(),
                    icon_size(kind).unwrap()
                );
            }
        });
    }
}
