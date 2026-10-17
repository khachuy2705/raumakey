//! File conversion safety: never the source, never an existing file unless asked, and always a
//! temporary file in the destination directory followed by a rename.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::cp1252_bytes;
use raumakey_converter::{
    CaseTransform, Charset, ConvertError, ConvertOptions, FileOutcome, MAX_INPUT_BYTES,
    convert_file,
};

const TCVN3_TEXT: &str = "TiÕng ViÖt";

fn setup() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("source.txt");
    fs::write(&src, cp1252_bytes(TCVN3_TEXT)).unwrap();
    (dir, src)
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn tcvn3_to_unicode(src: &Path, dst: &Path, overwrite: bool) -> Result<FileOutcome, ConvertError> {
    convert_file(
        src,
        dst,
        Charset::Tcvn3,
        Charset::Unicode,
        ConvertOptions::default(),
        overwrite,
    )
}

#[test]
fn converts_a_file_into_a_new_destination() {
    let (dir, src) = setup();
    let dst = dir.path().join("out.txt");
    let outcome = tcvn3_to_unicode(&src, &dst, false).unwrap();
    assert_eq!(fs::read_to_string(&dst).unwrap(), "Tiếng Việt");
    assert_eq!(
        outcome,
        FileOutcome {
            bytes_read: 10,
            bytes_written: 14,
            lossy: false,
            replaced_existing: false,
        }
    );
    assert_eq!(fs::read(&src).unwrap(), cp1252_bytes(TCVN3_TEXT));
    assert_eq!(entries(dir.path()), ["out.txt", "source.txt"]);
}

#[test]
fn options_apply_and_loss_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("unicode.txt");
    fs::write(&src, "Tiếng Việt").unwrap();
    let dst = dir.path().join("tcvn3.txt");
    let options = ConvertOptions {
        case: CaseTransform::Upper,
        remove_tones: false,
    };
    let outcome =
        convert_file(&src, &dst, Charset::Unicode, Charset::Tcvn3, options, false).unwrap();
    assert!(outcome.lossy);
    assert_eq!(fs::read(&dst).unwrap(), cp1252_bytes("TIÕNG VIÖT"));
}

#[test]
fn the_source_is_never_the_destination() {
    let (dir, src) = setup();
    let aliases = [
        src.clone(),
        dir.path().join(".").join("source.txt"),
        dir.path().join("sub").join("..").join("source.txt"),
    ];
    fs::create_dir(dir.path().join("sub")).unwrap();
    for dst in aliases {
        for overwrite in [false, true] {
            assert_eq!(
                tcvn3_to_unicode(&src, &dst, overwrite),
                Err(ConvertError::SameFile),
                "{dst:?}"
            );
        }
    }
    assert_eq!(fs::read(&src).unwrap(), cp1252_bytes(TCVN3_TEXT));
    assert_eq!(entries(dir.path()), ["source.txt", "sub"]);
}

#[cfg(windows)]
#[test]
fn a_differently_cased_name_of_the_source_is_the_same_file_on_windows() {
    let (dir, src) = setup();
    let dst = dir.path().join("SOURCE.TXT");
    assert_eq!(
        tcvn3_to_unicode(&src, &dst, true),
        Err(ConvertError::SameFile)
    );
    assert_eq!(fs::read(&src).unwrap(), cp1252_bytes(TCVN3_TEXT));
}

/// Windows drops trailing dots and spaces from a file name, so these spellings open the source;
/// `::$DATA` names its unnamed data stream.
#[cfg(windows)]
#[test]
fn spellings_windows_reads_as_the_source_are_the_same_file() {
    let (dir, src) = setup();
    for name in [
        "source.txt.",
        "source.txt ",
        "source.txt. . .",
        "Source.TXT.",
        "source.txt::$DATA",
    ] {
        let dst = dir.path().join(name);
        for overwrite in [false, true] {
            assert_eq!(
                tcvn3_to_unicode(&src, &dst, overwrite),
                Err(ConvertError::SameFile),
                "{name:?} overwrite={overwrite}"
            );
        }
    }
    assert_eq!(fs::read(&src).unwrap(), cp1252_bytes(TCVN3_TEXT));
    assert_eq!(entries(dir.path()), ["source.txt"]);
}

/// Names that would not name the file written (Windows drops trailing dots and spaces), that
/// Windows reads as a device, or that it does not allow, are refused before anything is created.
#[cfg(windows)]
#[test]
fn names_windows_would_change_or_read_as_a_device_are_refused() {
    let (dir, src) = setup();
    fs::write(dir.path().join("out.txt"), "keep me").unwrap();
    let names = [
        "out.txt.",
        "out.txt ",
        "new. .",
        "NUL",
        "nul",
        "nul.txt",
        "NUL.tar.gz",
        "nul .txt",
        "CON",
        "prn.log",
        "Aux",
        "COM1",
        "com9.txt",
        "LPT1",
        "lpt9.dat",
        "COM0",
        "LPT0",
        "COM\u{B9}",
        "lpt\u{B3}.txt",
        "CONIN$",
        "conout$.txt",
        "out.txt:stream",
        "new.txt:stream:$DATA",
        "a?b.txt",
        "a*b.txt",
        "a<b.txt",
        "a>b.txt",
        "a\"b.txt",
        "a|b.txt",
        "a\u{1}b.txt",
    ];
    for name in names {
        let dst = dir.path().join(name);
        for overwrite in [false, true] {
            assert_eq!(
                tcvn3_to_unicode(&src, &dst, overwrite),
                Err(ConvertError::InvalidPath),
                "{name:?} overwrite={overwrite}"
            );
        }
    }
    assert_eq!(fs::read(&src).unwrap(), cp1252_bytes(TCVN3_TEXT));
    assert_eq!(
        fs::read_to_string(dir.path().join("out.txt")).unwrap(),
        "keep me"
    );
    assert_eq!(entries(dir.path()), ["out.txt", "source.txt"]);
}

/// Device paths are refused before the destination is opened: `\\.\` paths, and `\\?\` paths
/// that are not drive or UNC paths. `GLOBALROOT` is the root of the Windows object namespace, so
/// `\\?\GLOBALROOT\Device\Null` is the null device.
#[cfg(windows)]
#[test]
fn device_paths_are_refused() {
    let (dir, src) = setup();
    for dst in [
        r"\\.\NUL",
        r"\\.\GLOBALROOT\Device\Null",
        r"\\.\pipe\raumakey-converter-test",
        r"\\?\NUL",
        r"\\?\GLOBALROOT\Device\Null",
        "//?/GLOBALROOT/Device/Null",
        r"\\?\Volume{00000000-0000-0000-0000-000000000000}\out.txt",
    ] {
        for overwrite in [false, true] {
            assert_eq!(
                tcvn3_to_unicode(&src, Path::new(dst), overwrite),
                Err(ConvertError::InvalidPath),
                "{dst:?} overwrite={overwrite}"
            );
        }
    }
    assert_eq!(fs::read(&src).unwrap(), cp1252_bytes(TCVN3_TEXT));
    assert_eq!(entries(dir.path()), ["source.txt"]);
}

/// `folder\out.txt` spelled through the device namespace, if `folder` is a drive path: `\\.\` and
/// its forward-slash form, and the object namespace root followed by the drive letter.
#[cfg(windows)]
fn through_the_device_namespace(folder: &Path) -> Vec<PathBuf> {
    let Some(text) = folder.to_str() else {
        return Vec::new();
    };
    if !matches!(text.as_bytes(), [drive, b':', b'\\', ..] if drive.is_ascii_alphabetic()) {
        return Vec::new();
    }
    [
        format!(r"\\.\{text}\out.txt"),
        format!("//./{}/out.txt", text.replace('\\', "/")),
        format!(r"\\?\GLOBALROOT\??\{text}\out.txt"),
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

/// A device path is refused even when it leads to an ordinary folder; the folder's drive path is
/// the one to use.
#[cfg(windows)]
#[test]
fn a_folder_reached_through_the_device_namespace_is_refused() {
    let (dir, src) = setup();
    let aliases = through_the_device_namespace(dir.path());
    if aliases.is_empty() {
        eprintln!("skipped: the temporary folder is not on a drive");
        return;
    }
    for dst in aliases {
        for overwrite in [false, true] {
            assert_eq!(
                tcvn3_to_unicode(&src, &dst, overwrite),
                Err(ConvertError::InvalidPath),
                "{dst:?} overwrite={overwrite}"
            );
        }
    }
    assert_eq!(fs::read(&src).unwrap(), cp1252_bytes(TCVN3_TEXT));
    assert_eq!(entries(dir.path()), ["source.txt"]);
}

/// `\\localhost\<drive>$\...`, the administrative share of the source's drive, if this machine
/// serves it to the current user.
#[cfg(windows)]
fn through_the_administrative_share(path: &Path) -> Option<PathBuf> {
    let text = path.to_str()?;
    let (drive, rest) = text.split_once(":\\")?;
    if drive.len() != 1 {
        return None;
    }
    let alias = PathBuf::from(format!(r"\\localhost\{drive}$\{rest}"));
    fs::metadata(&alias).is_ok().then_some(alias)
}

/// A network path that reaches the source's folder spells it differently even after
/// canonicalization; replacing the destination must still not replace the source.
#[cfg(windows)]
#[test]
fn the_source_reached_through_a_network_share_is_the_same_file() {
    let (dir, src) = setup();
    let Some(alias) = through_the_administrative_share(&src) else {
        eprintln!("skipped: the administrative share of the temporary folder is not reachable");
        return;
    };
    assert_eq!(
        tcvn3_to_unicode(&src, &alias, false),
        Err(ConvertError::DestinationExists)
    );
    assert_eq!(
        tcvn3_to_unicode(&src, &alias, true),
        Err(ConvertError::SameFile)
    );
    assert_eq!(fs::read(&src).unwrap(), cp1252_bytes(TCVN3_TEXT));
    assert_eq!(entries(dir.path()), ["source.txt"]);

    // Other names in that folder are ordinary destinations.
    let out = alias.with_file_name("out.txt");
    tcvn3_to_unicode(&src, &out, false).unwrap();
    tcvn3_to_unicode(&src, &out, true).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("out.txt")).unwrap(),
        "Tiếng Việt"
    );
    assert_eq!(fs::read(&src).unwrap(), cp1252_bytes(TCVN3_TEXT));
    assert_eq!(entries(dir.path()), ["out.txt", "source.txt"]);
}

/// `\\?\C:\...` and `\\?\UNC\server\share\...` are the long-path forms of drive and UNC paths,
/// not device paths: they are ordinary destinations, and the source is still recognized through
/// them. The UNC form is checked when the administrative share is reachable.
#[cfg(windows)]
#[test]
fn long_path_forms_of_drive_and_unc_paths_are_ordinary_destinations() {
    let (dir, src) = setup();
    let folder = dir.path().to_str().unwrap();
    let mut folders = vec![format!(r"\\?\{folder}")];
    match through_the_administrative_share(dir.path()) {
        Some(share) => {
            let share = share.to_str().unwrap();
            folders.push(format!(r"\\?\UNC\{}", share.trim_start_matches('\\')));
        }
        None => eprintln!("skipped UNC: the administrative share is not reachable"),
    }
    for folder in folders {
        let folder = Path::new(&folder);
        let out = folder.join("out.txt");
        let outcome = tcvn3_to_unicode(&src, &out, false).unwrap();
        assert!(!outcome.replaced_existing, "{out:?}");
        assert!(
            tcvn3_to_unicode(&src, &out, true)
                .unwrap()
                .replaced_existing
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("out.txt")).unwrap(),
            "Tiếng Việt"
        );
        fs::remove_file(dir.path().join("out.txt")).unwrap();
        assert_eq!(
            tcvn3_to_unicode(&src, &folder.join("source.txt"), true),
            Err(ConvertError::SameFile),
            "{folder:?}"
        );
    }
    assert_eq!(fs::read(&src).unwrap(), cp1252_bytes(TCVN3_TEXT));
    assert_eq!(entries(dir.path()), ["source.txt"]);
}

/// A short (8.3) name of the source is the same file. Skipped on volumes that do not create
/// short names.
#[cfg(windows)]
#[test]
fn a_short_name_of_the_source_is_the_same_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("a long source name.txt");
    fs::write(&src, cp1252_bytes(TCVN3_TEXT)).unwrap();
    let short = dir.path().join("ALONGS~1.TXT");
    if fs::metadata(&short).is_err() {
        eprintln!("skipped: the temporary volume does not create short names");
        return;
    }
    for overwrite in [false, true] {
        assert_eq!(
            tcvn3_to_unicode(&src, &short, overwrite),
            Err(ConvertError::SameFile)
        );
    }
    assert_eq!(fs::read(&src).unwrap(), cp1252_bytes(TCVN3_TEXT));
}

/// When the destination cannot be replaced, the error is reported, the destination keeps its
/// content, and the temporary file is removed.
#[cfg(windows)]
#[test]
fn a_failed_replacement_leaves_the_destination_and_no_temporary_file() {
    /// Sets or clears the read-only attribute (Windows has no other meaning for it).
    fn set_read_only(path: &Path, read_only: bool) {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(read_only);
        fs::set_permissions(path, permissions).unwrap();
    }

    let (dir, src) = setup();
    let dst = dir.path().join("out.txt");
    fs::write(&dst, "keep me").unwrap();
    set_read_only(&dst, true);
    let result = tcvn3_to_unicode(&src, &dst, true);
    let remaining = entries(dir.path());
    // Clear the attribute before asserting so the temporary folder can always be removed.
    set_read_only(&dst, false);

    assert_eq!(
        result,
        Err(ConvertError::Io(std::io::ErrorKind::PermissionDenied))
    );
    assert_eq!(fs::read_to_string(&dst).unwrap(), "keep me");
    assert_eq!(remaining, ["out.txt", "source.txt"]);
}

#[test]
fn an_existing_destination_is_kept_unless_overwrite_is_requested() {
    let (dir, src) = setup();
    let dst = dir.path().join("out.txt");
    fs::write(&dst, "keep me").unwrap();
    assert_eq!(
        tcvn3_to_unicode(&src, &dst, false),
        Err(ConvertError::DestinationExists)
    );
    assert_eq!(fs::read_to_string(&dst).unwrap(), "keep me");
    assert_eq!(entries(dir.path()), ["out.txt", "source.txt"]);

    let outcome = tcvn3_to_unicode(&src, &dst, true).unwrap();
    assert!(outcome.replaced_existing);
    assert_eq!(fs::read_to_string(&dst).unwrap(), "Tiếng Việt");
    assert_eq!(entries(dir.path()), ["out.txt", "source.txt"]);
}

#[test]
fn overwriting_a_hard_link_to_the_source_leaves_the_source_intact() {
    let (dir, src) = setup();
    let dst = dir.path().join("link.txt");
    fs::hard_link(&src, &dst).unwrap();
    tcvn3_to_unicode(&src, &dst, true).unwrap();
    assert_eq!(fs::read_to_string(&dst).unwrap(), "Tiếng Việt");
    assert_eq!(fs::read(&src).unwrap(), cp1252_bytes(TCVN3_TEXT));
}

#[test]
fn directories_are_not_files() {
    let (dir, src) = setup();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    for overwrite in [false, true] {
        assert_eq!(
            tcvn3_to_unicode(&src, &sub, overwrite),
            Err(ConvertError::InvalidPath)
        );
    }
    assert_eq!(
        tcvn3_to_unicode(&sub, &dir.path().join("out.txt"), false),
        Err(ConvertError::InvalidPath)
    );
    assert_eq!(
        tcvn3_to_unicode(&src, &dir.path().join(".."), true),
        Err(ConvertError::InvalidPath)
    );
    assert_eq!(entries(dir.path()), ["source.txt", "sub"]);
}

#[test]
fn missing_paths_are_io_errors() {
    let (dir, src) = setup();
    assert_eq!(
        tcvn3_to_unicode(
            &dir.path().join("missing.txt"),
            &dir.path().join("out.txt"),
            false
        ),
        Err(ConvertError::Io(std::io::ErrorKind::NotFound))
    );
    assert_eq!(
        tcvn3_to_unicode(&src, &dir.path().join("no-such-dir").join("out.txt"), false),
        Err(ConvertError::Io(std::io::ErrorKind::NotFound))
    );
    assert_eq!(entries(dir.path()), ["source.txt"]);
}

#[test]
fn malformed_or_oversized_sources_create_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.txt");
    fs::write(&bad, [0x56, 0xC3]).unwrap();
    let dst = dir.path().join("out.txt");
    assert_eq!(
        convert_file(
            &bad,
            &dst,
            Charset::Unicode,
            Charset::Tcvn3,
            ConvertOptions::default(),
            false
        ),
        Err(ConvertError::MalformedInput)
    );

    let big = dir.path().join("big.txt");
    let file = fs::File::create(&big).unwrap();
    file.set_len(u64::try_from(MAX_INPUT_BYTES).unwrap() + 1)
        .unwrap();
    drop(file);
    assert_eq!(
        tcvn3_to_unicode(&big, &dst, false),
        Err(ConvertError::InputTooLarge)
    );
    assert_eq!(entries(dir.path()), ["bad.txt", "big.txt"]);
}
