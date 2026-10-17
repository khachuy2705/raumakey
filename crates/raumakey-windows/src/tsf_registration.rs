//! TSF registration of the Raumakey TIP (plan §10.4, D17, D21).
//!
//! The entries are:
//!
//! - `HKLM\Software\Classes\CLSID\{TIP_CLSID}` with the default value `"Raumakey"` and its
//!   `InprocServer32` subkey (default value = DLL path, `ThreadingModel = Apartment`), written in
//!   the registry view that matches the DLL bitness ([`TipArch::registry_view`]);
//! - the TSF language profile ([`TIP_PROFILE_GUID`], LANGID `0x042A`, description
//!   `"Raumakey"`, icon index 0 of the DLL), registered with
//!   `ITfInputProcessorProfileMgr::RegisterProfile`;
//! - the [`TIP_CATEGORIES`] (keyboard TIP and its capabilities), registered with
//!   `ITfCategoryMgr::RegisterCategory`.
//!
//! The pure part compiles on every platform: [`plan_install`]/[`plan_uninstall`] build the
//! dry-run [`RegistrationPlan`]; [`execute_registration`] registers through the narrow
//! [`RegistrationPort`] and rolls back exactly what it changed when a step fails;
//! [`execute_unregistration`] removes only Raumakey's entries; [`verify_with`] and
//! [`verify_removal_with`] read the result back through the read-only [`RegistrationReader`].
//! Tests drive all of it with fakes.
//!
//! On Windows, the TIP's `DllRegisterServer`/`DllUnregisterServer` call
//! [`register_in_process`]/[`unregister_in_process`], and the installer calls
//! [`verify_installation`]/[`verify_removal`], which only read the registry of both views.
//!
//! Only Raumakey identities appear here: [`TIP_CLSID`], [`TIP_PROFILE_GUID`], and the Windows
//! SDK category GUIDs. Plan and report lines never contain any other GUID.

use core::fmt;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use raumakey_model::identity::{
    Guid, LANGID_VIETNAMESE, PRODUCT_NAME, TIP_CLSID, TIP_PROFILE_GUID,
};

use crate::error::{E_INVALIDARG, WinError, WinResult};
use crate::registry::RegView;

/// Default value of the COM class key.
pub const COM_CLASS_NAME: &str = PRODUCT_NAME;
/// Description of the TSF language profile.
pub const PROFILE_DESCRIPTION: &str = PRODUCT_NAME;
/// Index of the profile icon inside the TIP DLL.
pub const PROFILE_ICON_INDEX: u32 = 0;
/// `ThreadingModel` of the in-process server.
pub const THREADING_MODEL: &str = "Apartment";
/// Longest accepted DLL path in UTF-16 units: COM resolves in-process servers through
/// `MAX_PATH` buffers, so a longer path could not be loaded.
pub const MAX_DLL_PATH_UNITS: usize = 259;
/// Longest accepted profile description in UTF-16 units.
pub const MAX_DESCRIPTION_UNITS: usize = 255;

/// Bitness of a TIP DLL.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TipArch {
    /// x86-64.
    X64,
    /// 32-bit x86.
    X86,
    /// ARM64.
    Arm64,
}

impl TipArch {
    /// Every architecture.
    pub const ALL: [Self; 3] = [Self::X64, Self::X86, Self::Arm64];

    /// The architecture this binary was compiled for.
    pub const fn current() -> Self {
        if cfg!(target_arch = "x86") {
            Self::X86
        } else if cfg!(target_arch = "aarch64") {
            Self::Arm64
        } else {
            Self::X64
        }
    }

    /// The registry view of a DLL of this bitness: 64-bit DLLs register in the 64-bit view,
    /// 32-bit DLLs in the 32-bit view (`WOW6432Node` on 64-bit Windows).
    pub const fn registry_view(self) -> RegView {
        match self {
            Self::X64 | Self::Arm64 => RegView::Wow64_64,
            Self::X86 => RegView::Wow64_32,
        }
    }

    /// Short name: `x64`, `x86`, or `arm64`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::X86 => "x86",
            Self::Arm64 => "arm64",
        }
    }
}

impl fmt::Display for TipArch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Human-readable name of a registry view, for plan and report lines.
const fn view_name(view: RegView) -> &'static str {
    match view {
        RegView::Default => "default view",
        RegView::Wow64_64 => "64-bit view",
        RegView::Wow64_32 => "32-bit view",
    }
}

/// A TIP DLL to register.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TipBinary {
    /// Absolute path of the DLL.
    pub path: PathBuf,
    /// Its bitness.
    pub arch: TipArch,
}

/// A TSF category the TIP registers itself in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TsfCategory {
    /// Windows SDK name of the category GUID.
    pub name: &'static str,
    /// The category GUID (Windows SDK value).
    pub guid: Guid,
}

/// The categories of plan §10.4: a keyboard TIP that supports secure mode, UI elements, the
/// input-mode compartment, COM-less activation, immersive (UWP) apps, and the system tray.
pub const TIP_CATEGORIES: [TsfCategory; 7] = [
    TsfCategory {
        name: "GUID_TFCAT_TIP_KEYBOARD",
        guid: Guid::from_u128(0x34745c63_b2f0_4784_8b67_5e12c8701a31),
    },
    TsfCategory {
        name: "GUID_TFCAT_TIPCAP_SECUREMODE",
        guid: Guid::from_u128(0x49d2f9ce_1f5e_11d7_a6d3_00065b84435c),
    },
    TsfCategory {
        name: "GUID_TFCAT_TIPCAP_UIELEMENTENABLED",
        guid: Guid::from_u128(0x49d2f9cf_1f5e_11d7_a6d3_00065b84435c),
    },
    TsfCategory {
        name: "GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT",
        guid: Guid::from_u128(0xccf05dd7_4a87_11d7_a6e2_00065b84435c),
    },
    TsfCategory {
        name: "GUID_TFCAT_TIPCAP_COMLESS",
        guid: Guid::from_u128(0x364215d9_75bc_11d7_a6ef_00065b84435c),
    },
    TsfCategory {
        name: "GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT",
        guid: Guid::from_u128(0x13a016df_560b_46cd_947a_4c3af1e0e35d),
    },
    TsfCategory {
        name: "GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT",
        guid: Guid::from_u128(0x25504fb4_7bab_4bc1_9c69_cf81890f0ef5),
    },
];

/// The entry of [`TIP_CATEGORIES`] with this GUID.
pub fn category_by_guid(guid: Guid) -> Option<&'static TsfCategory> {
    TIP_CATEGORIES.iter().find(|category| category.guid == guid)
}

/// `Software\Classes\CLSID\{TIP_CLSID}` (below `HKLM`).
pub fn com_class_key() -> String {
    format!("Software\\Classes\\CLSID\\{TIP_CLSID}")
}

/// `Software\Classes\CLSID\{TIP_CLSID}\InprocServer32` (below `HKLM`).
pub fn inproc_server_key() -> String {
    format!("{}\\InprocServer32", com_class_key())
}

/// `SOFTWARE\Microsoft\CTF\TIP\{TIP_CLSID}`: where TSF keeps the TIP's registrations (below
/// `HKLM`, per view). Raumakey only reads it; TSF writes it.
pub fn tsf_tip_key() -> String {
    format!("SOFTWARE\\Microsoft\\CTF\\TIP\\{TIP_CLSID}")
}

/// The key TSF creates for the Raumakey language profile:
/// `…\TIP\{TIP_CLSID}\LanguageProfile\0x0000042a\{TIP_PROFILE_GUID}`.
pub fn tsf_profile_key() -> String {
    format!(
        "{}\\LanguageProfile\\0x{:08x}\\{TIP_PROFILE_GUID}",
        tsf_tip_key(),
        LANGID_VIETNAMESE
    )
}

/// The key TSF creates when the TIP registers in `category`:
/// `…\TIP\{TIP_CLSID}\Category\Category\{category}\{TIP_CLSID}`.
pub fn tsf_category_key(category: Guid) -> String {
    format!(
        "{}\\Category\\Category\\{category}\\{TIP_CLSID}",
        tsf_tip_key()
    )
}

/// Why a DLL path cannot be registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DllPathError {
    Relative,
    NotADll,
    Invalid,
}

/// Checks a TIP DLL path without touching the file system (so the planner behaves the same on
/// every host): an absolute Windows path (`X:\…` or `\\server\share\…`, either separator),
/// a final component `<name>.dll`, no `.`/`..`/empty components, none of the characters
/// `" < > | ? *`, no control characters, no `:` other than the drive colon, Unicode only, and
/// at most [`MAX_DLL_PATH_UNITS`] UTF-16 units.
fn check_dll_path(path: &Path) -> Result<(), DllPathError> {
    let text = path.to_str().ok_or(DllPathError::Invalid)?;
    let (prefix_len, unc) = absolute_prefix(text).ok_or(DllPathError::Relative)?;
    let rest = text.get(prefix_len..).ok_or(DllPathError::Relative)?;
    let components: Vec<&str> = rest.split(['\\', '/']).collect();
    let file = components.last().copied().unwrap_or_default();
    let is_dll = file.len() > 4
        && file
            .get(file.len() - 4..)
            .is_some_and(|extension| extension.eq_ignore_ascii_case(".dll"));
    if !is_dll {
        return Err(DllPathError::NotADll);
    }
    // A UNC path needs a server and a share before the file.
    let minimum = if unc { 3 } else { 1 };
    let invalid_component = |component: &&str| {
        component.is_empty()
            || *component == "."
            || *component == ".."
            || component
                .chars()
                .any(|c| c.is_control() || matches!(c, '"' | '<' | '>' | '|' | '?' | '*' | ':'))
    };
    if components.len() < minimum
        || components.iter().any(invalid_component)
        || text.encode_utf16().count() > MAX_DLL_PATH_UNITS
    {
        return Err(DllPathError::Invalid);
    }
    Ok(())
}

/// Length of the absolute prefix (`X:\` or `\\`) and whether it is UNC.
fn absolute_prefix(text: &str) -> Option<(usize, bool)> {
    let bytes = text.as_bytes();
    let separator = |byte: u8| byte == b'\\' || byte == b'/';
    match bytes {
        [drive, b':', root, ..] if drive.is_ascii_alphabetic() && separator(*root) => {
            Some((3, false))
        }
        [first, second, third, ..] if separator(*first) && separator(*second) => {
            (!separator(*third)).then_some((2, true))
        }
        _ => None,
    }
}

/// Why an install plan could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlanError {
    /// No TIP binary was given.
    NoBinaries,
    /// Two binaries have the same architecture.
    DuplicateArch(TipArch),
    /// The path is not an absolute Windows path.
    RelativePath(TipArch),
    /// The path does not name a `.dll` file.
    NotADll(TipArch),
    /// The path cannot be stored as an in-process server (see [`MAX_DLL_PATH_UNITS`] and the
    /// rules of the planner).
    InvalidPath(TipArch),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBinaries => f.write_str("no TIP binary was given"),
            Self::DuplicateArch(arch) => write!(f, "more than one {arch} TIP binary was given"),
            Self::RelativePath(arch) => write!(f, "the {arch} TIP path is not absolute"),
            Self::NotADll(arch) => write!(f, "the {arch} TIP path does not name a .dll file"),
            Self::InvalidPath(arch) => {
                write!(
                    f,
                    "the {arch} TIP path cannot be registered as a COM server"
                )
            }
        }
    }
}

impl std::error::Error for PlanError {}

/// One step of a registration plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistrationStep {
    /// Write the COM class key and its `InprocServer32` subkey.
    RegisterComServer {
        /// DLL bitness.
        arch: TipArch,
        /// Registry view.
        view: RegView,
        /// DLL path.
        dll: PathBuf,
    },
    /// Register the TSF language profile.
    RegisterProfile {
        /// DLL bitness.
        arch: TipArch,
        /// DLL that holds the icon.
        dll: PathBuf,
        /// Profile description.
        description: String,
        /// Icon index inside the DLL.
        icon_index: u32,
    },
    /// Register the TIP in a category.
    RegisterCategory {
        /// DLL bitness.
        arch: TipArch,
        /// The category.
        category: TsfCategory,
    },
    /// Read every entry back.
    Verify {
        /// DLL bitness.
        arch: TipArch,
        /// Registry view.
        view: RegView,
    },
    /// Remove the TIP from a category.
    UnregisterCategory {
        /// DLL bitness.
        arch: TipArch,
        /// The category.
        category: TsfCategory,
    },
    /// Unregister the TSF language profile.
    UnregisterProfile {
        /// DLL bitness.
        arch: TipArch,
    },
    /// Delete the COM class key.
    RemoveComServer {
        /// DLL bitness.
        arch: TipArch,
        /// Registry view.
        view: RegView,
    },
    /// Check that every entry is gone.
    VerifyRemoved {
        /// DLL bitness.
        arch: TipArch,
        /// Registry view.
        view: RegView,
    },
}

impl RegistrationStep {
    /// The architecture the step belongs to.
    pub const fn arch(&self) -> TipArch {
        match self {
            Self::RegisterComServer { arch, .. }
            | Self::RegisterProfile { arch, .. }
            | Self::RegisterCategory { arch, .. }
            | Self::Verify { arch, .. }
            | Self::UnregisterCategory { arch, .. }
            | Self::UnregisterProfile { arch }
            | Self::RemoveComServer { arch, .. }
            | Self::VerifyRemoved { arch, .. } => *arch,
        }
    }

    /// One dry-run line. It names the registry keys, Raumakey's GUIDs, and the DLL path the
    /// step writes; no other GUID appears.
    pub fn describe(&self) -> String {
        let arch = self.arch();
        let view = view_name(arch.registry_view());
        let body = match self {
            Self::RegisterComServer { view, dll, .. } => format!(
                "register COM class {TIP_CLSID} \"{COM_CLASS_NAME}\": HKLM\\{} = {} \
                 (ThreadingModel={THREADING_MODEL}, {})",
                inproc_server_key(),
                dll.display(),
                view_name(*view)
            ),
            Self::RegisterProfile {
                dll,
                description,
                icon_index,
                ..
            } => format!(
                "register TSF language profile {TIP_PROFILE_GUID} of {TIP_CLSID} for LANGID \
                 0x{LANGID_VIETNAMESE:04X}: description \"{description}\", icon {icon_index} \
                 of {} ({view})",
                dll.display()
            ),
            Self::RegisterCategory { category, .. } => format!(
                "register {TIP_CLSID} in category {} {} ({view})",
                category.name, category.guid
            ),
            Self::Verify { view, .. } => format!(
                "verify: read back COM class {TIP_CLSID}, profile {TIP_PROFILE_GUID}, and {} \
                 categories ({})",
                TIP_CATEGORIES.len(),
                view_name(*view)
            ),
            Self::UnregisterCategory { category, .. } => format!(
                "remove {TIP_CLSID} from category {} {} ({view})",
                category.name, category.guid
            ),
            Self::UnregisterProfile { .. } => format!(
                "unregister TSF language profile {TIP_PROFILE_GUID} of {TIP_CLSID} for LANGID \
                 0x{LANGID_VIETNAMESE:04X} ({view})"
            ),
            Self::RemoveComServer { view, .. } => format!(
                "delete COM class {TIP_CLSID}: HKLM\\{} ({})",
                com_class_key(),
                view_name(*view)
            ),
            Self::VerifyRemoved { view, .. } => format!(
                "verify: COM class {TIP_CLSID}, profile {TIP_PROFILE_GUID}, and {} categories \
                 are gone ({})",
                TIP_CATEGORIES.len(),
                view_name(*view)
            ),
        };
        format!("[{arch}] {body}")
    }
}

/// An ordered list of registration steps (the installer's dry run prints it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegistrationPlan {
    steps: Vec<RegistrationStep>,
}

impl RegistrationPlan {
    /// The steps in execution order.
    pub fn steps(&self) -> &[RegistrationStep] {
        &self.steps
    }

    /// One line per step (see [`RegistrationStep::describe`]).
    pub fn describe(&self) -> Vec<String> {
        self.steps.iter().map(RegistrationStep::describe).collect()
    }
}

/// The install plan: for each binary, in the given order, register the COM server, the
/// profile, and every category, then verify (plan §10.2: register COM → profile and
/// categories → verify).
pub fn plan_install(binaries: &[TipBinary]) -> Result<RegistrationPlan, PlanError> {
    if binaries.is_empty() {
        return Err(PlanError::NoBinaries);
    }
    let mut seen = BTreeSet::new();
    let mut steps = Vec::with_capacity(binaries.len() * (TIP_CATEGORIES.len() + 3));
    for binary in binaries {
        let arch = binary.arch;
        if !seen.insert(arch) {
            return Err(PlanError::DuplicateArch(arch));
        }
        check_dll_path(&binary.path).map_err(|error| match error {
            DllPathError::Relative => PlanError::RelativePath(arch),
            DllPathError::NotADll => PlanError::NotADll(arch),
            DllPathError::Invalid => PlanError::InvalidPath(arch),
        })?;
        let view = arch.registry_view();
        steps.push(RegistrationStep::RegisterComServer {
            arch,
            view,
            dll: binary.path.clone(),
        });
        steps.push(RegistrationStep::RegisterProfile {
            arch,
            dll: binary.path.clone(),
            description: PROFILE_DESCRIPTION.to_owned(),
            icon_index: PROFILE_ICON_INDEX,
        });
        steps.extend(
            TIP_CATEGORIES
                .iter()
                .map(|category| RegistrationStep::RegisterCategory {
                    arch,
                    category: *category,
                }),
        );
        steps.push(RegistrationStep::Verify { arch, view });
    }
    Ok(RegistrationPlan { steps })
}

/// The uninstall plan: for each architecture (duplicates ignored), the install steps in
/// reverse (categories, profile, COM server), then a removal check. Only Raumakey's entries
/// are named.
pub fn plan_uninstall(archs: &[TipArch]) -> RegistrationPlan {
    let mut seen = BTreeSet::new();
    let mut steps = Vec::new();
    for &arch in archs {
        if !seen.insert(arch) {
            continue;
        }
        let view = arch.registry_view();
        steps.extend(TIP_CATEGORIES.iter().rev().map(|category| {
            RegistrationStep::UnregisterCategory {
                arch,
                category: *category,
            }
        }));
        steps.push(RegistrationStep::UnregisterProfile { arch });
        steps.push(RegistrationStep::RemoveComServer { arch, view });
        steps.push(RegistrationStep::VerifyRemoved { arch, view });
    }
    RegistrationPlan { steps }
}

/// The tracked values of the COM class key.
///
/// `Debug` shows the class name and threading model but only the length of the DLL path, which
/// may contain a user-profile path.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ComClassState {
    /// Default value of `CLSID\{TIP_CLSID}`.
    pub class_name: Option<String>,
    /// Default value of `InprocServer32` (the DLL path).
    pub inproc_server: Option<String>,
    /// `ThreadingModel` of `InprocServer32`.
    pub threading_model: Option<String>,
}

impl fmt::Debug for ComClassState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let server = self
            .inproc_server
            .as_ref()
            .map(|path| format!("<{} chars>", path.chars().count()));
        f.debug_struct("ComClassState")
            .field("class_name", &self.class_name)
            .field("inproc_server", &server)
            .field("threading_model", &self.threading_model)
            .finish()
    }
}

/// Mutating access to the registrations of one view (the process's own one in production).
pub trait RegistrationPort {
    /// The COM class values; `Ok(None)` when the class key does not exist.
    fn read_com_class(&mut self) -> Result<Option<ComClassState>, WinError>;
    /// Writes the class key (default [`COM_CLASS_NAME`]) and `InprocServer32` (`dll`,
    /// [`THREADING_MODEL`]).
    fn write_com_class(&mut self, dll: &Path) -> Result<(), WinError>;
    /// Restores the values read earlier; `None` deletes the class key.
    fn restore_com_class(&mut self, previous: Option<&ComClassState>) -> Result<(), WinError>;
    /// Whether the Raumakey language profile is registered.
    fn profile_registered(&mut self) -> Result<bool, WinError>;
    /// Registers the Raumakey language profile.
    fn register_profile(
        &mut self,
        dll: &Path,
        description: &str,
        icon_index: u32,
    ) -> Result<(), WinError>;
    /// Unregisters the Raumakey language profile.
    fn unregister_profile(&mut self) -> Result<(), WinError>;
    /// Which of the [`TIP_CATEGORIES`] the TIP is registered in.
    fn registered_categories(&mut self) -> Result<Vec<Guid>, WinError>;
    /// Registers the TIP in `category`.
    fn register_category(&mut self, category: Guid) -> Result<(), WinError>;
    /// Removes the TIP from `category`.
    fn unregister_category(&mut self, category: Guid) -> Result<(), WinError>;
}

/// Checks the inputs of [`execute_registration`].
fn check_registration_inputs(dll: &Path, description: &str) -> WinResult<()> {
    if check_dll_path(dll).is_err() {
        return Err(WinError::new(E_INVALIDARG, "TIP DLL path"));
    }
    let valid_description = !description.is_empty()
        && !description.contains('\0')
        && description.encode_utf16().count() <= MAX_DESCRIPTION_UNITS;
    if !valid_description {
        return Err(WinError::new(E_INVALIDARG, "TIP profile description"));
    }
    Ok(())
}

/// What [`execute_registration`] changed so far, for the rollback.
struct Undo {
    previous_com: Option<ComClassState>,
    profile_existed: bool,
    existing_categories: BTreeSet<Guid>,
    com_touched: bool,
    profile_touched: bool,
    categories_touched: Vec<Guid>,
}

impl Undo {
    /// Reverts every change in reverse order. Categories and the profile that existed before
    /// are left registered; the COM values are restored to what was read. Errors are ignored:
    /// the caller reports the failure that caused the rollback.
    fn run<P: RegistrationPort + ?Sized>(&self, port: &mut P) {
        for category in self.categories_touched.iter().rev() {
            if !self.existing_categories.contains(category) {
                let _ = port.unregister_category(*category);
            }
        }
        if self.profile_touched && !self.profile_existed {
            let _ = port.unregister_profile();
        }
        if self.com_touched {
            let _ = port.restore_com_class(self.previous_com.as_ref());
        }
    }
}

/// Registers the COM server, the language profile, and every category of [`TIP_CATEGORIES`],
/// in that order. Inputs are checked before anything changes (`InvalidInput`).
///
/// When a step fails, the changes made so far are rolled back exactly: categories and a profile
/// that did not exist before are removed, and the COM values are restored to the values read
/// at the start (or the class key is deleted). A profile or category that existed before stays
/// registered. The error of the failed step is returned. Registering twice is idempotent.
pub fn execute_registration<P: RegistrationPort + ?Sized>(
    port: &mut P,
    dll: &Path,
    description: &str,
    icon_index: u32,
) -> WinResult<()> {
    check_registration_inputs(dll, description)?;
    let mut undo = Undo {
        previous_com: port.read_com_class()?,
        profile_existed: port.profile_registered()?,
        existing_categories: port.registered_categories()?.into_iter().collect(),
        com_touched: false,
        profile_touched: false,
        categories_touched: Vec::new(),
    };
    undo.com_touched = true;
    if let Err(error) = port.write_com_class(dll) {
        undo.run(port);
        return Err(error);
    }
    undo.profile_touched = true;
    if let Err(error) = port.register_profile(dll, description, icon_index) {
        undo.run(port);
        return Err(error);
    }
    for category in &TIP_CATEGORIES {
        undo.categories_touched.push(category.guid);
        if let Err(error) = port.register_category(category.guid) {
            undo.run(port);
            return Err(error);
        }
    }
    Ok(())
}

/// Removes the TIP from every registered category of [`TIP_CATEGORIES`], unregisters the
/// profile, and deletes the COM class key, touching nothing else. Every removal is attempted
/// even after a failure; the first error is returned. Removing twice is idempotent.
pub fn execute_unregistration<P: RegistrationPort + ?Sized>(port: &mut P) -> WinResult<()> {
    let mut first_error = None;
    let mut record = |result: WinResult<()>| {
        if let Err(error) = result {
            first_error.get_or_insert(error);
        }
    };
    match port.registered_categories() {
        Ok(registered) => {
            for category in TIP_CATEGORIES.iter().rev() {
                if registered.contains(&category.guid) {
                    record(port.unregister_category(category.guid));
                }
            }
        }
        Err(error) => record(Err(error)),
    }
    match port.profile_registered() {
        Ok(true) => record(port.unregister_profile()),
        Ok(false) => {}
        Err(error) => record(Err(error)),
    }
    match port.read_com_class() {
        Ok(Some(_)) => record(port.restore_com_class(None)),
        Ok(None) => {}
        Err(error) => record(Err(error)),
    }
    first_error.map_or(Ok(()), Err)
}

/// Read-only access to the registrations of both views.
pub trait RegistrationReader {
    /// The COM class values in `view`; `Ok(None)` when the class key does not exist.
    fn com_class(&mut self, view: RegView) -> Result<Option<ComClassState>, WinError>;
    /// Whether the Raumakey language profile is registered in `view`.
    fn profile_present(&mut self, view: RegView) -> Result<bool, WinError>;
    /// Whether the TIP is registered in `category` in `view`.
    fn category_present(&mut self, view: RegView, category: Guid) -> Result<bool, WinError>;
}

/// What a verification check looked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckItem {
    /// The COM class key and its default value [`COM_CLASS_NAME`].
    ComClass,
    /// `InprocServer32` names the expected DLL (case and separator style are ignored).
    InprocServer,
    /// `ThreadingModel` is [`THREADING_MODEL`].
    ThreadingModel,
    /// The language profile.
    Profile,
    /// One of the [`TIP_CATEGORIES`].
    Category(Guid),
}

/// What the entries are expected to look like.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Expectation {
    /// Registered as planned (after an install).
    #[default]
    Installed,
    /// Absent (after an uninstall).
    Removed,
}

/// One verification result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationCheck {
    /// Architecture (and so view) checked.
    pub arch: TipArch,
    /// What was checked.
    pub item: CheckItem,
    /// Whether the entry matches the expectation.
    pub passed: bool,
    /// The read error, when the entry could not be read (the check then fails).
    pub error: Option<WinError>,
}

impl VerificationCheck {
    fn new(arch: TipArch, item: CheckItem, outcome: Result<bool, WinError>) -> Self {
        match outcome {
            Ok(passed) => Self {
                arch,
                item,
                passed,
                error: None,
            },
            Err(error) => Self {
                arch,
                item,
                passed: false,
                error: Some(error),
            },
        }
    }

    fn subject(&self) -> String {
        match self.item {
            CheckItem::ComClass => format!("COM class {TIP_CLSID}"),
            CheckItem::InprocServer => format!("InprocServer32 of {TIP_CLSID}"),
            CheckItem::ThreadingModel => format!("ThreadingModel of {TIP_CLSID}"),
            CheckItem::Profile => format!("language profile {TIP_PROFILE_GUID}"),
            CheckItem::Category(guid) => match category_by_guid(guid) {
                Some(category) => format!("category {} {}", category.name, category.guid),
                None => "category (not a Raumakey category)".to_owned(),
            },
        }
    }
}

/// The result of reading the registrations back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VerificationReport {
    expectation: Expectation,
    checks: Vec<VerificationCheck>,
}

impl VerificationReport {
    /// What the checks expected.
    pub const fn expectation(&self) -> Expectation {
        self.expectation
    }

    /// Every check in order.
    pub fn checks(&self) -> &[VerificationCheck] {
        &self.checks
    }

    /// True when every check passed (and there was at least one).
    pub fn all_passed(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|check| check.passed)
    }

    /// The checks that failed.
    pub fn failures(&self) -> impl Iterator<Item = &VerificationCheck> {
        self.checks.iter().filter(|check| !check.passed)
    }

    /// One line per check: architecture, subject, view, and outcome (with the error code when
    /// a read failed). Paths and other registry values are never included.
    pub fn describe(&self) -> Vec<String> {
        self.checks
            .iter()
            .map(|check| {
                let outcome = match (self.expectation, check.passed, check.error) {
                    (_, false, Some(error)) => format!("FAILED (0x{:08X})", error.code() as u32),
                    (Expectation::Installed, true, None) => "ok".to_owned(),
                    (Expectation::Installed, false, None) => "FAILED".to_owned(),
                    (Expectation::Removed, true, None) => "removed".to_owned(),
                    (Expectation::Removed, false, None) => "still present".to_owned(),
                    (_, true, Some(_)) => "ok".to_owned(),
                };
                format!(
                    "[{}] {} ({}): {outcome}",
                    check.arch,
                    check.subject(),
                    view_name(check.arch.registry_view())
                )
            })
            .collect()
    }
}

/// Compares registry and expected DLL paths ignoring case and separator style.
fn same_path(found: &str, expected: &Path) -> bool {
    let normalize = |text: &str| text.replace('/', "\\").to_lowercase();
    normalize(found) == normalize(&expected.to_string_lossy())
}

/// Reads back every entry of every `expected` binary in its view: COM class name,
/// `InprocServer32` path and threading model, profile, and each category (3 + 1 + 7 checks per
/// binary).
pub fn verify_with<R: RegistrationReader + ?Sized>(
    reader: &mut R,
    expected: &[TipBinary],
) -> VerificationReport {
    let mut checks = Vec::new();
    for binary in expected {
        let arch = binary.arch;
        let view = arch.registry_view();
        match reader.com_class(view) {
            Ok(com) => {
                let com = com.unwrap_or_default();
                let class_ok = com
                    .class_name
                    .as_deref()
                    .is_some_and(|name| name == COM_CLASS_NAME);
                let server_ok = com
                    .inproc_server
                    .as_deref()
                    .is_some_and(|path| same_path(path, &binary.path));
                let model_ok = com
                    .threading_model
                    .as_deref()
                    .is_some_and(|model| model.eq_ignore_ascii_case(THREADING_MODEL));
                checks.push(VerificationCheck::new(
                    arch,
                    CheckItem::ComClass,
                    Ok(class_ok),
                ));
                checks.push(VerificationCheck::new(
                    arch,
                    CheckItem::InprocServer,
                    Ok(server_ok),
                ));
                checks.push(VerificationCheck::new(
                    arch,
                    CheckItem::ThreadingModel,
                    Ok(model_ok),
                ));
            }
            Err(error) => {
                for item in [
                    CheckItem::ComClass,
                    CheckItem::InprocServer,
                    CheckItem::ThreadingModel,
                ] {
                    checks.push(VerificationCheck::new(arch, item, Err(error)));
                }
            }
        }
        checks.push(VerificationCheck::new(
            arch,
            CheckItem::Profile,
            reader.profile_present(view),
        ));
        for category in &TIP_CATEGORIES {
            checks.push(VerificationCheck::new(
                arch,
                CheckItem::Category(category.guid),
                reader.category_present(view, category.guid),
            ));
        }
    }
    VerificationReport {
        expectation: Expectation::Installed,
        checks,
    }
}

/// Checks that the COM class key, the profile, and every category are gone in the view of each
/// architecture (duplicates ignored; 1 + 1 + 7 checks each). A check passes when the entry is
/// absent.
pub fn verify_removal_with<R: RegistrationReader + ?Sized>(
    reader: &mut R,
    archs: &[TipArch],
) -> VerificationReport {
    let mut seen = BTreeSet::new();
    let mut checks = Vec::new();
    for &arch in archs {
        if !seen.insert(arch) {
            continue;
        }
        let view = arch.registry_view();
        checks.push(VerificationCheck::new(
            arch,
            CheckItem::ComClass,
            reader.com_class(view).map(|com| com.is_none()),
        ));
        checks.push(VerificationCheck::new(
            arch,
            CheckItem::Profile,
            reader.profile_present(view).map(|present| !present),
        ));
        for category in &TIP_CATEGORIES {
            checks.push(VerificationCheck::new(
                arch,
                CheckItem::Category(category.guid),
                reader
                    .category_present(view, category.guid)
                    .map(|present| !present),
            ));
        }
    }
    VerificationReport {
        expectation: Expectation::Removed,
        checks,
    }
}

#[cfg(windows)]
pub use win::{
    WinRegistrationReader, register_in_process, unregister_in_process, verify_installation,
    verify_removal,
};

#[cfg(windows)]
mod win {
    use std::path::Path;

    use raumakey_model::identity::{Guid, LANGID_VIETNAMESE, TIP_CLSID, TIP_PROFILE_GUID};
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
    use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
    use windows::Win32::UI::TextServices::{
        CLSID_TF_CategoryMgr, CLSID_TF_InputProcessorProfiles, ITfCategoryMgr,
        ITfInputProcessorProfileMgr,
    };
    use windows::core::GUID;

    use super::{
        COM_CLASS_NAME, ComClassState, RegistrationPort, RegistrationReader, THREADING_MODEL,
        TIP_CATEGORIES, TipArch, TipBinary, VerificationReport, com_class_key,
        execute_registration, execute_unregistration, inproc_server_key, tsf_category_key,
        tsf_profile_key, verify_removal_with, verify_with,
    };
    use crate::com::ComApartment;
    use crate::error::{E_INVALIDARG, ResultExt, WinError, WinResult};
    use crate::registry::{
        OwnedRegKey, RegKey, RegRoot, RegValue, RegView, delete_empty_key, delete_tree, key_exists,
    };
    use crate::strings::WideString;

    /// Name of the in-process server subkey.
    const INPROC_SERVER: &str = "InprocServer32";
    /// Name of the threading model value.
    const THREADING_MODEL_VALUE: &str = "ThreadingModel";

    fn win_guid(guid: Guid) -> GUID {
        GUID::from_u128(guid.to_u128())
    }

    /// A string value; `None` when it is missing or not a string (an unusable value).
    fn string_value(key: &RegKey, name: &str) -> WinResult<Option<String>> {
        Ok(match key.get_value(name)? {
            Some(RegValue::String(text) | RegValue::ExpandString(text)) => Some(text),
            _ => None,
        })
    }

    fn read_com_class(view: RegView) -> WinResult<Option<ComClassState>> {
        let Some(class) = RegKey::open_read(RegRoot::LocalMachine, view, &com_class_key())? else {
            return Ok(None);
        };
        let class_name = string_value(&class, "")?;
        let (inproc_server, threading_model) = match class.open_subkey(INPROC_SERVER)? {
            Some(server) => (
                string_value(&server, "")?,
                string_value(&server, THREADING_MODEL_VALUE)?,
            ),
            None => (None, None),
        };
        Ok(Some(ComClassState {
            class_name,
            inproc_server,
            threading_model,
        }))
    }

    fn profile_present(view: RegView) -> WinResult<bool> {
        key_exists(RegRoot::LocalMachine, view, &tsf_profile_key())
    }

    fn category_present(view: RegView, category: Guid) -> WinResult<bool> {
        key_exists(RegRoot::LocalMachine, view, &tsf_category_key(category))
    }

    fn set_or_delete(key: &OwnedRegKey, name: &str, value: Option<&str>) -> WinResult<()> {
        match value {
            Some(text) => key.set_string(name, text),
            None => key.delete_value(name).map(|_| ()),
        }
    }

    /// Read-only [`RegistrationReader`] over `HKLM` in both views (opens keys with `KEY_READ`
    /// only).
    #[derive(Clone, Copy, Debug, Default)]
    pub struct WinRegistrationReader;

    impl RegistrationReader for WinRegistrationReader {
        fn com_class(&mut self, view: RegView) -> Result<Option<ComClassState>, WinError> {
            read_com_class(view)
        }

        fn profile_present(&mut self, view: RegView) -> Result<bool, WinError> {
            profile_present(view)
        }

        fn category_present(&mut self, view: RegView, category: Guid) -> Result<bool, WinError> {
            category_present(view, category)
        }
    }

    /// The registrations of this process's view: COM keys through the owned-path registry
    /// guard, profile and categories through the TSF managers. Field order releases the COM
    /// interfaces before the apartment guard.
    struct WinRegistrationPort {
        profiles: ITfInputProcessorProfileMgr,
        categories: ITfCategoryMgr,
        view: RegView,
        _apartment: ComApartment,
    }

    impl WinRegistrationPort {
        fn new() -> WinResult<Self> {
            let apartment = ComApartment::sta()?;
            // SAFETY: COM is initialized on this thread for the lifetime of the result (the
            // apartment guard is a field dropped after the interfaces).
            let profiles: ITfInputProcessorProfileMgr = unsafe {
                CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
            }
            .context("CoCreateInstance(TF_InputProcessorProfiles)")?;
            // SAFETY: as above.
            let categories: ITfCategoryMgr =
                unsafe { CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER) }
                    .context("CoCreateInstance(TF_CategoryMgr)")?;
            Ok(Self {
                profiles,
                categories,
                // The DLL runs in a process of its own bitness, whose TSF writes this view.
                view: TipArch::current().registry_view(),
                _apartment: apartment,
            })
        }
    }

    impl RegistrationPort for WinRegistrationPort {
        fn read_com_class(&mut self) -> Result<Option<ComClassState>, WinError> {
            read_com_class(self.view)
        }

        fn write_com_class(&mut self, dll: &Path) -> Result<(), WinError> {
            let dll = dll
                .to_str()
                .ok_or_else(|| WinError::new(E_INVALIDARG, "TIP DLL path"))?;
            let class = OwnedRegKey::create(RegRoot::LocalMachine, self.view, &com_class_key())?;
            class.set_string("", COM_CLASS_NAME)?;
            let server = class.create_subkey(INPROC_SERVER)?;
            server.set_string("", dll)?;
            server.set_string(THREADING_MODEL_VALUE, THREADING_MODEL)
        }

        fn restore_com_class(&mut self, previous: Option<&ComClassState>) -> Result<(), WinError> {
            let Some(previous) = previous else {
                delete_tree(RegRoot::LocalMachine, self.view, &com_class_key())?;
                return Ok(());
            };
            let class = OwnedRegKey::create(RegRoot::LocalMachine, self.view, &com_class_key())?;
            set_or_delete(&class, "", previous.class_name.as_deref())?;
            let server = class.create_subkey(INPROC_SERVER)?;
            set_or_delete(&server, "", previous.inproc_server.as_deref())?;
            set_or_delete(
                &server,
                THREADING_MODEL_VALUE,
                previous.threading_model.as_deref(),
            )?;
            if previous.inproc_server.is_none() && previous.threading_model.is_none() {
                drop(server);
                // Removes the subkey only when nothing else lives in it.
                delete_empty_key(RegRoot::LocalMachine, self.view, &inproc_server_key())?;
            }
            Ok(())
        }

        fn profile_registered(&mut self) -> Result<bool, WinError> {
            profile_present(self.view)
        }

        fn register_profile(
            &mut self,
            dll: &Path,
            description: &str,
            icon_index: u32,
        ) -> Result<(), WinError> {
            let description = WideString::new(description)?;
            let icon_file = WideString::from_os(dll.as_os_str())?;
            let clsid = win_guid(TIP_CLSID);
            let profile = win_guid(TIP_PROFILE_GUID);
            // SAFETY: the GUIDs live on this frame; both slices are borrowed from
            // NUL-terminated buffers that outlive the call (the counts exclude the NUL).
            unsafe {
                self.profiles.RegisterProfile(
                    &clsid,
                    LANGID_VIETNAMESE,
                    &profile,
                    description.as_units(),
                    icon_file.as_units(),
                    icon_index,
                    HKL::default(),
                    0,
                    true,
                    0,
                )
            }
            .context("ITfInputProcessorProfileMgr::RegisterProfile")
        }

        fn unregister_profile(&mut self) -> Result<(), WinError> {
            let clsid = win_guid(TIP_CLSID);
            let profile = win_guid(TIP_PROFILE_GUID);
            // SAFETY: the GUIDs live on this frame for the duration of the call.
            unsafe {
                self.profiles
                    .UnregisterProfile(&clsid, LANGID_VIETNAMESE, &profile, 0)
            }
            .context("ITfInputProcessorProfileMgr::UnregisterProfile")
        }

        fn registered_categories(&mut self) -> Result<Vec<Guid>, WinError> {
            let mut registered = Vec::new();
            for category in &TIP_CATEGORIES {
                if category_present(self.view, category.guid)? {
                    registered.push(category.guid);
                }
            }
            Ok(registered)
        }

        fn register_category(&mut self, category: Guid) -> Result<(), WinError> {
            let clsid = win_guid(TIP_CLSID);
            let category = win_guid(category);
            // SAFETY: the GUIDs live on this frame for the duration of the call.
            unsafe { self.categories.RegisterCategory(&clsid, &category, &clsid) }
                .context("ITfCategoryMgr::RegisterCategory")
        }

        fn unregister_category(&mut self, category: Guid) -> Result<(), WinError> {
            let clsid = win_guid(TIP_CLSID);
            let category = win_guid(category);
            // SAFETY: the GUIDs live on this frame for the duration of the call.
            unsafe {
                self.categories
                    .UnregisterCategory(&clsid, &category, &clsid)
            }
            .context("ITfCategoryMgr::UnregisterCategory")
        }
    }

    /// `DllRegisterServer` body of the TIP: registers the COM server (`HKLM`, in the view of
    /// this process, which has the DLL's bitness), the language profile, and the categories,
    /// rolling back exactly on failure (see [`super::execute_registration`]). Requires an
    /// elevated caller. COM is initialized for the call when the thread has no apartment.
    pub fn register_in_process(
        dll_path: &Path,
        description: &str,
        icon_index: u32,
    ) -> WinResult<()> {
        let mut port = WinRegistrationPort::new()?;
        execute_registration(&mut port, dll_path, description, icon_index)
    }

    /// `DllUnregisterServer` body of the TIP: removes the categories, the profile, and the COM
    /// class of this process's view (see [`super::execute_unregistration`]). Requires an
    /// elevated caller.
    pub fn unregister_in_process() -> WinResult<()> {
        let mut port = WinRegistrationPort::new()?;
        execute_unregistration(&mut port)
    }

    /// Reads back every entry of every `expected` binary in its view (read-only; see
    /// [`super::verify_with`]).
    pub fn verify_installation(expected: &[TipBinary]) -> VerificationReport {
        verify_with(&mut WinRegistrationReader, expected)
    }

    /// Checks that every Raumakey entry is gone in the view of each architecture (read-only;
    /// see [`super::verify_removal_with`]).
    pub fn verify_removal(archs: &[TipArch]) -> VerificationReport {
        verify_removal_with(&mut WinRegistrationReader, archs)
    }
}
