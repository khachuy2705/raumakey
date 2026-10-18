//! Icons: loading an icon group from a module's resources at an explicit size, and the system's
//! small and large icon sizes (plan §10.2: the V/E notification icons; plan §11: icons generated
//! by `raumakey-resources` into each binary).
//!
//! [`IconSize`] is pure. On Windows:
//!
//! - [`Icon::load`] loads an `RT_GROUP_ICON` by integer resource id from a
//!   [`Module`](crate::module::Module) with `LoadImageW` at the requested size; Windows picks the
//!   closest image of the group and scales it when needed. The result is a new icon owned by
//!   [`Icon`] and destroyed with `DestroyIcon` on drop. Icons loaded with `LR_SHARED` are owned
//!   by the system and must never be destroyed, so this module never loads or wraps one.
//! - [`icon_size`] reads the small (`SM_CXSMICON`) or large (`SM_CXICON`) icon size for the
//!   system DPI with `GetSystemMetrics`.
//! - [`icon_size_for_dpi`] reads them for a given DPI with `GetSystemMetricsForDpi`, which
//!   exists from Windows 10 1607 on. It is looked up at run time, so the binaries still load on
//!   older Windows 10 releases, where the function returns `None` and callers use
//!   [`icon_size`].
//! - [`icon_size_for_window`] reads them for the DPI of a window (`GetDpiForWindow`, also from
//!   Windows 10 1607 on and looked up at run time), and falls back to [`icon_size`]: the icons
//!   of a dialog's title bar and taskbar button follow the monitor it is on.
//!
//! [`Icon::handle`] lends the `HICON` to a call that uses it before the borrow ends. A
//! [`NotifyIcon`](crate::shell::NotifyIcon) sends its icon handle to the shell again with every
//! later change and re-add, so it takes shared ownership of the icon (`Rc<Icon>`) instead.

/// Largest accepted icon width or height in pixels.
pub const MAX_ICON_DIMENSION: u32 = 1024;

/// Width and height of an icon in pixels, each in `1..=MAX_ICON_DIMENSION`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IconSize {
    width: u32,
    height: u32,
}

impl IconSize {
    /// A size of `width` x `height` pixels; `None` when either is 0 or above
    /// [`MAX_ICON_DIMENSION`].
    pub const fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 || width > MAX_ICON_DIMENSION || height > MAX_ICON_DIMENSION {
            None
        } else {
            Some(Self { width, height })
        }
    }

    /// A square size of `size` x `size` pixels.
    pub const fn square(size: u32) -> Option<Self> {
        Self::new(size, size)
    }

    /// Width in pixels.
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub const fn height(self) -> u32 {
        self.height
    }
}

/// The two system icon sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconKind {
    /// Small icons: window captions, the notification area (`SM_CXSMICON`, 16 px at 96 DPI).
    Small,
    /// Large icons: Alt+Tab, dialogs (`SM_CXICON`, 32 px at 96 DPI).
    Large,
}

#[cfg(windows)]
pub use win::{Icon, icon_size, icon_size_for_dpi, icon_size_for_window};
#[cfg(windows)]
pub(crate) use win::{system_metric_for_dpi, user32_export, window_dpi};

#[cfg(windows)]
mod win {
    use core::fmt;
    use std::sync::OnceLock;

    use windows::Win32::Foundation::{HMODULE, HWND};
    use windows::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_PIN, GetModuleHandleExW, GetProcAddress,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, GetSystemMetrics, HICON, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, SM_CXICON,
        SM_CXSMICON, SM_CYICON, SM_CYSMICON, SYSTEM_METRICS_INDEX,
    };
    use windows::core::{PCSTR, PCWSTR, s, w};

    use super::{IconKind, IconSize};
    use crate::error::{E_FAIL, E_INVALIDARG, ResultExt, WinError, WinResult};
    use crate::module::Module;

    /// An icon created for this process (not `LR_SHARED`), destroyed with `DestroyIcon` on
    /// drop.
    pub struct Icon {
        handle: HICON,
    }

    impl fmt::Debug for Icon {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Icon")
                .field("handle", &self.handle.0)
                .finish()
        }
    }

    impl Icon {
        /// Loads the icon group `resource_id` (`1..=0xFFFF`) of `module` at `size`
        /// (`LoadImageW` without `LR_SHARED`).
        pub fn load(module: Module, resource_id: u16, size: IconSize) -> WinResult<Self> {
            if resource_id == 0 {
                return Err(WinError::new(E_INVALIDARG, "icon resource id"));
            }
            // MAKEINTRESOURCEW: an integer id travels in place of the name pointer.
            let name = PCWSTR(usize::from(resource_id) as *const u16);
            // SAFETY: `module` is a loaded module per its contract; the name is an integer
            // resource id; without LR_SHARED the icon is new and owned by the result.
            let handle = unsafe {
                LoadImageW(
                    Some(module.instance()),
                    name,
                    IMAGE_ICON,
                    size.width() as i32,
                    size.height() as i32,
                    LR_DEFAULTCOLOR,
                )
            }
            .context("LoadImageW")?;
            Ok(Self {
                handle: HICON(handle.0),
            })
        }

        /// The icon handle, valid while `self` lives. Code that keeps the handle beyond the
        /// borrow must keep the `Icon` alive as long, as
        /// [`NotifyIcon`](crate::shell::NotifyIcon) does with its `Rc<Icon>`.
        pub fn handle(&self) -> HICON {
            self.handle
        }
    }

    impl Drop for Icon {
        fn drop(&mut self) {
            // SAFETY: the icon was created by `load` (not shared) and is destroyed once.
            let _ = unsafe { DestroyIcon(self.handle) };
        }
    }

    const fn metrics(kind: IconKind) -> (SYSTEM_METRICS_INDEX, SYSTEM_METRICS_INDEX) {
        match kind {
            IconKind::Small => (SM_CXSMICON, SM_CYSMICON),
            IconKind::Large => (SM_CXICON, SM_CYICON),
        }
    }

    fn to_size(width: i32, height: i32) -> Option<IconSize> {
        IconSize::new(u32::try_from(width).ok()?, u32::try_from(height).ok()?)
    }

    /// The small or large icon size for the system DPI (`GetSystemMetrics`).
    pub fn icon_size(kind: IconKind) -> WinResult<IconSize> {
        let (x, y) = metrics(kind);
        // SAFETY: no preconditions.
        let (width, height) = unsafe { (GetSystemMetrics(x), GetSystemMetrics(y)) };
        to_size(width, height).ok_or(WinError::new(E_FAIL, "GetSystemMetrics(icon size)"))
    }

    /// The small or large icon size for `dpi` (`GetSystemMetricsForDpi`); `None` for a DPI of
    /// 0, when the function is unavailable (before Windows 10 1607), or when it fails.
    pub fn icon_size_for_dpi(kind: IconKind, dpi: u32) -> Option<IconSize> {
        if dpi == 0 {
            return None;
        }
        let metrics_for_dpi = system_metrics_for_dpi()?;
        let (x, y) = metrics(kind);
        // SAFETY: `metrics_for_dpi` is user32's GetSystemMetricsForDpi, which takes a metric
        // index and a DPI and has no other preconditions.
        let (width, height) = unsafe { (metrics_for_dpi(x, dpi), metrics_for_dpi(y, dpi)) };
        to_size(width, height)
    }

    /// The small or large icon size for the DPI of window `hwnd` (`GetDpiForWindow`, then
    /// [`icon_size_for_dpi`]), or for the system DPI ([`icon_size`]) when the DPI functions are
    /// unavailable (before Windows 10 1607) or fail.
    pub fn icon_size_for_window(kind: IconKind, hwnd: HWND) -> WinResult<IconSize> {
        match window_dpi(hwnd).and_then(|dpi| icon_size_for_dpi(kind, dpi)) {
            Some(size) => Ok(size),
            None => icon_size(kind),
        }
    }

    /// System metric `index` for `dpi` (`GetSystemMetricsForDpi`); `None` for a DPI of 0 and
    /// when the function is unavailable (before Windows 10 1607).
    pub(crate) fn system_metric_for_dpi(index: SYSTEM_METRICS_INDEX, dpi: u32) -> Option<i32> {
        if dpi == 0 {
            return None;
        }
        let metrics_for_dpi = system_metrics_for_dpi()?;
        // SAFETY: `metrics_for_dpi` is user32's GetSystemMetricsForDpi, which takes a metric
        // index and a DPI and has no other preconditions.
        Some(unsafe { metrics_for_dpi(index, dpi) })
    }

    /// The DPI of window `hwnd`; `None` when `GetDpiForWindow` is unavailable or fails (0).
    pub(crate) fn window_dpi(hwnd: HWND) -> Option<u32> {
        let dpi_for_window = dpi_for_window()?;
        // SAFETY: `dpi_for_window` is user32's GetDpiForWindow, which validates the handle and
        // answers 0 for one that names no window.
        let dpi = unsafe { dpi_for_window(hwnd) };
        (dpi != 0).then_some(dpi)
    }

    /// `int WINAPI GetSystemMetricsForDpi(int nIndex, UINT dpi)`.
    type MetricsForDpi = unsafe extern "system" fn(SYSTEM_METRICS_INDEX, u32) -> i32;

    /// `UINT WINAPI GetDpiForWindow(HWND hwnd)`.
    type DpiForWindow = unsafe extern "system" fn(HWND) -> u32;

    /// `GetSystemMetricsForDpi` of user32, looked up once; `None` before Windows 10 1607 (or
    /// if user32 is not loaded, which never happens in a process that uses windows).
    fn system_metrics_for_dpi() -> Option<MetricsForDpi> {
        static FUNCTION: OnceLock<Option<MetricsForDpi>> = OnceLock::new();
        *FUNCTION.get_or_init(|| {
            let address = user32_export(s!("GetSystemMetricsForDpi"))?;
            // SAFETY: the export has the documented signature `MetricsForDpi` describes
            // (`int` and `UINT` parameters, `int` result, WINAPI calling convention).
            Some(unsafe {
                core::mem::transmute::<unsafe extern "system" fn() -> isize, MetricsForDpi>(address)
            })
        })
    }

    /// `GetDpiForWindow` of user32, looked up once; `None` before Windows 10 1607.
    fn dpi_for_window() -> Option<DpiForWindow> {
        static FUNCTION: OnceLock<Option<DpiForWindow>> = OnceLock::new();
        *FUNCTION.get_or_init(|| {
            let address = user32_export(s!("GetDpiForWindow"))?;
            // SAFETY: the export has the documented signature `DpiForWindow` describes (an
            // `HWND` parameter, a `UINT` result, WINAPI calling convention).
            Some(unsafe {
                core::mem::transmute::<unsafe extern "system" fn() -> isize, DpiForWindow>(address)
            })
        })
    }

    /// The export `name` of user32; `None` when user32 lacks it (or is not loaded, which never
    /// happens in a process that uses windows). The module is pinned, so the address stays
    /// valid until the process ends.
    pub(crate) fn user32_export(name: PCSTR) -> Option<unsafe extern "system" fn() -> isize> {
        let mut user32 = HMODULE::default();
        // SAFETY: a static NUL-terminated name and a valid out pointer. Nothing is loaded; PIN
        // keeps an already loaded user32 until the process ends, so a cached function pointer
        // never dangles.
        unsafe { GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_PIN, w!("user32.dll"), &mut user32) }
            .ok()?;
        // SAFETY: a pinned module and a NUL-terminated ANSI export name (callers pass `s!`
        // literals).
        unsafe { GetProcAddress(user32, name) }
    }
}
