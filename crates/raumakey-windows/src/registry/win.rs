//! Win32 registry calls behind the guard of the parent module.

use core::ops::Deref;

use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_PATH_NOT_FOUND, WIN32_ERROR,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ENUMERATE_SUB_KEYS, KEY_QUERY_VALUE, KEY_READ,
    KEY_SET_VALUE, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_VALUE_TYPE, RegCloseKey,
    RegCreateKeyExW, RegDeleteKeyExW, RegDeleteTreeW, RegDeleteValueW, RegEnumKeyExW,
    RegEnumValueW, RegOpenKeyExW, RegQueryInfoKeyW, RegQueryValueExW, RegSetValueExW,
};
use windows::core::{PCWSTR, PWSTR};

use super::{RegRoot, RegValue, RegView, is_owned_path};
use crate::error::{E_ACCESSDENIED, E_INVALIDARG, WinError, WinResult, check_win32, win32};
use crate::strings::WideString;

/// `DELETE` standard access right.
const DELETE: u32 = 0x0001_0000;
/// Longest key name in characters.
const MAX_KEY_NAME: usize = 255;
/// Retries when a value grows between the size query and the read.
const MAX_RETRIES: usize = 8;

/// An open key handle, closed on drop.
struct KeyHandle(HKEY);

// SAFETY: registry handles are not tied to the opening thread, and the registry API is
// thread-safe; the handle is closed exactly once in `Drop`.
unsafe impl Send for KeyHandle {}
// SAFETY: see `Send`; all operations through a shared reference are thread-safe API calls.
unsafe impl Sync for KeyHandle {}

impl Drop for KeyHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by this module and is closed exactly once.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn root_key(root: RegRoot) -> HKEY {
    match root {
        RegRoot::CurrentUser => HKEY_CURRENT_USER,
        RegRoot::LocalMachine => HKEY_LOCAL_MACHINE,
    }
}

fn access(rights: u32, view: RegView) -> REG_SAM_FLAGS {
    REG_SAM_FLAGS(rights | view.sam_flags())
}

fn is_missing(status: WIN32_ERROR) -> bool {
    status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND
}

fn guard(root: RegRoot, path: &str) -> WinResult<()> {
    if is_owned_path(root, path) {
        Ok(())
    } else {
        Err(WinError::new(E_ACCESSDENIED, "registry write guard"))
    }
}

fn type_mismatch() -> WinError {
    WinError::from_win32(win32::ERROR_INVALID_DATA, "registry value type")
}

fn open_raw(parent: HKEY, path: &str, rights: u32, view: RegView) -> WinResult<Option<KeyHandle>> {
    let wide = WideString::new(path)?;
    let mut key = HKEY::default();
    // SAFETY: `parent` is a predefined or open key, the name outlives the call, and `key` is a
    // valid out pointer; the opened key is owned by `KeyHandle`.
    let status = unsafe {
        RegOpenKeyExW(
            parent,
            wide.as_pcwstr(),
            None,
            access(rights, view),
            &mut key,
        )
    };
    if is_missing(status) {
        return Ok(None);
    }
    check_win32(status, "RegOpenKeyExW")?;
    Ok(Some(KeyHandle(key)))
}

/// A key opened for reading (`KEY_READ`).
pub struct RegKey {
    key: KeyHandle,
    root: RegRoot,
    view: RegView,
    path: String,
}

impl core::fmt::Debug for RegKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RegKey")
            .field("root", &self.root)
            .field("view", &self.view)
            .finish_non_exhaustive()
    }
}

impl RegKey {
    /// Opens `root\path` read-only in `view`; `Ok(None)` when the key does not exist.
    pub fn open_read(root: RegRoot, view: RegView, path: &str) -> WinResult<Option<Self>> {
        Ok(
            open_raw(root_key(root), path, KEY_READ.0, view)?.map(|key| Self {
                key,
                root,
                view,
                path: path.to_owned(),
            }),
        )
    }

    /// Opens a subkey (relative path) read-only; `Ok(None)` when it does not exist.
    pub fn open_subkey(&self, name: &str) -> WinResult<Option<Self>> {
        Ok(
            open_raw(self.key.0, name, KEY_READ.0, self.view)?.map(|key| Self {
                key,
                root: self.root,
                view: self.view,
                path: format!("{}\\{name}", self.path),
            }),
        )
    }

    /// The root of the key.
    pub const fn root(&self) -> RegRoot {
        self.root
    }

    /// The view the key was opened in.
    pub const fn view(&self) -> RegView {
        self.view
    }

    /// The path below the root.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Reads value `name` (`""` is the default value); `Ok(None)` when it does not exist.
    pub fn get_value(&self, name: &str) -> WinResult<Option<RegValue>> {
        let wide = WideString::new(name)?;
        let mut size = 0_u32;
        for _ in 0..MAX_RETRIES {
            let mut kind = REG_VALUE_TYPE::default();
            // SAFETY: size query with no data buffer; the out pointers are valid.
            let status = unsafe {
                RegQueryValueExW(
                    self.key.0,
                    wide.as_pcwstr(),
                    None,
                    Some(&raw mut kind),
                    None,
                    Some(&raw mut size),
                )
            };
            if is_missing(status) {
                return Ok(None);
            }
            check_win32(status, "RegQueryValueExW")?;
            let mut data = vec![0_u8; size as usize];
            let mut len = size;
            // SAFETY: `data` has `len` writable bytes; the out pointers are valid.
            let status = unsafe {
                RegQueryValueExW(
                    self.key.0,
                    wide.as_pcwstr(),
                    None,
                    Some(&raw mut kind),
                    Some(data.as_mut_ptr()),
                    Some(&raw mut len),
                )
            };
            if status == ERROR_MORE_DATA {
                size = len;
                continue;
            }
            if is_missing(status) {
                return Ok(None);
            }
            check_win32(status, "RegQueryValueExW")?;
            data.truncate(len as usize);
            return Ok(Some(RegValue::from_raw(kind.0, &data)));
        }
        Err(WinError::from_win32(
            win32::ERROR_MORE_DATA,
            "RegQueryValueExW",
        ))
    }

    /// A `REG_SZ` or `REG_EXPAND_SZ` value (not expanded). Another type is an `InvalidInput`
    /// error.
    pub fn get_string(&self, name: &str) -> WinResult<Option<String>> {
        match self.get_value(name)? {
            None => Ok(None),
            Some(RegValue::String(text) | RegValue::ExpandString(text)) => Ok(Some(text)),
            Some(_) => Err(type_mismatch()),
        }
    }

    /// A `REG_DWORD` value.
    pub fn get_dword(&self, name: &str) -> WinResult<Option<u32>> {
        match self.get_value(name)? {
            None => Ok(None),
            Some(RegValue::Dword(value)) => Ok(Some(value)),
            Some(_) => Err(type_mismatch()),
        }
    }

    /// A `REG_QWORD` value.
    pub fn get_qword(&self, name: &str) -> WinResult<Option<u64>> {
        match self.get_value(name)? {
            None => Ok(None),
            Some(RegValue::Qword(value)) => Ok(Some(value)),
            Some(_) => Err(type_mismatch()),
        }
    }

    /// A `REG_MULTI_SZ` value.
    pub fn get_multi_string(&self, name: &str) -> WinResult<Option<Vec<String>>> {
        match self.get_value(name)? {
            None => Ok(None),
            Some(RegValue::MultiString(items)) => Ok(Some(items)),
            Some(_) => Err(type_mismatch()),
        }
    }

    /// A `REG_BINARY` value.
    pub fn get_binary(&self, name: &str) -> WinResult<Option<Vec<u8>>> {
        match self.get_value(name)? {
            None => Ok(None),
            Some(RegValue::Binary(bytes)) => Ok(Some(bytes)),
            Some(_) => Err(type_mismatch()),
        }
    }

    /// Every value of the key as `(name, value)`, in registry order.
    pub fn enum_values(&self) -> WinResult<Vec<(String, RegValue)>> {
        let mut values = Vec::new();
        let (mut name_capacity, mut data_capacity) = self.value_limits()?;
        let mut index = 0_u32;
        let mut retries = 0;
        loop {
            let mut name = vec![0_u16; name_capacity];
            let mut name_len = u32::try_from(name.len()).unwrap_or(u32::MAX);
            let mut data = vec![0_u8; data_capacity];
            let mut data_len = u32::try_from(data.len()).unwrap_or(u32::MAX);
            let mut kind = 0_u32;
            // SAFETY: both buffers are writable for the lengths passed; the out pointers are
            // valid.
            let status = unsafe {
                RegEnumValueW(
                    self.key.0,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    Some(&raw mut kind),
                    Some(data.as_mut_ptr()),
                    Some(&raw mut data_len),
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                return Ok(values);
            }
            if status == ERROR_MORE_DATA && retries < MAX_RETRIES {
                // A value grew while enumerating: re-read the limits and retry this index.
                retries += 1;
                (name_capacity, data_capacity) = self.value_limits()?;
                name_capacity = name_capacity.max(name.len().saturating_mul(2));
                data_capacity = data_capacity.max(data_len as usize);
                continue;
            }
            check_win32(status, "RegEnumValueW")?;
            retries = 0;
            let name = String::from_utf16_lossy(name.get(..name_len as usize).unwrap_or(&[]));
            let data = data.get(..data_len as usize).unwrap_or(&[]);
            values.push((name, RegValue::from_raw(kind, data)));
            index += 1;
        }
    }

    /// Names of the direct subkeys, in registry order.
    pub fn enum_subkeys(&self) -> WinResult<Vec<String>> {
        let mut names = Vec::new();
        let mut index = 0_u32;
        loop {
            let mut name = [0_u16; MAX_KEY_NAME + 1];
            let mut name_len = name.len() as u32;
            // SAFETY: the buffer is writable for the length passed; other outputs are unused.
            let status = unsafe {
                RegEnumKeyExW(
                    self.key.0,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                return Ok(names);
            }
            check_win32(status, "RegEnumKeyExW")?;
            names.push(String::from_utf16_lossy(
                name.get(..name_len as usize).unwrap_or(&[]),
            ));
            index += 1;
        }
    }

    /// Buffer sizes for enumerating values: (name units incl. NUL, data bytes).
    fn value_limits(&self) -> WinResult<(usize, usize)> {
        let mut max_name = 0_u32;
        let mut max_data = 0_u32;
        // SAFETY: only the two requested counters are written.
        let status = unsafe {
            RegQueryInfoKeyW(
                self.key.0,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(&raw mut max_name),
                Some(&raw mut max_data),
                None,
                None,
            )
        };
        check_win32(status, "RegQueryInfoKeyW")?;
        Ok((max_name as usize + 1, max_data as usize))
    }
}

/// A key below an owned path (see [`is_owned_path`]), opened for reading and writing.
///
/// Dereferences to [`RegKey`] for reads.
#[derive(Debug)]
pub struct OwnedRegKey {
    key: RegKey,
}

impl Deref for OwnedRegKey {
    type Target = RegKey;

    fn deref(&self) -> &RegKey {
        &self.key
    }
}

impl OwnedRegKey {
    /// Creates (or opens) `root\path` in `view`. Fails with `AccessDenied` before any registry
    /// call when the path is not owned.
    pub fn create(root: RegRoot, view: RegView, path: &str) -> WinResult<Self> {
        guard(root, path)?;
        let wide = WideString::new(path)?;
        let mut key = HKEY::default();
        // SAFETY: predefined root, NUL-terminated name, valid out pointer; the created key is
        // owned by `KeyHandle`.
        let status = unsafe {
            RegCreateKeyExW(
                root_key(root),
                wide.as_pcwstr(),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                access(KEY_READ.0 | KEY_WRITE.0, view),
                None,
                &mut key,
                None,
            )
        };
        check_win32(status, "RegCreateKeyExW")?;
        Ok(Self {
            key: RegKey {
                key: KeyHandle(key),
                root,
                view,
                path: path.to_owned(),
            },
        })
    }

    /// Opens an existing owned key for reading and writing; `Ok(None)` when it is missing.
    pub fn open(root: RegRoot, view: RegView, path: &str) -> WinResult<Option<Self>> {
        guard(root, path)?;
        Ok(
            open_raw(root_key(root), path, KEY_READ.0 | KEY_WRITE.0, view)?.map(|key| Self {
                key: RegKey {
                    key,
                    root,
                    view,
                    path: path.to_owned(),
                },
            }),
        )
    }

    /// Creates (or opens) a subkey; the combined path must still be owned.
    pub fn create_subkey(&self, name: &str) -> WinResult<Self> {
        Self::create(
            self.key.root,
            self.key.view,
            &format!("{}\\{name}", self.key.path),
        )
    }

    /// Writes `value` under `name` (`""` is the default value).
    pub fn set_value(&self, name: &str, value: &RegValue) -> WinResult<()> {
        let (kind, data) = value
            .to_raw()
            .map_err(|_| WinError::new(E_INVALIDARG, "registry value"))?;
        let wide = WideString::new(name)?;
        // SAFETY: the key is open with KEY_SET_VALUE; name and data outlive the call.
        let status = unsafe {
            RegSetValueExW(
                self.key.key.0,
                wide.as_pcwstr(),
                None,
                REG_VALUE_TYPE(kind),
                Some(&data),
            )
        };
        check_win32(status, "RegSetValueExW")
    }

    /// Writes a `REG_SZ` value.
    pub fn set_string(&self, name: &str, value: &str) -> WinResult<()> {
        self.set_value(name, &RegValue::String(value.to_owned()))
    }

    /// Writes a `REG_EXPAND_SZ` value.
    pub fn set_expand_string(&self, name: &str, value: &str) -> WinResult<()> {
        self.set_value(name, &RegValue::ExpandString(value.to_owned()))
    }

    /// Writes a `REG_DWORD` value.
    pub fn set_dword(&self, name: &str, value: u32) -> WinResult<()> {
        self.set_value(name, &RegValue::Dword(value))
    }

    /// Writes a `REG_QWORD` value.
    pub fn set_qword(&self, name: &str, value: u64) -> WinResult<()> {
        self.set_value(name, &RegValue::Qword(value))
    }

    /// Writes a `REG_MULTI_SZ` value.
    pub fn set_multi_string(&self, name: &str, value: &[&str]) -> WinResult<()> {
        let items = value.iter().map(|item| (*item).to_owned()).collect();
        self.set_value(name, &RegValue::MultiString(items))
    }

    /// Writes a `REG_BINARY` value.
    pub fn set_binary(&self, name: &str, value: &[u8]) -> WinResult<()> {
        self.set_value(name, &RegValue::Binary(value.to_vec()))
    }

    /// Deletes value `name`; returns whether it existed.
    pub fn delete_value(&self, name: &str) -> WinResult<bool> {
        let wide = WideString::new(name)?;
        // SAFETY: the key is open with KEY_SET_VALUE; the name outlives the call.
        let status = unsafe { RegDeleteValueW(self.key.key.0, wide.as_pcwstr()) };
        if is_missing(status) {
            return Ok(false);
        }
        check_win32(status, "RegDeleteValueW")?;
        Ok(true)
    }
}

/// Deletes the owned key `root\path` with all its values and subkeys in `view`; returns
/// whether it existed. Fails with `AccessDenied` before any registry call when the path is not
/// owned.
pub fn delete_tree(root: RegRoot, view: RegView, path: &str) -> WinResult<bool> {
    guard(root, path)?;
    let rights = DELETE | KEY_ENUMERATE_SUB_KEYS.0 | KEY_QUERY_VALUE.0 | KEY_SET_VALUE.0;
    let Some(key) = open_raw(root_key(root), path, rights, view)? else {
        return Ok(false);
    };
    // SAFETY: the key is open with the rights RegDeleteTreeW needs; a null subkey deletes the
    // key's own values and subkeys (in the key's view).
    let status = unsafe { RegDeleteTreeW(key.0, PCWSTR::null()) };
    check_win32(status, "RegDeleteTreeW")?;
    drop(key);
    let wide = WideString::new(path)?;
    // SAFETY: predefined root and NUL-terminated path; the view flag selects the same view.
    let status =
        unsafe { RegDeleteKeyExW(root_key(root), wide.as_pcwstr(), view.sam_flags(), None) };
    if is_missing(status) {
        return Ok(true);
    }
    check_win32(status, "RegDeleteKeyExW")?;
    Ok(true)
}

/// Deletes the owned key `root\path` only when it has neither values nor subkeys; returns
/// whether it was deleted (`false` when it is missing or not empty). Used to remove the product
/// root after its last subkey is gone without ever deleting data.
pub fn delete_empty_key(root: RegRoot, view: RegView, path: &str) -> WinResult<bool> {
    guard(root, path)?;
    let Some(key) = open_raw(root_key(root), path, KEY_QUERY_VALUE.0, view)? else {
        return Ok(false);
    };
    let mut subkeys = 0_u32;
    let mut values = 0_u32;
    // SAFETY: only the two requested counters are written.
    let status = unsafe {
        RegQueryInfoKeyW(
            key.0,
            None,
            None,
            None,
            Some(&raw mut subkeys),
            None,
            None,
            Some(&raw mut values),
            None,
            None,
            None,
            None,
        )
    };
    check_win32(status, "RegQueryInfoKeyW")?;
    drop(key);
    if subkeys != 0 || values != 0 {
        return Ok(false);
    }
    let wide = WideString::new(path)?;
    // SAFETY: predefined root and NUL-terminated path. RegDeleteKeyExW refuses keys that have
    // subkeys, so a subkey created concurrently is never deleted.
    let status =
        unsafe { RegDeleteKeyExW(root_key(root), wide.as_pcwstr(), view.sam_flags(), None) };
    if is_missing(status) {
        return Ok(false);
    }
    if status.0 == win32::ERROR_ACCESS_DENIED {
        // A subkey appeared after the check.
        return Ok(false);
    }
    check_win32(status, "RegDeleteKeyExW")?;
    Ok(true)
}

/// Whether `root\path` exists in `view` (read-only query, any path).
pub fn key_exists(root: RegRoot, view: RegView, path: &str) -> WinResult<bool> {
    Ok(open_raw(root_key(root), path, KEY_QUERY_VALUE.0, view)?.is_some())
}
