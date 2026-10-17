//! Helpers shared by the integration tests.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};

static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// `<pid>-<n>`, unique within this test process and across concurrently running test processes.
pub fn unique_suffix() -> String {
    let n = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{n}", std::process::id())
}

/// Runs `body` on a new thread and returns its result; a panic (a failed assertion) fails the
/// calling test. Windows, menus, message queues, and per-thread message state created by `body`
/// stay on that thread, so the harness threads stay clean.
pub fn on_fresh_thread<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    match std::thread::spawn(body).join() {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[cfg(windows)]
pub use win::{ResourceFile, group_icon_ids};

#[cfg(windows)]
mod win {
    use raumakey_windows::known_folder::system_dir;
    use raumakey_windows::module::Module;
    use raumakey_windows::panic::guard_bool;
    use raumakey_windows::strings::WideString;
    use windows::Win32::Foundation::{FreeLibrary, HMODULE};
    use windows::Win32::System::LibraryLoader::{
        EnumResourceNamesW, LOAD_LIBRARY_AS_DATAFILE, LOAD_LIBRARY_AS_IMAGE_RESOURCE,
        LoadLibraryExW,
    };
    use windows::Win32::UI::WindowsAndMessaging::RT_GROUP_ICON;
    use windows::core::{BOOL, PCWSTR};

    /// A system DLL loaded as an image resource only (no code runs); freed on drop.
    pub struct ResourceFile(HMODULE);

    impl ResourceFile {
        /// Loads `name` from the system directory.
        pub fn system(name: &str) -> Self {
            let path = system_dir().unwrap().join(name);
            let path = WideString::from_os(path.as_os_str()).unwrap();
            // SAFETY: NUL-terminated path; the flags map the file without running any code.
            let module = unsafe {
                LoadLibraryExW(
                    path.as_pcwstr(),
                    None,
                    LOAD_LIBRARY_AS_IMAGE_RESOURCE | LOAD_LIBRARY_AS_DATAFILE,
                )
            }
            .unwrap();
            Self(module)
        }

        /// The module, valid while `self` lives.
        pub fn module(&self) -> Module {
            // SAFETY: the file stays loaded while `self` lives, and the tests use the result
            // only within that time.
            unsafe { Module::from_raw(self.0) }.unwrap()
        }
    }

    impl Drop for ResourceFile {
        fn drop(&mut self) {
            // SAFETY: frees the load done in `system`, once.
            let _ = unsafe { FreeLibrary(self.0) };
        }
    }

    unsafe extern "system" fn collect_integer_ids(
        _module: HMODULE,
        _kind: PCWSTR,
        name: PCWSTR,
        ids: isize,
    ) -> BOOL {
        guard_bool(|| {
            // Integer resource names are values below 0x10000 in place of a pointer.
            let value = name.0 as usize;
            if let Ok(id) = u16::try_from(value) {
                // SAFETY: `ids` is the `&mut Vec<u16>` passed to EnumResourceNamesW below,
                // valid for the whole enumeration.
                unsafe { &mut *(ids as *mut Vec<u16>) }.push(id);
            }
            true
        })
    }

    /// The integer ids of the `RT_GROUP_ICON` resources of `file`, in enumeration order.
    pub fn group_icon_ids(file: &ResourceFile) -> Vec<u16> {
        let mut ids: Vec<u16> = Vec::new();
        // SAFETY: a loaded resource module; the callback only appends to `ids`, which outlives
        // the call.
        let _ = unsafe {
            EnumResourceNamesW(
                Some(file.0),
                RT_GROUP_ICON,
                Some(collect_integer_ids),
                &mut ids as *mut Vec<u16> as isize,
            )
        };
        ids
    }
}
