//! `tsf_registration`: the pure planner, the registration/rollback logic over a fake port, and
//! verification over a fake reader. The Windows executor never runs in tests.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use raumakey_model::identity::{Guid, LANGID_VIETNAMESE, TIP_CLSID, TIP_PROFILE_GUID};
use raumakey_windows::error::{E_FAIL, WinError, WinErrorKind};
use raumakey_windows::registry::RegView;
use raumakey_windows::tsf_registration::{
    CheckItem, ComClassState, PlanError, RegistrationPort, RegistrationReader, RegistrationStep,
    TIP_CATEGORIES, TipArch, TipBinary, com_class_key, execute_registration,
    execute_unregistration, inproc_server_key, plan_install, plan_uninstall, tsf_category_key,
    tsf_profile_key, verify_removal_with, verify_with,
};

fn x64() -> TipBinary {
    TipBinary {
        path: PathBuf::from("C:\\Program Files\\Raumakey\\x64\\raumakey_tip.dll"),
        arch: TipArch::X64,
    }
}

fn x86() -> TipBinary {
    TipBinary {
        path: PathBuf::from("C:\\Program Files\\Raumakey\\x86\\raumakey_tip.dll"),
        arch: TipArch::X86,
    }
}

/// Every `{…}` GUID token in `text`.
fn guid_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let Some(len) = rest[start..].find('}') else {
            break;
        };
        tokens.push(rest[start..=start + len].to_owned());
        rest = &rest[start + len + 1..];
    }
    tokens
}

fn allowed_guids() -> BTreeSet<String> {
    let mut allowed: BTreeSet<String> = TIP_CATEGORIES
        .iter()
        .map(|category| category.guid.to_string())
        .collect();
    allowed.insert(TIP_CLSID.to_string());
    allowed.insert(TIP_PROFILE_GUID.to_string());
    allowed
}

#[test]
fn categories_are_the_keyboard_tip_and_its_capabilities() {
    let names: Vec<&str> = TIP_CATEGORIES
        .iter()
        .map(|category| category.name)
        .collect();
    assert_eq!(
        names,
        [
            "GUID_TFCAT_TIP_KEYBOARD",
            "GUID_TFCAT_TIPCAP_SECUREMODE",
            "GUID_TFCAT_TIPCAP_UIELEMENTENABLED",
            "GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT",
            "GUID_TFCAT_TIPCAP_COMLESS",
            "GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT",
            "GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT",
        ]
    );
    assert_eq!(
        TIP_CATEGORIES[0].guid,
        Guid::parse("{34745C63-B2F0-4784-8B67-5E12C8701A31}").unwrap()
    );
}

#[cfg(windows)]
#[test]
fn category_guids_match_the_windows_sdk() {
    use windows::Win32::UI::TextServices::{
        GUID_TFCAT_TIP_KEYBOARD, GUID_TFCAT_TIPCAP_COMLESS, GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
        GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT, GUID_TFCAT_TIPCAP_SECUREMODE,
        GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT, GUID_TFCAT_TIPCAP_UIELEMENTENABLED,
    };
    let sdk = [
        GUID_TFCAT_TIP_KEYBOARD,
        GUID_TFCAT_TIPCAP_SECUREMODE,
        GUID_TFCAT_TIPCAP_UIELEMENTENABLED,
        GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT,
        GUID_TFCAT_TIPCAP_COMLESS,
        GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
        GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
    ];
    for (category, expected) in TIP_CATEGORIES.iter().zip(sdk) {
        assert_eq!(
            category.guid.to_u128(),
            expected.to_u128(),
            "{}",
            category.name
        );
    }
}

#[test]
fn registry_paths_name_only_raumakey_identities() {
    let clsid = TIP_CLSID.to_string();
    assert_eq!(
        com_class_key(),
        format!("Software\\Classes\\CLSID\\{clsid}")
    );
    assert_eq!(
        inproc_server_key(),
        format!("Software\\Classes\\CLSID\\{clsid}\\InprocServer32")
    );
    assert_eq!(
        tsf_profile_key(),
        format!(
            "SOFTWARE\\Microsoft\\CTF\\TIP\\{clsid}\\LanguageProfile\\0x0000042a\\{TIP_PROFILE_GUID}"
        )
    );
    assert_eq!(
        tsf_category_key(TIP_CATEGORIES[0].guid),
        format!(
            "SOFTWARE\\Microsoft\\CTF\\TIP\\{clsid}\\Category\\Category\\{}\\{clsid}",
            TIP_CATEGORIES[0].guid
        )
    );
    assert_eq!(LANGID_VIETNAMESE, 0x042A);
}

#[test]
fn the_current_architecture_is_the_build_target() {
    let expected = if cfg!(target_arch = "x86") {
        TipArch::X86
    } else if cfg!(target_arch = "aarch64") {
        TipArch::Arm64
    } else {
        TipArch::X64
    };
    assert_eq!(TipArch::current(), expected);
    assert_eq!(TipArch::X64.to_string(), "x64");
    assert_eq!(TipArch::X86.to_string(), "x86");
    assert_eq!(TipArch::Arm64.to_string(), "arm64");
}

#[test]
fn plan_errors_display_without_paths() {
    let errors = [
        PlanError::NoBinaries,
        PlanError::DuplicateArch(TipArch::X64),
        PlanError::RelativePath(TipArch::X86),
        PlanError::NotADll(TipArch::Arm64),
        PlanError::InvalidPath(TipArch::X64),
    ];
    for error in errors {
        let text = error.to_string();
        assert!(!text.is_empty());
        assert!(!text.contains('\\'), "{text}");
    }
    assert!(
        PlanError::NotADll(TipArch::Arm64)
            .to_string()
            .contains("arm64")
    );
}

#[test]
fn views_follow_dll_bitness() {
    assert_eq!(TipArch::X64.registry_view(), RegView::Wow64_64);
    assert_eq!(TipArch::Arm64.registry_view(), RegView::Wow64_64);
    assert_eq!(TipArch::X86.registry_view(), RegView::Wow64_32);
}

#[test]
fn install_plan_registers_com_profile_categories_then_verifies_per_binary() {
    let plan = plan_install(&[x64(), x86()]).unwrap();
    let steps = plan.steps();
    assert_eq!(steps.len(), 2 * (1 + 1 + TIP_CATEGORIES.len() + 1));
    assert_eq!(
        steps[0],
        RegistrationStep::RegisterComServer {
            arch: TipArch::X64,
            view: RegView::Wow64_64,
            dll: x64().path,
        }
    );
    assert!(matches!(
        steps[1],
        RegistrationStep::RegisterProfile {
            arch: TipArch::X64,
            icon_index: 0,
            ..
        }
    ));
    assert!(matches!(
        steps[2],
        RegistrationStep::RegisterCategory {
            arch: TipArch::X64,
            ..
        }
    ));
    assert_eq!(
        steps[9],
        RegistrationStep::Verify {
            arch: TipArch::X64,
            view: RegView::Wow64_64
        }
    );
    assert_eq!(
        steps[10],
        RegistrationStep::RegisterComServer {
            arch: TipArch::X86,
            view: RegView::Wow64_32,
            dll: x86().path,
        }
    );
    if let RegistrationStep::RegisterProfile { description, .. } = &steps[1] {
        assert_eq!(description, "Raumakey");
    }
}

#[test]
fn dry_run_lines_name_the_raumakey_identities_and_nothing_else() {
    let lines = plan_install(&[x64(), x86()]).unwrap().describe();
    let text = lines.join("\n");
    assert!(text.contains(&TIP_CLSID.to_string()));
    assert!(text.contains(&TIP_PROFILE_GUID.to_string()));
    assert!(text.contains("0x042A"));
    assert!(text.contains("64-bit view") && text.contains("32-bit view"));
    for category in &TIP_CATEGORIES {
        assert!(text.contains(category.name), "{}", category.name);
    }
    let allowed = allowed_guids();
    for token in guid_tokens(&text) {
        assert!(
            allowed.contains(&token),
            "unexpected GUID {token} in:\n{text}"
        );
    }
    assert_eq!(
        lines.len(),
        plan_install(&[x64(), x86()]).unwrap().steps().len()
    );

    let uninstall = plan_uninstall(&[TipArch::X64, TipArch::X86])
        .describe()
        .join("\n");
    assert!(uninstall.contains(&TIP_CLSID.to_string()));
    for token in guid_tokens(&uninstall) {
        assert!(allowed.contains(&token), "unexpected GUID {token}");
    }
}

#[test]
fn uninstall_plan_reverses_the_install_order() {
    let plan = plan_uninstall(&[TipArch::X86]);
    let steps = plan.steps();
    assert!(matches!(
        steps[0],
        RegistrationStep::UnregisterCategory {
            arch: TipArch::X86,
            ..
        }
    ));
    assert_eq!(
        steps[TIP_CATEGORIES.len()],
        RegistrationStep::UnregisterProfile { arch: TipArch::X86 }
    );
    assert_eq!(
        steps[TIP_CATEGORIES.len() + 1],
        RegistrationStep::RemoveComServer {
            arch: TipArch::X86,
            view: RegView::Wow64_32
        }
    );
    assert_eq!(
        steps[TIP_CATEGORIES.len() + 2],
        RegistrationStep::VerifyRemoved {
            arch: TipArch::X86,
            view: RegView::Wow64_32
        }
    );
}

#[test]
fn install_plans_reject_bad_inputs() {
    assert_eq!(plan_install(&[]).unwrap_err(), PlanError::NoBinaries);
    assert_eq!(
        plan_install(&[x64(), x64()]).unwrap_err(),
        PlanError::DuplicateArch(TipArch::X64)
    );
    let relative = TipBinary {
        path: PathBuf::from("raumakey_tip.dll"),
        arch: TipArch::X64,
    };
    assert_eq!(
        plan_install(&[relative]).unwrap_err(),
        PlanError::RelativePath(TipArch::X64)
    );
    let not_dll = TipBinary {
        path: PathBuf::from("C:\\Program Files\\Raumakey\\raumakey.exe"),
        arch: TipArch::X86,
    };
    assert_eq!(
        plan_install(&[not_dll]).unwrap_err(),
        PlanError::NotADll(TipArch::X86)
    );
    let quoted = TipBinary {
        path: PathBuf::from("C:\\a\"b\\raumakey_tip.dll"),
        arch: TipArch::X86,
    };
    assert_eq!(
        plan_install(&[quoted]).unwrap_err(),
        PlanError::InvalidPath(TipArch::X86)
    );
}

fn binary(path: &str, arch: TipArch) -> TipBinary {
    TipBinary {
        path: PathBuf::from(path),
        arch,
    }
}

#[test]
fn install_plans_accept_drive_unc_and_unicode_paths() {
    for path in [
        "C:\\Program Files\\Raumakey\\raumakey_tip.dll",
        "D:/Raumakey/RAUMAKEY_TIP.DLL",
        "\\\\server\\share\\Raumakey\\raumakey_tip.dll",
        "C:\\Users\\Nguyễn Văn A\\Raumakey\\raumakey_tip.dll",
    ] {
        assert!(
            plan_install(&[binary(path, TipArch::X64)]).is_ok(),
            "{path}"
        );
    }
}

#[test]
fn install_plans_reject_relative_traversal_and_unloadable_paths() {
    for path in [
        "\\Raumakey\\raumakey_tip.dll",
        "C:raumakey_tip.dll",
        "Raumakey\\raumakey_tip.dll",
        "\\\\",
    ] {
        assert_eq!(
            plan_install(&[binary(path, TipArch::X64)]).unwrap_err(),
            PlanError::RelativePath(TipArch::X64),
            "{path}"
        );
    }
    for path in [
        "C:\\Raumakey\\raumakey_tip.dll.exe",
        "C:\\Raumakey\\.dll",
        "C:\\Raumakey\\",
    ] {
        assert_eq!(
            plan_install(&[binary(path, TipArch::X86)]).unwrap_err(),
            PlanError::NotADll(TipArch::X86),
            "{path}"
        );
    }
    let long = format!("C:\\{}\\raumakey_tip.dll", "a".repeat(250));
    for path in [
        "C:\\Raumakey\\..\\raumakey_tip.dll",
        "C:\\Raumakey\\.\\raumakey_tip.dll",
        "C:\\Raumakey\\\\raumakey_tip.dll",
        "C:\\Rau\tmakey\\raumakey_tip.dll",
        "C:\\Raumakey\\raumakey|tip.dll",
        "C:\\Raumakey\\raumakey_tip.dll:stream.dll",
        "C:\\Raumakey\\rau*.dll",
        long.as_str(),
    ] {
        assert_eq!(
            plan_install(&[binary(path, TipArch::Arm64)]).unwrap_err(),
            PlanError::InvalidPath(TipArch::Arm64),
            "{path}"
        );
    }
}

#[test]
fn dry_run_lines_name_the_dll_and_the_com_keys() {
    let plan = plan_install(&[x64()]).unwrap();
    let lines = plan.describe();
    let first = &lines[0];
    assert!(
        first.contains("C:\\Program Files\\Raumakey\\x64\\raumakey_tip.dll"),
        "{first}"
    );
    assert!(first.contains("InprocServer32"), "{first}");
    assert!(first.contains("Apartment"), "{first}");
    assert!(lines[1].contains("Raumakey"), "{}", lines[1]);
    assert!(
        lines.iter().all(|line| line.starts_with("[x64] ")),
        "{lines:#?}"
    );
    let uninstall = plan_uninstall(&[TipArch::X86]).describe();
    assert!(
        uninstall.iter().all(|line| line.starts_with("[x86] ")),
        "{uninstall:#?}"
    );
    // Duplicate architectures collapse into one removal.
    assert_eq!(
        plan_uninstall(&[TipArch::X86, TipArch::X86]).steps().len(),
        TIP_CATEGORIES.len() + 3
    );
    assert!(plan_uninstall(&[]).steps().is_empty());
}

#[test]
fn registration_rejects_invalid_inputs_before_changing_anything() {
    let cases = [
        (PathBuf::from("raumakey_tip.dll"), "Raumakey"),
        (PathBuf::from("C:\\Raumakey\\raumakey_tip.exe"), "Raumakey"),
        (
            PathBuf::from("C:\\Rau\"makey\\raumakey_tip.dll"),
            "Raumakey",
        ),
        (dll(), ""),
        (dll(), "Rau\0makey"),
    ];
    for (path, description) in cases {
        let mut port = FakePort::default();
        let error = execute_registration(&mut port, &path, description, 0).unwrap_err();
        assert_eq!(
            error.kind(),
            WinErrorKind::InvalidInput,
            "{path:?} {description:?}"
        );
        assert!(port.log.is_empty(), "{:?}", port.log);
        assert_eq!(port.state, FakeState::default());
    }
}

/// In-memory registration state with failure injection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FakeState {
    com: Option<ComClassState>,
    profile: Option<(PathBuf, String, u32)>,
    categories: BTreeSet<Guid>,
}

#[derive(Default)]
struct FakePort {
    state: FakeState,
    /// Fail the n-th mutating call (1-based); 0 = never.
    fail_at: usize,
    /// Fail the unregistration of this category (it stays registered).
    fail_unregister: Option<Guid>,
    calls: usize,
    log: Vec<String>,
}

impl FakePort {
    fn tick(&mut self, what: &str) -> Result<(), WinError> {
        self.calls += 1;
        self.log.push(what.to_owned());
        if self.calls == self.fail_at {
            Err(WinError::new(E_FAIL, "injected"))
        } else {
            Ok(())
        }
    }
}

impl RegistrationPort for FakePort {
    fn read_com_class(&mut self) -> Result<Option<ComClassState>, WinError> {
        Ok(self.state.com.clone())
    }

    fn write_com_class(&mut self, dll: &Path) -> Result<(), WinError> {
        // A partial write before failing: the class key appears first.
        self.state.com = Some(ComClassState {
            class_name: Some("Raumakey".to_owned()),
            inproc_server: None,
            threading_model: None,
        });
        self.tick("write_com_class")?;
        self.state.com = Some(ComClassState {
            class_name: Some("Raumakey".to_owned()),
            inproc_server: Some(dll.to_string_lossy().into_owned()),
            threading_model: Some("Apartment".to_owned()),
        });
        Ok(())
    }

    fn restore_com_class(&mut self, previous: Option<&ComClassState>) -> Result<(), WinError> {
        self.log.push("restore_com_class".to_owned());
        self.state.com = previous.cloned();
        Ok(())
    }

    fn profile_registered(&mut self) -> Result<bool, WinError> {
        Ok(self.state.profile.is_some())
    }

    fn register_profile(
        &mut self,
        dll: &Path,
        description: &str,
        icon_index: u32,
    ) -> Result<(), WinError> {
        self.tick("register_profile")?;
        self.state.profile = Some((dll.to_path_buf(), description.to_owned(), icon_index));
        Ok(())
    }

    fn unregister_profile(&mut self) -> Result<(), WinError> {
        self.log.push("unregister_profile".to_owned());
        self.state.profile = None;
        Ok(())
    }

    fn registered_categories(&mut self) -> Result<Vec<Guid>, WinError> {
        Ok(self.state.categories.iter().copied().collect())
    }

    fn register_category(&mut self, category: Guid) -> Result<(), WinError> {
        self.tick("register_category")?;
        self.state.categories.insert(category);
        Ok(())
    }

    fn unregister_category(&mut self, category: Guid) -> Result<(), WinError> {
        self.log.push("unregister_category".to_owned());
        if self.fail_unregister == Some(category) {
            return Err(WinError::new(E_FAIL, "injected unregister"));
        }
        self.state.categories.remove(&category);
        Ok(())
    }
}

fn dll() -> PathBuf {
    x64().path
}

fn installed() -> FakeState {
    FakeState {
        com: Some(ComClassState {
            class_name: Some("Raumakey".to_owned()),
            inproc_server: Some(dll().to_string_lossy().into_owned()),
            threading_model: Some("Apartment".to_owned()),
        }),
        profile: Some((dll(), "Raumakey".to_owned(), 0)),
        categories: TIP_CATEGORIES
            .iter()
            .map(|category| category.guid)
            .collect(),
    }
}

#[test]
fn registration_writes_com_profile_and_every_category() {
    let mut port = FakePort::default();
    execute_registration(&mut port, &dll(), "Raumakey", 0).unwrap();
    assert_eq!(port.state, installed());
}

#[test]
fn every_failure_point_rolls_back_to_the_initial_state() {
    let mutating_calls = 1 + 1 + TIP_CATEGORIES.len();
    for fail_at in 1..=mutating_calls {
        let mut port = FakePort {
            fail_at,
            ..FakePort::default()
        };
        let error = execute_registration(&mut port, &dll(), "Raumakey", 0).unwrap_err();
        assert_eq!(error.context(), "injected");
        assert_eq!(
            port.state,
            FakeState::default(),
            "fail_at {fail_at}: {:?}",
            port.log
        );
    }
}

#[test]
fn rollback_keeps_what_existed_before() {
    let previous_com = ComClassState {
        class_name: Some("Raumakey".to_owned()),
        inproc_server: Some("C:\\old\\raumakey_tip.dll".to_owned()),
        threading_model: Some("Apartment".to_owned()),
    };
    let before = FakeState {
        com: Some(previous_com),
        profile: Some((
            PathBuf::from("C:\\old\\raumakey_tip.dll"),
            "Raumakey".to_owned(),
            0,
        )),
        categories: [TIP_CATEGORIES[0].guid].into_iter().collect(),
    };
    let last = 1 + 1 + TIP_CATEGORIES.len();
    let mut port = FakePort {
        state: before.clone(),
        fail_at: last,
        ..FakePort::default()
    };
    execute_registration(&mut port, &dll(), "Raumakey", 0).unwrap_err();
    // The COM values come back, the pre-existing category stays, and the pre-existing profile
    // is not unregistered (it was re-registered with the new DLL, which rollback cannot undo).
    assert_eq!(port.state.com, before.com);
    assert_eq!(port.state.categories, before.categories);
    assert!(port.state.profile.is_some());
    assert!(!port.log.contains(&"unregister_profile".to_owned()));
}

#[test]
fn unregistration_removes_every_raumakey_entry() {
    let mut port = FakePort {
        state: installed(),
        ..FakePort::default()
    };
    execute_unregistration(&mut port).unwrap();
    assert_eq!(port.state, FakeState::default());
    // Idempotent.
    execute_unregistration(&mut port).unwrap();
    assert_eq!(port.state, FakeState::default());
}

#[test]
fn unregistration_is_best_effort_and_reports_the_first_error() {
    let stuck = TIP_CATEGORIES[2].guid;
    let mut port = FakePort {
        state: installed(),
        fail_unregister: Some(stuck),
        ..FakePort::default()
    };
    let error = execute_unregistration(&mut port).unwrap_err();
    assert_eq!(error.context(), "injected unregister");
    // Everything else is still removed.
    assert_eq!(port.state.categories, [stuck].into_iter().collect());
    assert!(port.state.profile.is_none());
    assert!(port.state.com.is_none());
}

#[test]
fn com_class_debug_output_hides_the_dll_path() {
    let state = ComClassState {
        class_name: Some("Raumakey".to_owned()),
        inproc_server: Some("C:\\Users\\Secret Name\\raumakey_tip.dll".to_owned()),
        threading_model: Some("Apartment".to_owned()),
    };
    let debug = format!("{state:?}");
    assert!(!debug.contains("Secret"), "{debug}");
    assert!(
        debug.contains("Raumakey") && debug.contains("Apartment"),
        "{debug}"
    );
}

#[test]
fn registration_twice_is_idempotent() {
    let mut port = FakePort::default();
    execute_registration(&mut port, &dll(), "Raumakey", 0).unwrap();
    execute_registration(&mut port, &dll(), "Raumakey", 0).unwrap();
    assert_eq!(port.state, installed());
}

/// Read-only view for verification.
struct FakeReader {
    state: FakeState,
    error: bool,
}

impl RegistrationReader for FakeReader {
    fn com_class(&mut self, _view: RegView) -> Result<Option<ComClassState>, WinError> {
        if self.error {
            return Err(WinError::new(E_FAIL, "read"));
        }
        Ok(self.state.com.clone())
    }

    fn profile_present(&mut self, _view: RegView) -> Result<bool, WinError> {
        Ok(self.state.profile.is_some())
    }

    fn category_present(&mut self, _view: RegView, category: Guid) -> Result<bool, WinError> {
        Ok(self.state.categories.contains(&category))
    }
}

#[test]
fn verification_reports_each_check() {
    let mut reader = FakeReader {
        state: installed(),
        error: false,
    };
    let report = verify_with(&mut reader, &[x64()]);
    assert!(report.all_passed(), "{:?}", report.describe());
    assert_eq!(report.checks().len(), 3 + 1 + TIP_CATEGORIES.len());

    // Path comparison ignores case and separator style.
    let mut reader = FakeReader {
        state: installed(),
        error: false,
    };
    if let Some(com) = reader.state.com.as_mut() {
        com.inproc_server = Some("c:/program files/raumakey/x64/RAUMAKEY_TIP.DLL".to_owned());
    }
    assert!(verify_with(&mut reader, &[x64()]).all_passed());

    let mut reader = FakeReader {
        state: installed(),
        error: false,
    };
    reader.state.categories.remove(&TIP_CATEGORIES[3].guid);
    if let Some(com) = reader.state.com.as_mut() {
        com.inproc_server = Some("C:\\elsewhere\\raumakey_tip.dll".to_owned());
    }
    let report = verify_with(&mut reader, &[x64()]);
    assert!(!report.all_passed());
    let failed: Vec<CheckItem> = report.failures().map(|check| check.item).collect();
    assert_eq!(
        failed,
        [
            CheckItem::InprocServer,
            CheckItem::Category(TIP_CATEGORIES[3].guid)
        ]
    );
    // Lines describe failures without the DLL path.
    let lines = report.describe().join("\n");
    assert!(!lines.contains("elsewhere"), "{lines}");

    let mut failing = FakeReader {
        state: installed(),
        error: true,
    };
    let report = verify_with(&mut failing, &[x64()]);
    assert!(!report.all_passed());
    assert!(report.failures().any(|check| check.error.is_some()));
}

#[test]
fn verification_lines_cover_every_check_without_paths() {
    let mut reader = FakeReader {
        state: installed(),
        error: false,
    };
    let report = verify_with(&mut reader, &[x64()]);
    let lines = report.describe();
    assert_eq!(lines.len(), report.checks().len());
    for category in &TIP_CATEGORIES {
        assert!(
            lines.iter().any(|line| line.contains(category.name)),
            "{}",
            category.name
        );
    }
    assert!(
        lines.iter().all(|line| line.ends_with(": ok")),
        "{lines:#?}"
    );
    assert!(lines.iter().all(|line| !line.contains("raumakey_tip.dll")));
    assert!(lines.iter().all(|line| line.contains("64-bit view")));

    let mut reader = FakeReader {
        state: FakeState::default(),
        error: false,
    };
    let report = verify_with(&mut reader, &[x86()]);
    let lines = report.describe();
    assert!(
        lines.iter().all(|line| line.ends_with(": FAILED")),
        "{lines:#?}"
    );
    assert!(lines.iter().all(|line| line.contains("32-bit view")));

    let mut failing = FakeReader {
        state: installed(),
        error: true,
    };
    let lines = verify_with(&mut failing, &[x64()]).describe();
    assert!(lines[0].contains("0x80004005"), "{}", lines[0]);
}

#[test]
fn removal_verification_checks_class_profile_and_categories_per_view() {
    let mut reader = FakeReader {
        state: installed(),
        error: false,
    };
    let report = verify_removal_with(&mut reader, &[TipArch::X64, TipArch::X86]);
    assert_eq!(report.checks().len(), 2 * (2 + TIP_CATEGORIES.len()));
    assert_eq!(report.failures().count(), report.checks().len());
    reader.state.categories.clear();
    reader.state.profile = None;
    let report = verify_removal_with(&mut reader, &[TipArch::X64]);
    let failed: Vec<CheckItem> = report.failures().map(|check| check.item).collect();
    assert_eq!(failed, [CheckItem::ComClass]);
    assert!(
        report
            .describe()
            .iter()
            .any(|line| line.ends_with(": still present")),
        "{:?}",
        report.describe()
    );
    let mut failing = FakeReader {
        state: FakeState::default(),
        error: true,
    };
    assert!(!verify_removal_with(&mut failing, &[TipArch::X64]).all_passed());
}

#[test]
fn removal_verification_requires_every_entry_to_be_gone() {
    let mut reader = FakeReader {
        state: FakeState::default(),
        error: false,
    };
    assert!(verify_removal_with(&mut reader, &[TipArch::X64]).all_passed());
    let mut reader = FakeReader {
        state: installed(),
        error: false,
    };
    let report = verify_removal_with(&mut reader, &[TipArch::X64]);
    assert!(!report.all_passed());
}

#[cfg(windows)]
mod win {
    use std::path::PathBuf;

    use raumakey_windows::tsf_registration::{TipArch, TipBinary, verify_installation};

    #[test]
    fn verification_of_a_machine_without_raumakey_reports_failures() {
        // Read-only: only HKLM keys are opened, with KEY_READ.
        let expected = TipBinary {
            path: PathBuf::from("C:\\Raumakey-Test-Not-Installed\\raumakey_tip.dll"),
            arch: TipArch::X64,
        };
        let report = verify_installation(&[expected]);
        assert!(!report.all_passed());
    }

    #[test]
    fn removal_verification_agrees_with_the_registry() {
        use raumakey_windows::registry::{RegRoot, key_exists};
        use raumakey_windows::tsf_registration::{CheckItem, com_class_key, verify_removal};

        // Read-only; independent of whether Raumakey is installed on this machine.
        let report = verify_removal(&[TipArch::X64, TipArch::X86]);
        assert_eq!(report.checks().len(), 2 * (2 + super::TIP_CATEGORIES.len()));
        for arch in [TipArch::X64, TipArch::X86] {
            let present = key_exists(
                RegRoot::LocalMachine,
                arch.registry_view(),
                &com_class_key(),
            )
            .unwrap();
            let check = report
                .checks()
                .iter()
                .find(|check| check.arch == arch && check.item == CheckItem::ComClass)
                .unwrap();
            assert_eq!(check.passed, !present, "{arch}");
        }
    }

    /// Writes HKLM and registers a TSF profile. Manual test only, elevated.
    #[test]
    #[ignore = "manual: registers the TIP machine-wide (HKLM, TSF); run elevated"]
    fn manual_register_and_unregister_in_process() {
        let dll = std::env::var_os("RAUMAKEY_TIP_DLL").expect("set RAUMAKEY_TIP_DLL");
        raumakey_windows::tsf_registration::register_in_process(
            std::path::Path::new(&dll),
            "Raumakey",
            0,
        )
        .unwrap();
        raumakey_windows::tsf_registration::unregister_in_process().unwrap();
    }
}
