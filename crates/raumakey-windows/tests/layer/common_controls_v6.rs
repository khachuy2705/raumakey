//! Common Controls 6 for test dialogs. `raumakey.exe`'s manifest asks for
//! `Microsoft.Windows.Common-Controls` 6.0, but a test binary has no manifest, so its controls
//! are those of Common Controls 5, which behave otherwise: only version 6 searches a list view
//! for the letters typed into it, and it hands a caller of `LVM_GETITEMTEXTW` the text a
//! virtual list view lent, besides a copy. A test that needs version 6 holds a
//! [`CommonControls6`] on its own thread while it creates its windows: an activation context
//! made from a manifest like `raumakey.exe`'s, which the threads started meanwhile inherit (a
//! dialog of `spawn_dialog` included).
//!
//! The tests of `raumakey-controller` include this file too (`#[path]`), so it uses the
//! `windows` crate, `tempfile`, and the standard library only.

#![allow(dead_code)]

use core::ffi::c_void;
use core::marker::PhantomData;
use std::os::windows::ffi::OsStrExt;

/// The application manifest's dependency on Common Controls 6, as in `raumakey.exe`.
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
        processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;

/// `CCM_GETVERSION`: the version of Common Controls a control belongs to (5 or 6).
pub const CCM_GETVERSION: u32 = 0x2008;

/// `ACTCTXW`, with only `lpSource` (the manifest file) set.
#[repr(C)]
struct ActCtxW {
    size: u32,
    flags: u32,
    source: *const u16,
    processor_architecture: u16,
    language: u16,
    assembly_directory: *const u16,
    resource_name: *const u16,
    application_name: *const u16,
    module: *mut c_void,
}

// The `windows` crate of this workspace leaves out the activation context functions.
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateActCtxW(context: *const ActCtxW) -> *mut c_void;
    fn ActivateActCtx(context: *mut c_void, cookie: *mut usize) -> i32;
    fn DeactivateActCtx(flags: u32, cookie: usize) -> i32;
    fn ReleaseActCtx(context: *mut c_void);
}

/// Common Controls 6 is active on this thread while this lives (see the module
/// documentation). It stays on the thread that made it, which must deactivate it.
pub struct CommonControls6 {
    context: *mut c_void,
    cookie: usize,
    /// The manifest, removed once the context is released.
    _folder: tempfile::TempDir,
    /// Activation contexts belong to a thread.
    _thread: PhantomData<*const ()>,
}

impl CommonControls6 {
    /// Activates Common Controls 6 on this thread; fails the test when Windows refuses.
    pub fn activate() -> Self {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("common-controls-6.manifest");
        std::fs::write(&path, MANIFEST).unwrap();
        let source: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let request = ActCtxW {
            size: size_of::<ActCtxW>() as u32,
            flags: 0,
            source: source.as_ptr(),
            processor_architecture: 0,
            language: 0,
            assembly_directory: core::ptr::null(),
            resource_name: core::ptr::null(),
            application_name: core::ptr::null(),
            module: core::ptr::null_mut(),
        };
        // SAFETY: a fully initialized request whose manifest path is NUL-terminated and
        // outlives the call, which reads the manifest and keeps no pointer to either.
        let context = unsafe { CreateActCtxW(&request) };
        assert_ne!(
            context as isize,
            -1,
            "CreateActCtxW: {}",
            std::io::Error::last_os_error()
        );
        let mut cookie = 0_usize;
        // SAFETY: a context made above and a valid out pointer.
        if unsafe { ActivateActCtx(context, &mut cookie) } == 0 {
            let error = std::io::Error::last_os_error();
            // SAFETY: releases the context made above, which nothing uses.
            unsafe { ReleaseActCtx(context) };
            panic!("ActivateActCtx: {error}");
        }
        Self {
            context,
            cookie,
            _folder: folder,
            _thread: PhantomData,
        }
    }
}

impl Drop for CommonControls6 {
    fn drop(&mut self) {
        // SAFETY: deactivates, on the thread that activated it (the guard is not `Send`), the
        // context this guard activated, then releases the reference `CreateActCtxW` returned;
        // the windows made meanwhile keep their classes.
        unsafe {
            DeactivateActCtx(0, self.cookie);
            ReleaseActCtx(self.context);
        }
    }
}
