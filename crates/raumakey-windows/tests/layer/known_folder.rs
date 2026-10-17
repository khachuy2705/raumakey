//! `known_folder`: read-only folder lookups.

use std::path::Path;

use raumakey_windows::known_folder::{
    current_exe_dir, local_app_data, program_files, startup_folder, system_dir, syswow64_dir,
    windows_dir,
};

fn ends_with_ignore_case(path: &Path, suffix: &str) -> bool {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(&suffix.to_ascii_lowercase())
}

#[test]
fn local_app_data_is_the_users_local_profile_folder() {
    let path = local_app_data().unwrap();
    assert!(path.is_absolute());
    assert!(path.is_dir());
    if let Some(env) = std::env::var_os("LOCALAPPDATA") {
        assert!(
            path.as_os_str().eq_ignore_ascii_case(&env),
            "{} vs {}",
            path.display(),
            Path::new(&env).display()
        );
    }
}

#[test]
fn startup_and_program_files_resolve() {
    let startup = startup_folder().unwrap();
    assert!(startup.is_absolute());
    assert!(
        ends_with_ignore_case(&startup, "Startup"),
        "{}",
        startup.display()
    );
    let program_files = program_files().unwrap();
    assert!(program_files.is_absolute());
    if cfg!(target_pointer_width = "64") {
        assert!(
            ends_with_ignore_case(&program_files, "Program Files"),
            "{}",
            program_files.display()
        );
    }
}

#[test]
fn executable_and_system_directories_resolve() {
    let exe_dir = current_exe_dir().unwrap();
    assert_eq!(exe_dir, std::env::current_exe().unwrap().parent().unwrap());
    let windows = windows_dir().unwrap();
    assert!(windows.is_dir());
    let system = system_dir().unwrap();
    assert!(system.is_dir());
    assert!(system.starts_with(&windows));
    if cfg!(target_pointer_width = "64") {
        assert!(
            ends_with_ignore_case(&system, "System32"),
            "{}",
            system.display()
        );
        let wow = syswow64_dir()
            .unwrap()
            .expect("64-bit Windows has SysWOW64");
        assert!(ends_with_ignore_case(&wow, "SysWOW64"), "{}", wow.display());
    }
}
