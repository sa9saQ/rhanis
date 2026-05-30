//! Path validation for file-touching tools (koe-1vi).
//!
//! Adapted from Enitar's `validation.rs`. Confines file access to an explicit
//! allow-list of base directories (M1: Documents / Desktop, supplied by the
//! caller) and rejects path traversal, symlink escapes, and access to sensitive
//! directories (`.ssh`, `.git`, system dirs, …).
//!
//! # Why it lives here (and is not yet called)
//! The approval gate gates DANGER file operations (`delete_file`, …); the actual
//! path-safety check happens when those operations *execute* — in the
//! tool_dispatcher (koe-2gy) and the file tools (`read_file` / `write_file`,
//! koe-s7i). This module is the stable, fully-tested primitive those PRs import,
//! landed here per the issue scope. Functions with no in-crate caller yet carry
//! `#[allow(dead_code)]` naming the consumer — the same interface-first
//! convention as `secret_store::SecretStore::get_api_key` and the
//! `storage::RecorderAdapter` methods, NOT skeleton.
//!
//! # TOCTOU caveat for consumers (koe-2gy / koe-s7i) — MUST read
//! These functions validate the path **at call time** and return a `PathBuf`.
//! Between validation and the eventual open, a component could be swapped for a
//! symlink that escapes the allow-list (time-of-check vs time-of-use). The
//! returned path is therefore necessary but not sufficient: the consumer MUST
//! open it without following symlinks (Unix: `O_NOFOLLOW` via `OpenOptions`
//! `custom_flags`; Windows: do not follow reparse points) and operate on the
//! resulting handle — never re-resolve the path by name. Tracked for the file
//! tools (koe-s7i) and the dispatcher (koe-2gy), which own the actual I/O.
//!
//! transaction N/A · idempotency_key N/A (read-only path validation, not billing).

use std::path::{Path, PathBuf};

/// Maximum accepted input path length (defense against pathological inputs).
const MAX_PATH_LENGTH: usize = 4096;

/// Directory/file names that must never appear anywhere in a validated path.
/// These hold credentials, VCS metadata, or OS internals — off-limits even
/// inside an otherwise-allowed base dir (e.g. `~/Documents/.git/config`).
#[cfg(not(windows))]
const SENSITIVE_COMPONENTS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".config",
    ".local", // .local/share/keyrings, …
    ".git",
    ".svn",
    ".hg",
    "node_modules",
];

#[cfg(windows)]
const SENSITIVE_COMPONENTS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".git",
    ".local", // parity with the non-Windows list (e.g. a synced .local dir)
    "node_modules",
    // Canonical Windows paths carry a drive-letter prefix, so system roots are
    // matched as components rather than via a leading-path prefix.
    "appdata",
    "windows",
    "system32",
    "syswow64",
    "programdata",
];

/// Why a path was rejected. `Display` returns a **fixed** message per variant —
/// it never echoes the offending path — so a rejection reason can be surfaced in
/// a redacted approval summary or returned over IPC without leaking the on-disk
/// layout (same redaction posture as `secret_store::SecretError`).
#[derive(Debug, PartialEq, Eq)]
pub enum PathValidationError {
    /// Input exceeds [`MAX_PATH_LENGTH`].
    TooLong,
    /// Input was empty or whitespace-only.
    Empty,
    /// The path (or, for writes, its parent directory) does not resolve on disk.
    Unresolvable,
    /// The resolved path is outside every allowed base directory.
    OutsideAllowed,
    /// The path contains a blocked sensitive component (`.ssh`, `.git`, …).
    Sensitive,
    /// A read target was expected to be an existing regular file but was not
    /// (missing, or a directory).
    NotAFile,
}

impl std::fmt::Display for PathValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            PathValidationError::TooLong => "path is too long",
            PathValidationError::Empty => "path must not be empty",
            PathValidationError::Unresolvable => "path could not be resolved",
            PathValidationError::OutsideAllowed => "path is outside the allowed directories",
            PathValidationError::Sensitive => "path is in a protected location",
            PathValidationError::NotAFile => "path is not a file",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for PathValidationError {}

/// True if any component of `path` matches a [`SENSITIVE_COMPONENTS`] entry
/// (case-insensitive — Windows and macOS file systems are case-insensitive, and
/// a case-only bypass like `.SSH` must not slip through).
fn contains_sensitive(path: &Path) -> bool {
    path.components().any(|c| {
        let name = c.as_os_str().to_string_lossy().to_ascii_lowercase();
        SENSITIVE_COMPONENTS.contains(&name.as_str())
    })
}

/// True iff `candidate` is `base` itself or lies inside it. Uses component-wise
/// `strip_prefix`, so `/home/u/Documents_evil/x` is correctly NOT inside
/// `/home/u/Documents` (a naive `starts_with` on the string form would match).
fn is_within(candidate: &Path, base: &Path) -> bool {
    candidate.strip_prefix(base).is_ok()
}

/// True if `candidate` is inside at least one of `allowed_bases`. Each base is
/// canonicalized for the comparison so a base passed as a symlink (e.g. macOS
/// `/tmp` → `/private/tmp`) still matches a canonicalized candidate; a base that
/// does not resolve is skipped rather than treated as matching (fail-closed).
fn within_any_allowed(candidate: &Path, allowed_bases: &[PathBuf]) -> bool {
    allowed_bases.iter().any(|base| match base.canonicalize() {
        Ok(base_canon) => is_within(candidate, &base_canon),
        Err(_) => false,
    })
}

/// Validates a path to **read** an existing file, confining it to
/// `allowed_bases`. Canonicalization resolves `..` and symlinks, so a traversal
/// or a symlink that escapes the allow-list is rejected as
/// [`OutsideAllowed`](PathValidationError::OutsideAllowed).
///
/// Consumed by the `read_file` tool (koe-s7i) and the tool_dispatcher (koe-2gy).
#[allow(dead_code)]
pub fn validate_read_path(
    input: &str,
    allowed_bases: &[PathBuf],
) -> Result<PathBuf, PathValidationError> {
    check_length_and_nonempty(input)?;

    // Canonicalize resolves symlinks + `..` AND requires the target to exist.
    let canonical = Path::new(input)
        .canonicalize()
        .map_err(|_| PathValidationError::Unresolvable)?;

    if !canonical.is_file() {
        return Err(PathValidationError::NotAFile);
    }
    if contains_sensitive(&canonical) {
        return Err(PathValidationError::Sensitive);
    }
    if !within_any_allowed(&canonical, allowed_bases) {
        return Err(PathValidationError::OutsideAllowed);
    }
    Ok(canonical)
}

/// Validates a path to **write** a file (which may not exist yet), confining it
/// to `allowed_bases`. The file's *parent* must already exist and is
/// canonicalized; the file name is rejoined so a not-yet-created file is
/// accepted while traversal via the parent is still resolved away. If the target
/// already exists, it is fully canonicalized and re-checked so a pre-placed
/// symlink cannot redirect the write outside the allow-list.
///
/// Consumed by the `write_file` tool (koe-s7i) and the tool_dispatcher (koe-2gy).
#[allow(dead_code)]
pub fn validate_write_path(
    input: &str,
    allowed_bases: &[PathBuf],
) -> Result<PathBuf, PathValidationError> {
    check_length_and_nonempty(input)?;

    let raw = Path::new(input);
    // A trailing `/` or a path ending in `..` has no final file name to write.
    let file_name = raw
        .file_name()
        .ok_or(PathValidationError::Unresolvable)?
        .to_owned();
    // The parent directory must already exist; canonicalize it to resolve any
    // `..`/symlink before we trust the location.
    let parent = raw.parent().ok_or(PathValidationError::Unresolvable)?;
    let parent_canon = parent
        .canonicalize()
        .map_err(|_| PathValidationError::Unresolvable)?;
    let candidate = parent_canon.join(&file_name);

    // If the target already exists, resolve it fully — a symlink placed at the
    // target must not let the write land outside the allow-list.
    let resolved = match candidate.canonicalize() {
        Ok(existing) => {
            if !existing.is_file() {
                // Refuse to "write" over a directory (or other non-file).
                return Err(PathValidationError::NotAFile);
            }
            existing
        }
        Err(_) => {
            // `canonicalize` failed: the target is either a brand-new file (fine)
            // or a **dangling symlink** — one that points at a missing path. A
            // plain `write` through a dangling symlink follows the link and
            // creates the file at its (out-of-base) target, so reject anything
            // that is itself a symlink rather than trusting the in-base name.
            if std::fs::symlink_metadata(&candidate)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err(PathValidationError::OutsideAllowed);
            }
            candidate
        }
    };

    if contains_sensitive(&resolved) {
        return Err(PathValidationError::Sensitive);
    }
    if !within_any_allowed(&resolved, allowed_bases) {
        return Err(PathValidationError::OutsideAllowed);
    }
    Ok(resolved)
}

fn check_length_and_nonempty(input: &str) -> Result<(), PathValidationError> {
    if input.len() > MAX_PATH_LENGTH {
        return Err(PathValidationError::TooLong);
    }
    if input.trim().is_empty() {
        return Err(PathValidationError::Empty);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A temp dir used as the single allowed base. Canonicalized because the
    /// validators compare against canonicalized bases (and `/tmp` may itself be
    /// a symlink on macOS).
    fn allowed_base() -> (tempfile::TempDir, Vec<PathBuf>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let base = dir.path().canonicalize().expect("canon base");
        (dir, vec![base])
    }

    // ---- error messages are fixed + leak-free --------------------------------

    #[test]
    fn error_messages_are_fixed_and_leak_free() {
        for e in [
            PathValidationError::TooLong,
            PathValidationError::Empty,
            PathValidationError::Unresolvable,
            PathValidationError::OutsideAllowed,
            PathValidationError::Sensitive,
            PathValidationError::NotAFile,
        ] {
            let s = e.to_string();
            // No path separators and no slipped-in detail.
            assert!(!s.contains('/'), "{s:?} leaks a unix separator");
            assert!(!s.contains('\\'), "{s:?} leaks a windows separator");
            assert!(!s.is_empty());
        }
    }

    // ---- length / empty ------------------------------------------------------

    #[test]
    fn rejects_too_long_and_empty() {
        let (_d, bases) = allowed_base();
        let long = "a".repeat(MAX_PATH_LENGTH + 1);
        assert_eq!(
            validate_read_path(&long, &bases).unwrap_err(),
            PathValidationError::TooLong
        );
        assert_eq!(
            validate_read_path("   ", &bases).unwrap_err(),
            PathValidationError::Empty
        );
        assert_eq!(
            validate_write_path("", &bases).unwrap_err(),
            PathValidationError::Empty
        );
    }

    // ---- read ---------------------------------------------------------------

    #[test]
    fn read_accepts_file_inside_base() {
        let (dir, bases) = allowed_base();
        let file = dir.path().join("note.txt");
        fs::write(&file, b"hi").unwrap();
        let got = validate_read_path(file.to_str().unwrap(), &bases).expect("valid");
        assert!(got.is_file());
    }

    #[test]
    fn read_rejects_missing_file() {
        let (dir, bases) = allowed_base();
        let missing = dir.path().join("nope.txt");
        assert_eq!(
            validate_read_path(missing.to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::Unresolvable
        );
    }

    #[test]
    fn read_rejects_directory() {
        let (dir, bases) = allowed_base();
        assert_eq!(
            validate_read_path(dir.path().to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::NotAFile
        );
    }

    #[test]
    fn read_rejects_traversal_escaping_base() {
        // A file that exists OUTSIDE the base, reached via `..`, must be rejected
        // as outside-allowed (canonicalize resolves the `..` first).
        let (dir, bases) = allowed_base();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        fs::write(&secret, b"x").unwrap();
        let traversal = format!(
            "{}/../{}/secret.txt",
            dir.path().display(),
            outside.path().file_name().unwrap().to_string_lossy()
        );
        // The traversal resolves to a real file outside the base.
        assert_eq!(
            validate_read_path(&traversal, &bases).unwrap_err(),
            PathValidationError::OutsideAllowed
        );
    }

    #[test]
    fn read_rejects_sensitive_component() {
        let (dir, bases) = allowed_base();
        let ssh = dir.path().join(".ssh");
        fs::create_dir(&ssh).unwrap();
        let key = ssh.join("id_rsa");
        fs::write(&key, b"k").unwrap();
        assert_eq!(
            validate_read_path(key.to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::Sensitive
        );
    }

    // ---- write --------------------------------------------------------------

    #[test]
    fn write_accepts_new_file_inside_base() {
        let (dir, bases) = allowed_base();
        let target = dir.path().join("out.txt"); // does not exist yet
        let got = validate_write_path(target.to_str().unwrap(), &bases).expect("valid");
        assert_eq!(got.file_name().unwrap(), "out.txt");
        assert!(got.starts_with(dir.path().canonicalize().unwrap()));
    }

    #[test]
    fn write_rejects_missing_parent() {
        let (dir, bases) = allowed_base();
        let target = dir.path().join("nope_dir").join("out.txt");
        assert_eq!(
            validate_write_path(target.to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::Unresolvable
        );
    }

    #[test]
    fn write_rejects_outside_base() {
        let (_dir, bases) = allowed_base();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("out.txt");
        assert_eq!(
            validate_write_path(target.to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::OutsideAllowed
        );
    }

    #[test]
    fn write_rejects_over_directory() {
        let (dir, bases) = allowed_base();
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        assert_eq!(
            validate_write_path(sub.to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::NotAFile
        );
    }

    #[test]
    fn write_rejects_sensitive_component() {
        let (dir, bases) = allowed_base();
        let git = dir.path().join(".git");
        fs::create_dir(&git).unwrap();
        let target = git.join("config");
        assert_eq!(
            validate_write_path(target.to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::Sensitive
        );
    }

    // ---- sibling-dir boundary (the strip_prefix vs starts_with distinction) --

    #[test]
    fn sibling_directory_is_not_inside_base() {
        // `<base>_evil` shares a string prefix with `<base>` but is a sibling,
        // not a child — it must NOT be treated as inside.
        let parent = tempfile::tempdir().unwrap();
        let base = parent.path().join("allowed");
        let evil = parent.path().join("allowed_evil");
        fs::create_dir(&base).unwrap();
        fs::create_dir(&evil).unwrap();
        let bases = vec![base.canonicalize().unwrap()];
        let target = evil.join("out.txt");
        assert_eq!(
            validate_write_path(target.to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::OutsideAllowed
        );
    }

    // ---- symlink escapes (the headline security property) --------------------

    #[cfg(unix)]
    #[test]
    fn read_rejects_symlink_escaping_base() {
        use std::os::unix::fs::symlink;
        let (dir, bases) = allowed_base();
        let outside = tempfile::tempdir().unwrap();
        let real = outside.path().join("secret.txt");
        fs::write(&real, b"x").unwrap();
        // A symlink that lives INSIDE the base but points OUTSIDE it.
        let link = dir.path().join("link.txt");
        symlink(&real, &link).unwrap();
        // canonicalize() follows the link to the outside target.
        assert_eq!(
            validate_read_path(link.to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::OutsideAllowed
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_rejects_live_symlink_at_target_escaping_base() {
        use std::os::unix::fs::symlink;
        let (dir, bases) = allowed_base();
        let outside = tempfile::tempdir().unwrap();
        let real = outside.path().join("target.txt");
        fs::write(&real, b"x").unwrap();
        let link = dir.path().join("link.txt");
        symlink(&real, &link).unwrap();
        // The target exists (via the link); canonicalize resolves it outside.
        assert_eq!(
            validate_write_path(link.to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::OutsideAllowed
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_rejects_dangling_symlink_target() {
        use std::os::unix::fs::symlink;
        let (dir, bases) = allowed_base();
        let outside = tempfile::tempdir().unwrap();
        // Link inside the base pointing to a NON-existent outside path: writing
        // through it would create the file outside the base.
        let missing = outside.path().join("will-be-created.txt");
        let link = dir.path().join("dangling.txt");
        symlink(&missing, &link).unwrap();
        assert_eq!(
            validate_write_path(link.to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::OutsideAllowed
        );
    }

    // ---- additional confinement / traversal cases ----------------------------

    #[test]
    fn empty_allowed_bases_rejects_everything() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f.txt");
        fs::write(&file, b"x").unwrap();
        let bases: Vec<PathBuf> = vec![];
        assert_eq!(
            validate_read_path(file.to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::OutsideAllowed
        );
        let target = dir.path().join("new.txt");
        assert_eq!(
            validate_write_path(target.to_str().unwrap(), &bases).unwrap_err(),
            PathValidationError::OutsideAllowed
        );
    }

    #[test]
    fn write_rejects_parent_traversal_escaping_base() {
        // `<base>/../escape.txt` — the parent `<base>/..` canonicalizes to the
        // base's parent, so the target lands outside the base.
        let (dir, bases) = allowed_base();
        let traversal = format!("{}/../escape.txt", dir.path().display());
        assert_eq!(
            validate_write_path(&traversal, &bases).unwrap_err(),
            PathValidationError::OutsideAllowed
        );
    }

    #[test]
    fn write_accepts_overwriting_existing_file_inside_base() {
        let (dir, bases) = allowed_base();
        let file = dir.path().join("existing.txt");
        fs::write(&file, b"old").unwrap();
        let got = validate_write_path(file.to_str().unwrap(), &bases).expect("valid overwrite");
        assert!(got.is_file());
    }

    #[test]
    fn write_rejects_too_long() {
        let (_d, bases) = allowed_base();
        let long = "a".repeat(MAX_PATH_LENGTH + 1);
        assert_eq!(
            validate_write_path(&long, &bases).unwrap_err(),
            PathValidationError::TooLong
        );
    }
}
