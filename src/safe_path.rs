//! Confines user-influenced relative paths to a storage root.
//!
//! A path is accepted only if it consists of plain components (no `..`, no
//! root/drive prefixes, no backslashes or NULs, including after repeated
//! percent-decoding) and its canonical form still lies inside the canonical
//! root, which also rules out symlinks that point outside of it.

use std::path::{Component, Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum PathError {
    /// The path is malformed or tries to leave the root.
    Rejected,
    /// The path is acceptable but nothing exists there.
    NotFound,
}

const MAX_DECODE_ROUNDS: usize = 4;

fn hex_val(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_plain(path: &str) -> bool {
    if path.is_empty() || path.contains('\0') || path.contains('\\') {
        return false;
    }
    Path::new(path)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

/// True if `rel` (and every percent-decoded form of it) is a plain relative path.
pub fn is_safe_relative(rel: &str) -> bool {
    let mut current = rel.to_string();
    for _ in 0..=MAX_DECODE_ROUNDS {
        if !is_plain(&current) {
            return false;
        }
        let decoded = percent_decode(&current);
        if decoded == current {
            return true;
        }
        current = decoded;
    }
    false
}

/// Resolves `rel` under `root`, returning the canonical path of an existing
/// file or directory that is guaranteed to lie inside `root`.
pub fn resolve_under(root: impl AsRef<Path>, rel: &str) -> Result<PathBuf, PathError> {
    if !is_safe_relative(rel) {
        return Err(PathError::Rejected);
    }
    let root = root
        .as_ref()
        .canonicalize()
        .map_err(|_| PathError::NotFound)?;
    let full = match root.join(rel).canonicalize() {
        Ok(p) => p,
        Err(_) => return Err(PathError::NotFound),
    };
    if full.starts_with(&root) {
        Ok(full)
    } else {
        Err(PathError::Rejected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("secret.txt"), "outside").unwrap();
        let root = dir.path().join("root");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("ok.txt"), "inside").unwrap();
        fs::write(root.join("sub/nested.txt"), "nested").unwrap();
        (dir, root)
    }

    #[test]
    fn rejects_traversal_payloads() {
        let (_guard, root) = fixture();
        let abs = _guard.path().join("secret.txt");
        let payloads = [
            "../secret.txt",
            "../../secret.txt",
            "sub/../../secret.txt",
            "..\\secret.txt",
            "sub\\..\\..\\secret.txt",
            "..%2fsecret.txt",
            "..%2Fsecret.txt",
            "%2e%2e/secret.txt",
            "%2e%2e%2fsecret.txt",
            "%2E%2E%2Fsecret.txt",
            "%252e%252e%252fsecret.txt",
            "%25252e%25252e%25252fsecret.txt",
            "..%5csecret.txt",
            "/etc/passwd",
            "%2fetc%2fpasswd",
            "//etc/passwd",
            "ok.txt%00.png",
            "ok.txt\0",
            "",
            "..",
            ".",
            "sub/..",
        ];
        for p in payloads {
            assert!(resolve_under(&root, p).is_err(), "accepted {p:?}");
            assert!(!is_safe_relative(p), "is_safe_relative accepted {p:?}");
        }
        assert!(resolve_under(&root, abs.to_str().unwrap()).is_err());
    }

    #[test]
    fn accepts_plain_relative_paths() {
        let (_guard, root) = fixture();
        let canonical_root = root.canonicalize().unwrap();
        assert_eq!(
            resolve_under(&root, "ok.txt").unwrap(),
            canonical_root.join("ok.txt")
        );
        assert_eq!(
            resolve_under(&root, "sub/nested.txt").unwrap(),
            canonical_root.join("sub/nested.txt")
        );
        assert_eq!(
            resolve_under(&root, "missing.txt"),
            Err(PathError::NotFound)
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escaping_root() {
        let (_guard, root) = fixture();
        std::os::unix::fs::symlink(_guard.path().join("secret.txt"), root.join("link.txt"))
            .unwrap();
        std::os::unix::fs::symlink(_guard.path(), root.join("linkdir")).unwrap();
        assert_eq!(resolve_under(&root, "link.txt"), Err(PathError::Rejected));
        assert_eq!(
            resolve_under(&root, "linkdir/secret.txt"),
            Err(PathError::Rejected)
        );
    }
}
