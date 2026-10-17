//! File conversion.
//!
//! The source is read completely (bounded by [`MAX_INPUT_BYTES`]) before anything is written.
//! The result goes to a new temporary file in the destination directory, which is flushed to
//! disk and then renamed onto the destination, so the destination is either untouched or holds
//! the complete result. The source file is never written: a destination that resolves to the
//! source is refused, and because the destination is replaced by a rename, even a hard link to
//! the source keeps the source's content.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{self, Component, Path, PathBuf, Prefix};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::charset::Charset;
use crate::codec::{MAX_INPUT_BYTES, convert_bytes};
use crate::error::ConvertError;
use crate::options::ConvertOptions;

/// Attempts to find an unused temporary file name.
const TEMP_ATTEMPTS: u32 = 64;

/// What [`convert_file`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileOutcome {
    /// Bytes read from the source.
    pub bytes_read: u64,
    /// Bytes written to the destination.
    pub bytes_written: u64,
    /// Some characters could not be represented exactly in the target charset.
    pub lossy: bool,
    /// An existing destination file was replaced.
    pub replaced_existing: bool,
}

/// Converts the file `src` from `from` to `to` into `dst`, applying `options`.
///
/// `dst` is read with the platform's path rules first (on Windows, `out.txt.` and `out.txt `
/// name `out.txt`, and `NUL` is a device). Then it refuses:
///
/// - with [`ConvertError::SameFile`] when `dst` names the source, also through `.`/`..`
///   components, symbolic links, a different case or a short name on Windows, a name that
///   Windows reads as the source (`source.txt.`), or a network share of the source's folder (the
///   share is recognized when replacing; without `overwrite` the existing destination is refused
///   first);
/// - with [`ConvertError::DestinationExists`] when `dst` exists and `overwrite` is false;
/// - with [`ConvertError::InvalidPath`] when `src` is not a regular file, `dst` is a directory or
///   has no file name, or, on Windows, `dst` is a device path (`\\.\…`, or `\\?\…` other than
///   the long-path forms `\\?\C:\…` and `\\?\UNC\…`, also when it leads to an ordinary folder) or
///   its file name is not one Windows stores and opens unchanged: a name ending in a dot or space,
///   a stream name (`out.txt:s`), a character Windows forbids in names, or a reserved device name
///   such as `NUL` or `COM1.txt`.
///
/// Without `overwrite`, publishing uses a hard link so a file created at `dst` concurrently is not
/// replaced either (falling back to a checked rename on volumes without hard links).
pub fn convert_file(
    src: &Path,
    dst: &Path,
    from: Charset,
    to: Charset,
    options: ConvertOptions,
    overwrite: bool,
) -> Result<FileOutcome, ConvertError> {
    let requested_name = dst.file_name().ok_or(ConvertError::InvalidPath)?;
    if src == dst {
        return Err(ConvertError::SameFile);
    }
    let source = fs::canonicalize(src)?;
    if !fs::metadata(&source)?.is_file() {
        return Err(ConvertError::InvalidPath);
    }
    let (directory, file_name) = destination_parts(dst)?;
    let destination = directory.join(&file_name);
    // Resolve links and the on-disk spelling (case, short names) so an alias of the source is
    // caught. This comes before the name checks so `source.txt.` reports the more useful error.
    if fs::canonicalize(&destination).is_ok_and(|resolved| resolved == source) {
        return Err(ConvertError::SameFile);
    }
    if file_name != requested_name || (cfg!(windows) && !is_plain_windows_name(&file_name)) {
        return Err(ConvertError::InvalidPath);
    }
    let replaced_existing = match existing(&destination)? {
        None => false,
        Some(metadata) => {
            if metadata.is_dir() || destination.is_dir() {
                return Err(ConvertError::InvalidPath);
            }
            if !overwrite {
                return Err(ConvertError::DestinationExists);
            }
            true
        }
    };

    let input = read_bounded(&source)?;
    let conversion = convert_bytes(&input, from, to, options)?;
    let temp = write_temp(&directory, &conversion.bytes)?;
    if replaced_existing && is_source_through_alias(&source, &directory, &file_name, &temp) {
        // Best effort: the temporary file holds only converted output.
        let _ = fs::remove_file(&temp);
        return Err(ConvertError::SameFile);
    }
    let published = if overwrite {
        fs::rename(&temp, &destination)
    } else {
        publish_new(&temp, &destination)
    };
    if let Err(error) = published {
        // Best effort: the temporary file holds only converted output.
        let _ = fs::remove_file(&temp);
        return Err(if error.kind() == io::ErrorKind::AlreadyExists {
            ConvertError::DestinationExists
        } else {
            error.into()
        });
    }
    Ok(FileOutcome {
        bytes_read: length(input.len()),
        bytes_written: length(conversion.bytes.len()),
        lossy: conversion.lossy,
        replaced_existing,
    })
}

fn length(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

/// The canonical directory of `dst` and the file name it has under the platform's path rules.
///
/// [`path::absolute`] applies those rules without touching the file system: on Windows it drops
/// trailing dots and spaces, turns a device name such as `NUL` into a `\\.\` device path, and
/// turns `//?/` into `\\?\`. The name must be taken from its result because the canonical
/// directory is a `\\?\` path on Windows, and a name joined to one is used exactly as written.
///
/// Device paths are refused before anything at `dst` is opened: `\\.\` paths, and the `\\?\`
/// paths that are not the long-path form of a drive path (`\\?\C:\`) or UNC path (`\\?\UNC\`),
/// such as `\\?\GLOBALROOT\Device\Null` or `\\?\Volume{…}\`. They name objects of the Windows
/// object namespace, which need not be folders or files. A folder reached that way is refused as
/// well: its drive or UNC path names it plainly.
fn destination_parts(dst: &Path) -> Result<(PathBuf, OsString), ConvertError> {
    let normalized = path::absolute(dst)?;
    if let Some(Component::Prefix(prefix)) = normalized.components().next()
        && matches!(prefix.kind(), Prefix::DeviceNS(_) | Prefix::Verbatim(_))
    {
        return Err(ConvertError::InvalidPath);
    }
    let file_name = normalized.file_name().ok_or(ConvertError::InvalidPath)?;
    let parent = normalized.parent().ok_or(ConvertError::InvalidPath)?;
    Ok((fs::canonicalize(parent)?, file_name.to_owned()))
}

/// True when Windows stores and opens a file under exactly `name`.
///
/// Windows drops a trailing dot or space; forbids `<>:"/\|?*` (`:` also starts a stream name)
/// and control characters; and reads the reserved device names (`CON`, `PRN`, `AUX`, `NUL`,
/// `COM0`-`COM9`, `LPT0`-`LPT9`, `COM¹`-`COM³`, `LPT¹`-`LPT³`, `CONIN$`, `CONOUT$`) as devices
/// in any case, and before Windows 11 also with any extension (`nul.txt`). Every such form is
/// refused: a file created under one through a `\\?\` path cannot be opened or deleted through
/// normal paths, and the file would not be portable to older Windows versions.
fn is_plain_windows_name(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    let forbidden =
        |c: char| c < ' ' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*');
    if name.is_empty() || name.ends_with(['.', ' ']) || name.contains(forbidden) {
        return false;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ');
    !is_reserved_device_name(stem)
}

fn is_reserved_device_name(stem: &str) -> bool {
    const NAMES: [&str; 6] = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
    if NAMES
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
    {
        return true;
    }
    let (Some(port), Some(number)) = (stem.get(..3), stem.get(3..)) else {
        return false;
    };
    let mut number = number.chars();
    let (Some(digit), None) = (number.next(), number.next()) else {
        return false;
    };
    (port.eq_ignore_ascii_case("COM") || port.eq_ignore_ascii_case("LPT"))
        && (digit.is_ascii_digit() || matches!(digit, '\u{B9}' | '\u{B2}' | '\u{B3}'))
}

/// True when `directory` is another name of the source's directory (a network share of it, for
/// example) and `file_name` there is the source. Canonicalization does not unify such names; the
/// temporary file just written to `directory` shows up in the source's directory exactly when
/// both are the same directory.
fn is_source_through_alias(
    source: &Path,
    directory: &Path,
    file_name: &OsStr,
    temp: &Path,
) -> bool {
    let (Some(source_directory), Some(temp_name)) = (source.parent(), temp.file_name()) else {
        return false;
    };
    source_directory != directory
        && fs::symlink_metadata(source_directory.join(temp_name)).is_ok()
        && fs::canonicalize(source_directory.join(file_name))
            .is_ok_and(|resolved| resolved == source)
}

/// Metadata of `path` itself (not of a link target), or `None` when nothing is there.
fn existing(path: &Path) -> Result<Option<Metadata>, ConvertError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Reads a whole file of at most [`MAX_INPUT_BYTES`] bytes.
fn read_bounded(path: &Path) -> Result<Vec<u8>, ConvertError> {
    let limit = length(MAX_INPUT_BYTES);
    let file = File::open(path)?;
    if file.metadata()?.len() > limit {
        return Err(ConvertError::InputTooLarge);
    }
    let mut bytes = Vec::new();
    // The file may grow after the size check; never read more than one byte past the limit.
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(ConvertError::InputTooLarge);
    }
    Ok(bytes)
}

/// Writes `bytes` to a new temporary file in `directory` and flushes it to disk.
fn write_temp(directory: &Path, bytes: &[u8]) -> Result<PathBuf, ConvertError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    for attempt in 0..TEMP_ATTEMPTS {
        let path = directory.join(format!(
            ".raumakey-convert-{}-{stamp:x}-{attempt}.tmp",
            std::process::id()
        ));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        let written = file.write_all(bytes).and_then(|()| file.sync_all());
        drop(file);
        return match written {
            Ok(()) => Ok(path),
            Err(error) => {
                let _ = fs::remove_file(&path);
                Err(error.into())
            }
        };
    }
    Err(ConvertError::Io(io::ErrorKind::AlreadyExists))
}

/// Moves `temp` to `destination` without replacing anything that appeared there meanwhile.
fn publish_new(temp: &Path, destination: &Path) -> io::Result<()> {
    publish_new_with(temp, destination, |original, link| {
        fs::hard_link(original, link)
    })
}

/// [`publish_new`] with the hard-link operation passed in, so tests can stand in for a volume
/// without hard links.
fn publish_new_with(
    temp: &Path,
    destination: &Path,
    hard_link: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    match hard_link(temp, destination) {
        Ok(()) => {
            // The destination now names the converted file; the extra name is only clutter.
            let _ = fs::remove_file(temp);
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(error),
        Err(_) => {
            // No hard links on this volume (for example FAT): check, then rename.
            if fs::symlink_metadata(destination).is_ok() {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            fs::rename(temp, destination)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn plain_windows_names() {
        for name in [
            "out.txt",
            "Tiếng Việt.txt",
            ".hidden",
            "a.b.c",
            " leading space",
            "nul_file.txt",
            "console.txt",
            "COM10",
            "LPT",
            "com1x.txt",
            "auxiliary",
            "a\u{7F}b",
        ] {
            assert!(is_plain_windows_name(OsStr::new(name)), "{name:?}");
        }
    }

    #[test]
    fn names_windows_changes_forbids_or_reads_as_devices() {
        for name in [
            "",
            "out.txt.",
            "out.txt ",
            "out. .",
            "out.txt:stream",
            "a?b",
            "a*b",
            "a<b",
            "a>b",
            "a\"b",
            "a|b",
            r"a\b",
            "a/b",
            "a\u{0}b",
            "a\u{1F}b",
            "NUL",
            "nul.txt",
            "Nul.tar.gz",
            "nul .txt",
            "con",
            "PRN.log",
            "aux",
            "COM0",
            "com9.txt",
            "lpt1",
            "LPT9.dat",
            "COM\u{B9}",
            "com\u{B2}.txt",
            "LPT\u{B3}",
            "CONIN$",
            "conout$.txt",
        ] {
            assert!(!is_plain_windows_name(OsStr::new(name)), "{name:?}");
        }
    }

    #[test]
    fn another_spelling_of_the_source_folder_is_recognized_by_the_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("source.txt"), "source").unwrap();
        fs::hard_link(dir.path().join("source.txt"), dir.path().join("link.txt")).unwrap();
        let source = fs::canonicalize(dir.path().join("source.txt")).unwrap();
        // The source's folder under a spelling canonicalization would not produce.
        let alias = dir.path().join("sub").join("..");
        assert_ne!(Some(alias.as_path()), source.parent());
        let temp = write_temp(&alias, b"converted").unwrap();
        assert!(is_source_through_alias(
            &source,
            &alias,
            OsStr::new("source.txt"),
            &temp
        ));
        // Other names in that folder, including another link to the source, are not the source.
        for other in ["link.txt", "missing.txt"] {
            assert!(
                !is_source_through_alias(&source, &alias, OsStr::new(other), &temp),
                "{other}"
            );
        }

        // A different folder holding a different file of the same name.
        let sub = dir.path().join("sub");
        fs::write(sub.join("source.txt"), "other").unwrap();
        let temp_in_sub = write_temp(&sub, b"converted").unwrap();
        assert!(!is_source_through_alias(
            &source,
            &sub,
            OsStr::new("source.txt"),
            &temp_in_sub
        ));
        // The canonical folder itself is compared directly, not through the temporary file.
        let canonical = source.parent().unwrap();
        let temp_in_canonical = write_temp(canonical, b"converted").unwrap();
        assert!(!is_source_through_alias(
            &source,
            canonical,
            OsStr::new("source.txt"),
            &temp_in_canonical
        ));
    }

    #[test]
    fn publishing_a_new_file_links_it_and_removes_the_temporary_name() {
        let dir = tempfile::tempdir().unwrap();
        let temp = write_temp(dir.path(), b"converted").unwrap();
        let destination = dir.path().join("out.txt");
        publish_new(&temp, &destination).unwrap();
        assert_eq!(read(&destination), "converted");
        assert!(fs::symlink_metadata(&temp).is_err());
    }

    #[test]
    fn publishing_a_new_file_never_replaces_one_that_appeared() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("out.txt");
        fs::write(&destination, "appeared meanwhile").unwrap();
        let temp = write_temp(dir.path(), b"converted").unwrap();
        let error = publish_new(&temp, &destination).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(read(&destination), "appeared meanwhile");
        // The caller removes the temporary file.
        assert_eq!(read(&temp), "converted");
    }

    #[test]
    fn without_hard_links_publishing_falls_back_to_a_checked_rename() {
        let no_hard_links = |_: &Path, _: &Path| Err(io::Error::from(io::ErrorKind::Unsupported));
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("out.txt");

        let temp = write_temp(dir.path(), b"converted").unwrap();
        publish_new_with(&temp, &destination, no_hard_links).unwrap();
        assert_eq!(read(&destination), "converted");
        assert!(fs::symlink_metadata(&temp).is_err());

        let temp = write_temp(dir.path(), b"second").unwrap();
        let error = publish_new_with(&temp, &destination, no_hard_links).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(read(&destination), "converted");
        assert_eq!(read(&temp), "second");
    }
}
