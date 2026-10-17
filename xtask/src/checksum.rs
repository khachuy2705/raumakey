//! SHA-256 helpers and the `SHA256SUMS` format.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

/// Lower-case hexadecimal SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Lower-case hexadecimal SHA-256 of the file at `path`, read in chunks.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Formats `(relative path, hex digest)` pairs as `SHA256SUMS` lines (`<hex>  <path>`), sorted by
/// path, with `/` separators and LF line endings, as accepted by `sha256sum -c`.
pub fn format_sha256sums(entries: &[(String, String)]) -> String {
    let mut lines: Vec<(String, &str)> = entries
        .iter()
        .map(|(path, digest)| (path.replace('\\', "/"), digest.as_str()))
        .collect();
    lines.sort();
    lines
        .iter()
        .map(|(path, digest)| format!("{digest}  {path}\n"))
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_digests() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn file_digest_matches_memory_digest() {
        let dir =
            std::env::temp_dir().join(format!("raumakey-xtask-checksum-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("data.bin");
        let data: Vec<u8> = (0..200_000u32).map(|value| (value % 251) as u8).collect();
        std::fs::write(&path, &data).expect("write");
        let digest = sha256_file(&path).expect("hash file");
        std::fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!(digest, sha256_hex(&data));
        assert!(sha256_file(&dir.join("missing")).is_err());
    }

    #[test]
    fn sums_are_sorted_with_two_spaces_and_forward_slashes() {
        let entries = vec![
            ("x64\\raumakey_tip.dll".to_owned(), "bb".repeat(32)),
            ("LICENSE".to_owned(), "aa".repeat(32)),
            ("raumakey.exe".to_owned(), "cc".repeat(32)),
        ];
        assert_eq!(
            format_sha256sums(&entries),
            format!(
                "{}  LICENSE\n{}  raumakey.exe\n{}  x64/raumakey_tip.dll\n",
                "aa".repeat(32),
                "cc".repeat(32),
                "bb".repeat(32)
            )
        );
        assert_eq!(format_sha256sums(&[]), "");
    }
}
