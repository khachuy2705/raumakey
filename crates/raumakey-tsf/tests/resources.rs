//! The Windows resources of `raumakey_tip.dll` (plan §11, design §13).
//!
//! `build.rs` links the resources of `raumakey_resources::Artifact::Tip` (version
//! information and the app, `V`, and `E` icon groups) into the DLL only. TSF shows the icon
//! that `ITfInputProcessorProfileMgr::RegisterProfile` names by an index into the DLL's icon
//! groups, and that index is defined twice (design §20.7): as
//! `raumakey_windows::tsf_registration::PROFILE_ICON_INDEX` for registration and as
//! `raumakey_resources::TIP_PROFILE_ICON_INDEX` beside the icons. The tests below keep the two
//! in step and, on Windows, check the DLL that Cargo built next to this test binary.

use raumakey_resources::{Artifact, ICON_APP, IconGlyph, TIP_PROFILE_ICON_INDEX};
use raumakey_windows::tsf_registration::PROFILE_ICON_INDEX;

#[test]
fn registration_and_resources_use_the_same_profile_icon_index() {
    assert_eq!(PROFILE_ICON_INDEX, TIP_PROFILE_ICON_INDEX);
}

#[test]
fn the_profile_icon_index_names_the_app_mark_of_the_tip() {
    // An icon index counts the icon groups in resource order: named groups first (the TIP has
    // none), then integer ordinals in ascending order.
    let mut groups = Artifact::Tip.icon_groups().to_vec();
    groups.sort_by_key(|(ordinal, _)| *ordinal);
    let index = usize::try_from(PROFILE_ICON_INDEX).expect("a small index");
    assert_eq!(
        groups.get(index).copied(),
        Some((ICON_APP, IconGlyph::AppMark))
    );
}

#[cfg(windows)]
mod win {
    use std::fs;
    use std::path::PathBuf;

    use raumakey_model::identity::{Guid, TIP_CLSID, TIP_PROFILE_GUID};
    use raumakey_resources::Artifact;
    use raumakey_windows::strings::WideString;
    use raumakey_windows::tsf_registration::PROFILE_ICON_INDEX;
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

    /// `raumakey_tip.dll` as Cargo built it for this test run: the library is compiled once
    /// for both of its crate types, so the DLL lies in `deps` beside this test binary.
    fn built_dll() -> PathBuf {
        let exe = std::env::current_exe().expect("the test binary has a path");
        let dll = exe
            .parent()
            .expect("the test binary lies in a directory")
            .join("raumakey_tip.dll");
        assert!(dll.is_file(), "{} was not built", dll.display());
        dll
    }

    fn utf16le(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    /// `guid` as COM lays it out in memory: `Data1`, `Data2`, and `Data3` little-endian, then
    /// the eight bytes of `Data4`.
    fn binary_form(guid: Guid) -> Vec<u8> {
        let mut bytes = guid.data1().to_le_bytes().to_vec();
        bytes.extend(guid.data2().to_le_bytes());
        bytes.extend(guid.data3().to_le_bytes());
        bytes.extend(guid.data4());
        bytes
    }

    /// `xtask inspect` looks for both identities as GUID structures in the image.
    #[test]
    fn the_dll_carries_its_class_and_profile_identities_in_binary_form() {
        let dll = fs::read(built_dll()).expect("the DLL is readable");
        for (name, guid) in [
            ("TIP_CLSID", TIP_CLSID),
            ("TIP_PROFILE_GUID", TIP_PROFILE_GUID),
        ] {
            assert!(
                contains(&dll, &binary_form(guid)),
                "{name} {guid} is not in the image"
            );
        }
    }

    #[test]
    fn only_the_dll_carries_the_version_information_of_the_tip() {
        let dll = fs::read(built_dll()).expect("the DLL is readable");
        // VS_VERSIONINFO strings: FileDescription and OriginalFilename.
        let description = utf16le(Artifact::Tip.description());
        let original_filename = utf16le(Artifact::Tip.original_filename());
        assert!(contains(&dll, &description), "no FileDescription");
        assert!(contains(&dll, &original_filename), "no OriginalFilename");
        let this_test = std::env::current_exe()
            .and_then(fs::read)
            .expect("the test binary is readable");
        assert!(
            !contains(&this_test, &description),
            "a test binary has the TIP's version information"
        );
    }

    #[test]
    fn the_profile_icon_of_the_registration_loads_from_the_dll() {
        let path = WideString::from_os(built_dll().as_os_str()).expect("a path without NUL");
        // SAFETY: a NUL-terminated path; with index -1 and no output arrays the call only
        // counts the icons. The file is mapped as data, so no code of the DLL runs.
        let count = unsafe { ExtractIconExW(path.as_pcwstr(), -1, None, None, 0) };
        assert_eq!(
            usize::try_from(count).ok(),
            Some(Artifact::Tip.icon_groups().len())
        );

        let index = i32::try_from(PROFILE_ICON_INDEX).expect("a small index");
        let mut large = HICON::default();
        // SAFETY: as above; `large` receives at most the one icon asked for.
        let extracted =
            unsafe { ExtractIconExW(path.as_pcwstr(), index, Some(&mut large), None, 1) };
        assert_eq!(extracted, 1, "no icon at the profile icon index");
        assert!(!large.is_invalid());
        // SAFETY: the icon was created by ExtractIconExW for this test and is destroyed once.
        unsafe { DestroyIcon(large) }.expect("the icon is destroyed");
    }
}
