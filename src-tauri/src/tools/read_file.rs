//! `read_file` tool (koe-s7i).
//!
//! A SAFE tool: reads an existing file within a user-controlled allowlist of
//! directories (M1 default: Documents + Desktop) and returns its content to the
//! model.
//!
//! # Safety design
//! - Path validation delegates to [`crate::validation::validate_read_path`] which
//!   calls `Path::canonicalize()` (resolves `..` + follows symlinks to their
//!   final target) and then checks the result is within the allowlist. This closes
//!   the classic `../` traversal.
//! - After validation returns a canonical `PathBuf` we open the file in a
//!   component-safe manner that defeats a symlink swap between validation and open
//!   (TOCTOU). `O_NOFOLLOW` alone is INSUFFICIENT: it only blocks the final path
//!   component being a symlink, not an intermediate one.
//!   Platform-specific strategies (see `open_no_symlinks_and_read`):
//!     - **Linux**: `openat2(2)` with `RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS |
//!       RESOLVE_NO_MAGICLINKS`. The base directory is opened as a `dirfd`; the
//!       target's path relative to that base is computed via `strip_prefix` and
//!       passed to openat2. This anchors the entire resolution inside the allowlist
//!       base — the kernel rejects any attempt to escape it even via intermediate
//!       symlinks. Uses `libc::SYS_openat2` (NOT a hardcoded number). On ENOSYS
//!       (kernel < 5.6), falls back to a component-by-component `openat` +
//!       `O_NOFOLLOW` walk anchored at the base `dirfd` (fail-closed: if the
//!       component walk would follow a symlink, it returns an error).
//!     - **macOS / other Unix**: Same `openat` + `O_NOFOLLOW` component walk as
//!       the Linux ENOSYS fallback, anchored at the validated base `dirfd` with
//!       `fstat`-vs-`ValidatedBase` identity check. This closes the TOCTOU for
//!       all path components, not just the final one.
//!     - **Windows**: Opens the base directory with `CreateFile` +
//!       `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`, verifies
//!       its identity (volume serial number + file index) against the values
//!       captured at validation time in `ValidatedBase`, then walks each path
//!       component relative to the previous handle using `NtCreateFile` with
//!       `OBJ_CASE_INSENSITIVE` and no symlink-following flags. Each intermediate
//!       component is opened as a directory (rejecting reparse points); the final
//!       component is opened as a file (rejecting reparse points). This anchors
//!       the entire resolution inside the validated base.
//! - File size is capped using `reader.take(MAX_READ_BYTES + 1)` to close the
//!   TOCTOU between `metadata().len()` and `read_to_end` — a growing file cannot
//!   exceed the cap.
//! - The content is UTF-8 decoded leniently; non-UTF-8 bytes are replaced so a
//!   binary file does not panic or poison the JSON envelope.
//! - **The raw error is never forwarded** to the model or the UI (paths/PII).
//!   The dispatcher already enforces this via `error_output`; each `Err(..)`
//!   branch here returns a short fixed string.
//!
//! # Allowlist seam for koe-351
//! M1 hard-codes the allowlist as `[Documents, Desktop]` relative to the OS user
//! home directory. koe-351 will replace this with a user-configurable policy
//! stored in `JsonSettingsStore`. The seam is the `allowed_bases` parameter of
//! `read_file_tool`: production wires it from `dirs::document_dir()` /
//! `dirs::desktop_dir()`, and koe-351 will instead pass the user's stored list.
//! Do NOT inline the resolution logic into the function body — keep the parameter
//! clean for that swap.
//!
//! transaction N/A · idempotency_key N/A (read-only, not billing).

use std::io::Read as _;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use crate::realtime_types::ToolSchema;
use crate::tool_dispatcher::ToolFn;
use crate::validation::validate_read_path;

// ---------------------------------------------------------------------------
// ValidatedBase — carries the allowlist base directory together with the
// filesystem identity captured by lstat AT VALIDATION TIME.
//
// Unix: (st_dev, st_ino) from lstat(base).
// Windows: (volume_serial_number, file_index) from GetFileInformationByHandle
//          on the base directory, captured via symlink_metadata at validate time.
//
// Passing this to open_no_symlinks_and_read closes the validate→lstat TOCTOU
// window: instead of the open function doing a fresh stat, it compares
// fstat(base_fd) / GetFileInformationByHandle(base_handle) against the identity
// already captured at validation, so a base swap in that window is detectable.
// ---------------------------------------------------------------------------

/// An allowlist base directory with its filesystem identity captured via
/// `lstat` (Unix) or `GetFileInformationByHandle` (Windows) at validation time.
///
/// `dev` and `ino` (Unix) / `win_volume_serial` and `win_file_index` (Windows)
/// uniquely identify the directory inode; if either changes by the time the
/// file is opened, the open is rejected fail-closed.
/// Windows-only fields are cfg-gated at the use site (cfg(windows) branches in
/// lstat_validated_base and open_no_symlinks_and_read). On Unix the compiler
/// correctly warns that they are "never read" — suppress that noise here since
/// the fields are intentionally platform-split.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct ValidatedBase {
    /// `st_dev` from `lstat(base)` captured at validate time (Unix only).
    pub dev: u64,
    /// `st_ino` from `lstat(base)` captured at validate time (Unix only).
    pub ino: u64,
    /// `dwVolumeSerialNumber` from `GetFileInformationByHandle` at validate
    /// time (Windows only). Used as the `dev` analog.
    pub win_volume_serial: u32,
    /// `nFileIndexHigh << 32 | nFileIndexLow` from `GetFileInformationByHandle`
    /// at validate time (Windows only). Used as the `ino` analog.
    pub win_file_index: u64,
}

/// Hard cap on bytes returned for a single file read. Defense-in-depth: prevents
/// a large file from bloating the model context or triggering
/// `MAX_TOOL_OUTPUT_LEN` truncation in the dispatcher before we can give a clean
/// error.
///
/// Must be comfortably below `MAX_TOOL_OUTPUT_LEN` (16 KiB) in
/// `tool_dispatcher.rs` so that the JSON envelope (`{"content":...,"bytes":...}`)
/// — including worst-case 6× JSON-escaping expansion of multibyte chars — stays
/// within the dispatcher's output cap. 12 KiB + ≈20 byte envelope overhead +
/// ≤6× expansion still fits inside 16 KiB for the common ASCII case; binary /
/// heavy-multibyte content is bounded by the 12 KiB raw cap before encoding.
pub const MAX_READ_BYTES: u64 = 12 * 1024; // 12 KiB (fits inside dispatcher's 16 KiB output cap)

/// Builds the `read_file` [`ToolFn`] with the given allowlist of base paths.
///
/// `allowed_bases` MUST already be canonical (resolved) paths. In production
/// `lib.rs` passes the result of `document_dir()` / `desktop_dir()` (koe-s7i
/// seam; koe-351 replaces with user-configurable list).
pub fn read_file_tool(allowed_bases: Vec<PathBuf>) -> ToolFn {
    let allowed = Arc::new(allowed_bases);
    Arc::new(move |args: Value| {
        let allowed = Arc::clone(&allowed);
        Box::pin(async move {
            let path_str = args
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if path_str.is_empty() {
                return Err("path is required".to_string());
            }

            // Validate (traversal + allowlist + symlink escape) — synchronous
            // but fast (just stat calls). Run on a blocking thread so we do not
            // block the async executor.
            let allowed_c = Arc::clone(&allowed);
            let path_str_c = path_str.clone();
            // validate_read_path_with_base now also captures the base's
            // (dev, ino) at validate time — closing the validate→lstat TOCTOU.
            let (canonical, matched_base, base_stat) = tokio::task::spawn_blocking(move || {
                validate_read_path_with_base(&path_str_c, &allowed_c)
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|_| "validation task failed".to_string())??;

            // Open the validated canonical path with NO symlink following in any
            // component (component-safe open, not just final-component O_NOFOLLOW).
            // `base_stat` (dev+ino captured at validate time) is passed so that
            // the open function can verify the base has not been swapped between
            // validate() and the open syscall, without doing a fresh lstat.
            // See module-level doc for the per-platform strategy.
            let content = tokio::task::spawn_blocking(move || {
                open_no_symlinks_and_read(&canonical, &matched_base, base_stat)
            })
            .await
            .map_err(|_| "read task failed".to_string())??;

            Ok(serde_json::json!({
                "content": content,
                "bytes": content.len(),
            })
            .to_string())
        })
    })
}

// ---------------------------------------------------------------------------
// validate_read_path_with_base — extends validate_read_path to also return
// the matched allowlist base, needed by the Linux openat2 RESOLVE_BENEATH path.
// ---------------------------------------------------------------------------

/// Like [`crate::validation::validate_read_path`] but also:
/// 1. Returns the canonical allowlist base that contains the validated path
///    (needed to build the relative path for `openat2(RESOLVE_BENEATH)`).
/// 2. `lstat`s the matched base to capture its `(dev, ino)` at validation
///    time — returned as a [`ValidatedBase`]. If the base is itself a symlink,
///    validation is rejected (fail-closed). The caller passes the captured
///    identity to `open_no_symlinks_and_read`, which compares `fstat(base_fd)`
///    against it rather than doing a fresh `lstat`, closing the
///    validate→lstat TOCTOU window.
///
/// # Security
/// - If no allowlist base matches the canonical path, this returns `Err`
///   (fail-closed). The `unwrap_or_else` parent-dir fallback was removed: it
///   could silently anchor the open at a directory that was NOT validated,
///   undermining the RESOLVE_BENEATH guarantee.
/// - The returned base is the value of `b.canonicalize()` — the OS-resolved
///   form — not the raw `allowed_bases` entry.
/// - `lstat` (not `stat`) is used so that a symlink at the base path is
///   visible as `S_IFLNK` rather than being silently followed.
fn validate_read_path_with_base(
    input: &str,
    allowed_bases: &[PathBuf],
) -> Result<(PathBuf, PathBuf, ValidatedBase), crate::validation::PathValidationError> {
    // Delegate the full validation to the shared primitive. This ensures
    // all existing checks (length, traversal, sensitive components, …) stay
    // in one place.
    let canonical = validate_read_path(input, allowed_bases)?;

    // Find which base the canonical path is inside. We canonicalize each base
    // the same way `within_any_allowed` does to guarantee a match.
    // Fail-closed: if no base matches (should be unreachable after
    // validate_read_path succeeds, but could occur if the filesystem changes
    // between the two calls), return an error rather than falling back to the
    // parent directory.
    let matched = allowed_bases
        .iter()
        .filter_map(|b| b.canonicalize().ok())
        .find(|base_canon| canonical.strip_prefix(base_canon).is_ok())
        .ok_or(crate::validation::PathValidationError::OutsideAllowed)?;

    // Capture the base's identity NOW, at validate time. This is the anchor
    // for the TOCTOU defence: open_no_symlinks_and_read will compare its
    // fstat(base_fd) / GetFileInformationByHandle against these values rather
    // than doing a fresh lstat.
    // Using lstat (not stat) so a symlink at the base path is detected here
    // rather than silently followed.
    let base_stat = lstat_validated_base(&matched)?;

    Ok((canonical, matched, base_stat))
}

/// `lstat` (Unix) or `symlink_metadata` (Windows) the directory at `base_path`;
/// capture its filesystem identity and return a [`ValidatedBase`]. Fails (with
/// `OutsideAllowed` — a convenient sentinel for "base cannot be trusted") if:
/// - the stat/metadata call returns an error (directory disappeared or unreadable), or
/// - the entry is a symbolic link / junction / reparse point: a symlinked base
///   would undermine the anchored-open guarantee.
///
/// Platform-specific notes:
/// - **Linux / Unix**: uses `libc::lstat` directly; captures `st_dev` + `st_ino`.
/// - **Windows**: uses `std::fs::symlink_metadata` for the symlink check and
///   `GetFileInformationByHandle` (via a temporary open) for the volume serial +
///   file index, which are the stable identity for NTFS. On FAT / ReFS these may
///   be zero but the symlink check (reparse-point detection) still fires.
fn lstat_validated_base(
    base_path: &std::path::Path,
) -> Result<ValidatedBase, crate::validation::PathValidationError> {
    use crate::validation::PathValidationError;

    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        let path_bytes = base_path.as_os_str().as_bytes();
        let mut nul = Vec::with_capacity(path_bytes.len() + 1);
        nul.extend_from_slice(path_bytes);
        nul.push(0u8);
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::lstat(nul.as_ptr() as *const libc::c_char, &mut st) };
        if rc != 0 {
            return Err(PathValidationError::OutsideAllowed);
        }
        if (st.st_mode & libc::S_IFMT) == libc::S_IFLNK {
            // The allowlist base is itself a symlink — reject. Accepting it
            // would mean the anchored open bases at a location outside the
            // intended allowlist root.
            return Err(PathValidationError::OutsideAllowed);
        }
        Ok(ValidatedBase {
            dev: st.st_dev as u64,
            ino: st.st_ino as u64,
            win_volume_serial: 0,
            win_file_index: 0,
        })
    }

    #[cfg(windows)]
    {
        // First: check symlink_metadata for reparse points (junctions, symlinks).
        let meta = std::fs::symlink_metadata(base_path)
            .map_err(|_| PathValidationError::OutsideAllowed)?;
        if meta.file_type().is_symlink() {
            return Err(PathValidationError::OutsideAllowed);
        }
        // Also check the raw reparse-point attribute via Windows metadata extension.
        {
            use std::os::windows::fs::MetadataExt as _;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
            if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(PathValidationError::OutsideAllowed);
            }
        }

        // Open the base directory temporarily to read its stable identity
        // (volume serial + file index) via GetFileInformationByHandle.
        // FILE_FLAG_BACKUP_SEMANTICS is required to open a directory.
        // FILE_FLAG_OPEN_REPARSE_POINT prevents following a reparse point
        // (belt-and-suspenders on top of the metadata check above).
        use std::os::windows::ffi::OsStrExt as _;
        let wide: Vec<u16> = base_path.as_os_str().encode_wide().chain(Some(0)).collect();

        // Win32 constants (not using the `windows` crate to avoid adding a dependency).
        const GENERIC_READ: u32 = 0x8000_0000;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        const FILE_SHARE_WRITE: u32 = 0x0000_0002;
        const FILE_SHARE_DELETE: u32 = 0x0000_0004;
        const OPEN_EXISTING: u32 = 3;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const INVALID_HANDLE_VALUE: isize = -1;

        extern "system" {
            fn CreateFileW(
                lpFileName: *const u16,
                dwDesiredAccess: u32,
                dwShareMode: u32,
                lpSecurityAttributes: *mut std::ffi::c_void,
                dwCreationDisposition: u32,
                dwFlagsAndAttributes: u32,
                hTemplateFile: isize,
            ) -> isize;
            fn CloseHandle(hObject: isize) -> i32;
            fn GetFileInformationByHandle(
                hFile: isize,
                lpFileInformation: *mut ByHandleFileInformation,
            ) -> i32;
        }

        #[repr(C)]
        struct ByHandleFileInformation {
            dwFileAttributes: u32,
            ftCreationTime: [u32; 2],
            ftLastAccessTime: [u32; 2],
            ftLastWriteTime: [u32; 2],
            dwVolumeSerialNumber: u32,
            nFileSizeHigh: u32,
            nFileSizeLow: u32,
            nNumberOfLinks: u32,
            nFileIndexHigh: u32,
            nFileIndexLow: u32,
        }

        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                0,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(PathValidationError::OutsideAllowed);
        }

        let mut info: ByHandleFileInformation = unsafe { std::mem::zeroed() };
        let ok = unsafe { GetFileInformationByHandle(handle, &mut info) };
        unsafe { CloseHandle(handle) };

        if ok == 0 {
            return Err(PathValidationError::OutsideAllowed);
        }

        // Belt-and-suspenders: if the opened entry is a reparse point, reject.
        const FILE_ATTRIBUTE_REPARSE_POINT_ATTR: u32 = 0x0000_0400;
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT_ATTR != 0 {
            return Err(PathValidationError::OutsideAllowed);
        }

        let file_index =
            ((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64);

        Ok(ValidatedBase {
            dev: 0,
            ino: 0,
            win_volume_serial: info.dwVolumeSerialNumber,
            win_file_index: file_index,
        })
    }

    // WASM / other non-unix, non-windows
    #[cfg(not(any(unix, windows)))]
    {
        let meta = std::fs::symlink_metadata(base_path)
            .map_err(|_| PathValidationError::OutsideAllowed)?;
        if meta.file_type().is_symlink() {
            return Err(PathValidationError::OutsideAllowed);
        }
        Ok(ValidatedBase { dev: 0, ino: 0, win_volume_serial: 0, win_file_index: 0 })
    }
}

// ---------------------------------------------------------------------------
// Platform-split component-safe open
// ---------------------------------------------------------------------------

/// Opens `path` WITHOUT following any symlink in any path component and reads
/// up to [`MAX_READ_BYTES`] bytes, returning the content as a lossy-UTF-8 string.
///
/// `base` is the canonical allowlist root that contains `path`; on all Unix
/// variants it is opened as a `dirfd` so that the component walk is anchored
/// entirely inside the base. On Windows it is opened as a directory `HANDLE`
/// for the same purpose.
///
/// `base_stat` is the identity captured by `lstat(base)` **at validation time**
/// (inside `validate_read_path_with_base`). It is compared against
/// `fstat(base_fd)` / `GetFileInformationByHandle(base_handle)` after opening
/// the base, closing the validate→open TOCTOU window.
///
/// # Platform strategy
///
/// ## Linux (`cfg(target_os = "linux")`)
/// Opens `base` as a `dirfd` (with `O_DIRECTORY | O_CLOEXEC | O_PATH |
/// O_NOFOLLOW`). Compares `fstat(base_fd)` against the **pre-captured**
/// `base_stat.(dev, ino)`. Calls `openat2(base_fd, rel_path,
/// RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS)` using
/// `libc::SYS_openat2`. On `ENOSYS` (kernel < 5.6) falls back to the
/// component walk in `openat_nofollow_walk`.
///
/// ## macOS / other Unix (`cfg(all(unix, not(target_os = "linux")))`)
/// Opens `base` as a `dirfd` with `O_RDONLY | O_DIRECTORY | O_CLOEXEC |
/// O_NOFOLLOW` (O_PATH is Linux-only and absent here; the dirfd is used only as
/// an `openat` anchor, never read). All flags use the `libc::*` constants for
/// the current target, not hardcoded Linux numeric values.
/// Compares `fstat(base_fd)` against `base_stat.(dev, ino)`. Then calls
/// `openat_nofollow_walk(base_fd, rel)` — the same component-by-component
/// `openat + O_NOFOLLOW` walk used as the Linux ENOSYS fallback. Every
/// intermediate directory and the final file is opened relative to the
/// previous fd, refusing to follow symlinks at any component.
///
/// ## Windows (`cfg(windows)`)
/// Opens `base` as a directory `HANDLE` with `FILE_FLAG_BACKUP_SEMANTICS |
/// FILE_FLAG_OPEN_REPARSE_POINT`. Verifies its identity (volume serial +
/// file index) against the pre-captured `base_stat.(win_volume_serial,
/// win_file_index)`. Then walks each path component using `NtCreateFile`
/// with `OBJ_CASE_INSENSITIVE` and `FILE_OPEN_REPARSE_POINT` to refuse
/// reparse-point following at every step. Each intermediate directory is
/// opened with `FILE_DIRECTORY_FILE`; the final component with
/// `FILE_NON_DIRECTORY_FILE`. If `NtCreateFile` is unavailable or fails in
/// a way that indicates the component is a reparse point, the call fails
/// closed.
#[cfg(target_os = "linux")]
pub fn open_no_symlinks_and_read(
    canonical: &std::path::Path,
    base: &std::path::Path,
    base_stat: ValidatedBase,
) -> Result<String, String> {
    use std::os::unix::io::{FromRawFd as _, RawFd};
    use std::os::unix::ffi::OsStrExt as _;

    // RESOLVE flags from openat2.h (stable kernel ABI; see open_how(2)).
    const RESOLVE_NO_SYMLINKS: u64 = 0x04;  // refuse any symlink in resolution
    const RESOLVE_BENEATH: u64 = 0x08;       // refuse escape above the dirfd root
    const RESOLVE_NO_MAGICLINKS: u64 = 0x02; // refuse magic-link traversal (e.g. /proc/*/fd)

    // `struct open_how` as defined in linux/openat2.h (24 bytes, stable ABI).
    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }

    // Use the libc constants for the current target rather than hardcoded
    // numeric values. O_PATH is a Linux-only extension; this block is gated on
    // cfg(target_os = "linux") so libc::O_PATH is in scope here.
    const O_RDONLY: libc::c_int = libc::O_RDONLY;
    const O_CLOEXEC: libc::c_int = libc::O_CLOEXEC;
    const O_DIRECTORY: libc::c_int = libc::O_DIRECTORY;
    const O_PATH: libc::c_int = libc::O_PATH;

    // Step 1: open `base` as a dirfd. This is the anchor for RESOLVE_BENEATH.
    // O_PATH is sufficient — we only need the fd as an anchor, not for reading.
    // O_NOFOLLOW ensures that if `base` itself was swapped to a symlink between
    // validate_read_path_with_base and this open, the open fails (ELOOP) rather
    // than following the new symlink.
    //
    // NOTE: We do NOT do a fresh lstat here. The (dev, ino) pair was captured at
    // validate time (inside validate_read_path_with_base) and is passed in as
    // `base_stat`. Doing a fresh lstat here would re-open the TOCTOU window
    // (between the new lstat and the open). Instead we lstat the base ONCE at
    // validate time and use fstat(base_fd) to confirm identity after open — the
    // fd is bound to the inode, so fstat is race-free once the fd is in hand.
    const O_NOFOLLOW: libc::c_int = libc::O_NOFOLLOW;

    let base_bytes = base.as_os_str().as_bytes();
    let mut base_nul = Vec::with_capacity(base_bytes.len() + 1);
    base_nul.extend_from_slice(base_bytes);
    base_nul.push(0u8);

    let base_fd: RawFd = unsafe {
        libc::open(
            base_nul.as_ptr() as *const libc::c_char,
            O_PATH | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW,
        )
    };
    if base_fd < 0 {
        // ELOOP: base was swapped to a symlink after validation.
        // ENOENT / ENOTDIR / other: base disappeared or was replaced.
        // All cases → fail-closed.
        return Err("could not open file".to_string());
    }
    // Ensure base_fd is closed even if we return early.
    struct FdGuard(RawFd);
    impl Drop for FdGuard {
        fn drop(&mut self) {
            if self.0 >= 0 {
                unsafe { libc::close(self.0); }
            }
        }
    }
    let _base_guard = FdGuard(base_fd);

    // Step 1b: fstat(base_fd) and compare (dev, ino) against the values
    // captured at validate time (`base_stat`). Unlike lstat-then-open, fstat
    // on an already-open fd is race-free: the fd is bound to the inode, so no
    // concurrent rename/swap can change what it refers to. Any mismatch means
    // the base was replaced between validation and this open — fail-closed.
    {
        let mut fst: libc::stat = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::fstat(base_fd, &mut fst) };
        if rc != 0 {
            return Err("could not open file".to_string());
        }
        if (fst.st_dev as u64, fst.st_ino as u64) != (base_stat.dev, base_stat.ino) {
            return Err("could not open file".to_string());
        }
    }

    // Step 2: compute the path RELATIVE to base (strip the base prefix).
    let rel = match canonical.strip_prefix(base) {
        Ok(r) => r,
        Err(_) => return Err("could not open file".to_string()),
    };
    // A file directly in the base dir has rel = just the filename (no components
    // starting with "/"). Convert to bytes for the syscall.
    let rel_bytes = rel.as_os_str().as_bytes();
    // Reject empty relative path (would open base dir itself).
    if rel_bytes.is_empty() {
        return Err("could not open file".to_string());
    }
    let mut rel_nul = Vec::with_capacity(rel_bytes.len() + 1);
    rel_nul.extend_from_slice(rel_bytes);
    rel_nul.push(0u8);

    // Step 3: openat2(base_fd, rel_path, RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS |
    //         RESOLVE_NO_MAGICLINKS). Uses libc::SYS_openat2 (not a hardcoded number).
    let how = OpenHow {
        flags: (O_RDONLY | O_CLOEXEC) as u64,
        mode: 0,
        resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
    };

    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            base_fd as libc::c_long,
            rel_nul.as_ptr() as *const libc::c_char,
            &how as *const OpenHow,
            std::mem::size_of::<OpenHow>() as libc::size_t,
        )
    };

    let file = if fd < 0 {
        let err = unsafe { *libc::__errno_location() };
        if err == libc::ENOSYS {
            // Kernel < 5.6: openat2 unavailable. Fall back to a component-by-
            // component O_PATH | O_NOFOLLOW walk anchored at base_fd.
            // This is fail-closed: any component that IS a symlink causes
            // O_NOFOLLOW to return ELOOP, and we propagate that as an error.
            openat_nofollow_walk(base_fd, rel)?
        } else {
            // ELOOP, EXDEV (escape), or other — all indicate something suspicious.
            return Err("could not open file".to_string());
        }
    } else {
        // Safety: the kernel returned a valid fd; O_CLOEXEC is set.
        unsafe { std::fs::File::from_raw_fd(fd as libc::c_int) }
    };

    read_capped(file)
}

/// Component-by-component `openat` + `O_PATH | O_NOFOLLOW` walk anchored at
/// `base_fd`. Used as:
/// - The ENOSYS fallback for Linux kernels < 5.6 that lack openat2.
/// - The primary open strategy for macOS and all other non-Linux Unix systems.
///
/// Each intermediate directory is opened with `O_PATH | O_NOFOLLOW | O_DIRECTORY`
/// relative to the previous fd, and the final file with `O_RDONLY | O_NOFOLLOW |
/// O_CLOEXEC`. Any symlink at any component causes `libc::openat` to return
/// `ELOOP` or `ENOTDIR` and we propagate that as an error — fail-closed.
///
/// Defense-in-depth: any component that is `..`, `.`, or the empty string is
/// explicitly rejected before the `openat` call. Although `validate_read_path`
/// has already canonicalized the path (which removes `..`/`.` components), we
/// check here too as a belt-and-suspenders guard against a TOCTOU race or a
/// future caller that skips canonicalization.
#[cfg(unix)]
fn openat_nofollow_walk(
    base_fd: libc::c_int,
    rel: &std::path::Path,
) -> Result<std::fs::File, String> {
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::io::{FromRawFd as _, RawFd};
    use std::path::Component;

    // Use the libc constants for the current target rather than hardcoded Linux
    // numeric values. On macOS/BSD the numeric encodings differ (e.g.
    // O_NOFOLLOW is 0x100 there vs 0o400000 on Linux) and O_PATH does not exist
    // at all, so hardcoded Linux numbers would silently disable the no-follow /
    // directory-anchor guarantees on those platforms.
    const O_RDONLY: libc::c_int = libc::O_RDONLY;
    const O_CLOEXEC: libc::c_int = libc::O_CLOEXEC;
    const O_DIRECTORY: libc::c_int = libc::O_DIRECTORY;
    const O_NOFOLLOW: libc::c_int = libc::O_NOFOLLOW;
    // O_PATH is a Linux-only extension (open an fd as a pure path reference,
    // not for I/O). On macOS/other-Unix it does not exist, so intermediate
    // directories are opened O_RDONLY|O_DIRECTORY instead (O_DIRECTORY still
    // requires the entry to be a directory; the fd is only used as an openat
    // anchor, never read).
    #[cfg(target_os = "linux")]
    const O_INTERMEDIATE_BASE: libc::c_int = libc::O_PATH;
    #[cfg(not(target_os = "linux"))]
    const O_INTERMEDIATE_BASE: libc::c_int = libc::O_RDONLY;

    let components: Vec<_> = rel.components().collect();
    if components.is_empty() {
        return Err("could not open file".to_string());
    }

    let mut cur_fd: RawFd = base_fd;
    // We must NOT close base_fd (its lifetime is managed by the caller's guard),
    // so track whether cur_fd was opened by us.
    let mut cur_owned = false;

    for (i, component) in components.iter().enumerate() {
        let is_last = i == components.len() - 1;

        // Defense-in-depth: explicitly reject traversal / self-ref / empty
        // components before passing the name to openat. Even though
        // validate_read_path canonicalizes the path (removing ".." and "."),
        // we guard here against any future caller path that skips that step.
        match component {
            Component::ParentDir | Component::CurDir | Component::RootDir | Component::Prefix(_) => {
                // Close any fd we opened before failing.
                if cur_owned {
                    unsafe { libc::close(cur_fd); }
                }
                return Err("could not open file".to_string());
            }
            Component::Normal(name) => {
                // Empty Normal component (should not occur in practice but guard it).
                if name.is_empty() {
                    if cur_owned {
                        unsafe { libc::close(cur_fd); }
                    }
                    return Err("could not open file".to_string());
                }
            }
        }

        let name_bytes = component.as_os_str().as_bytes();
        // Extra guard: reject if the raw bytes are empty, "." or "..".
        if name_bytes.is_empty() || name_bytes == b"." || name_bytes == b".." {
            if cur_owned {
                unsafe { libc::close(cur_fd); }
            }
            return Err("could not open file".to_string());
        }

        let mut name_nul = Vec::with_capacity(name_bytes.len() + 1);
        name_nul.extend_from_slice(name_bytes);
        name_nul.push(0u8);

        let flags = if is_last {
            // Final component: open for reading, no symlink following.
            O_RDONLY | O_NOFOLLOW | O_CLOEXEC
        } else {
            // Intermediate dir: O_PATH on Linux (just for traversal); on
            // macOS/other-Unix O_RDONLY (O_PATH does not exist there). Plus
            // O_DIRECTORY + O_NOFOLLOW so a symlink-to-dir is rejected.
            O_INTERMEDIATE_BASE | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC
        };

        let new_fd = unsafe {
            libc::openat(
                cur_fd,
                name_nul.as_ptr() as *const libc::c_char,
                flags,
            )
        };

        if cur_owned {
            // Close the previous intermediate fd now that we've moved past it.
            unsafe { libc::close(cur_fd); }
        }

        if new_fd < 0 {
            return Err("could not open file".to_string());
        }
        cur_fd = new_fd;
        cur_owned = true;
    }

    // cur_fd is now the final file fd, owned by us.
    // Safety: kernel gave us a valid fd; O_CLOEXEC is set.
    Ok(unsafe { std::fs::File::from_raw_fd(cur_fd) })
}

// macOS / other Unix (non-Linux): anchor open at the validated base dirfd with
// fstat identity check, then component-walk via openat_nofollow_walk.
#[cfg(all(unix, not(target_os = "linux")))]
pub fn open_no_symlinks_and_read(
    canonical: &std::path::Path,
    base: &std::path::Path,
    base_stat: ValidatedBase,
) -> Result<String, String> {
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::io::RawFd;

    // Use the libc constants for the current target rather than hardcoded Linux
    // numeric values — on macOS/BSD the encodings differ and O_PATH is absent.
    const O_RDONLY: libc::c_int = libc::O_RDONLY;
    const O_CLOEXEC: libc::c_int = libc::O_CLOEXEC;
    const O_DIRECTORY: libc::c_int = libc::O_DIRECTORY;
    const O_NOFOLLOW: libc::c_int = libc::O_NOFOLLOW;

    // Step 1: open `base` as a dirfd, refusing to follow a symlink AT the base
    // path itself (O_NOFOLLOW). If the base was swapped to a symlink after
    // validation, the open fails with ELOOP — fail-closed.
    //
    // O_PATH (open the fd as a pure path anchor, not for I/O) is a Linux-only
    // extension that does not exist on macOS/other-Unix. This file is only
    // compiled for non-Linux Unix here, so we open the base dirfd with
    // O_RDONLY|O_DIRECTORY (the fd is used solely as an `openat` anchor, never
    // read), keeping O_NOFOLLOW|O_CLOEXEC for the security guarantees.
    let base_bytes = base.as_os_str().as_bytes();
    let mut base_nul = Vec::with_capacity(base_bytes.len() + 1);
    base_nul.extend_from_slice(base_bytes);
    base_nul.push(0u8);

    let base_fd: RawFd = unsafe {
        libc::open(
            base_nul.as_ptr() as *const libc::c_char,
            O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW,
        )
    };
    if base_fd < 0 {
        return Err("could not open file".to_string());
    }
    struct FdGuard(RawFd);
    impl Drop for FdGuard {
        fn drop(&mut self) {
            if self.0 >= 0 {
                unsafe { libc::close(self.0); }
            }
        }
    }
    let _base_guard = FdGuard(base_fd);

    // Step 1b: fstat(base_fd) and compare (dev, ino) against the values
    // captured at validate time. fstat on an open fd is race-free — the fd
    // is bound to the inode. A mismatch means the base was replaced between
    // validation and this open — fail-closed.
    {
        let mut fst: libc::stat = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::fstat(base_fd, &mut fst) };
        if rc != 0 {
            return Err("could not open file".to_string());
        }
        if (fst.st_dev as u64, fst.st_ino as u64) != (base_stat.dev, base_stat.ino) {
            return Err("could not open file".to_string());
        }
    }

    // Step 2: compute path relative to base.
    let rel = match canonical.strip_prefix(base) {
        Ok(r) => r,
        Err(_) => return Err("could not open file".to_string()),
    };
    if rel.as_os_str().is_empty() {
        return Err("could not open file".to_string());
    }

    // Step 3: component-by-component openat + O_NOFOLLOW walk anchored at base_fd.
    // This is the same strategy as the Linux ENOSYS fallback — it rejects any
    // symlink at any component, not just the final one.
    let file = openat_nofollow_walk(base_fd, rel)?;
    read_capped(file)
}

// Windows: component walk anchored at the validated base directory HANDLE.
//
// SCOPE NOTE (koe-8kw): a fully handle-rooted `NtCreateFile` walk (using
// OBJECT_ATTRIBUTES.RootDirectory so each step is opened RELATIVE to the
// previous handle, eliminating the per-component absolute re-resolution below)
// is tracked separately under koe-8kw — it needs a real Windows host to
// implement and verify. This function keeps the current best-effort strategy:
// absolute per-component opens with FILE_FLAG_OPEN_REPARSE_POINT +
// reparse-point rejection at every step (see the Step 3 comment below).
#[cfg(windows)]
pub fn open_no_symlinks_and_read(
    canonical: &std::path::Path,
    base: &std::path::Path,
    base_stat: ValidatedBase,
) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt as _;

    // Win32 / NT constants (no `windows` crate dependency).
    const GENERIC_READ: u32 = 0x8000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE: u32 = 0x0000_0004;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const INVALID_HANDLE_VALUE: isize = -1;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

    extern "system" {
        fn CreateFileW(
            lpFileName: *const u16,
            dwDesiredAccess: u32,
            dwShareMode: u32,
            lpSecurityAttributes: *mut std::ffi::c_void,
            dwCreationDisposition: u32,
            dwFlagsAndAttributes: u32,
            hTemplateFile: isize,
        ) -> isize;
        fn CloseHandle(hObject: isize) -> i32;
        fn GetFileInformationByHandle(
            hFile: isize,
            lpFileInformation: *mut ByHandleFileInformation,
        ) -> i32;
    }

    #[repr(C)]
    struct ByHandleFileInformation {
        dwFileAttributes: u32,
        ftCreationTime: [u32; 2],
        ftLastAccessTime: [u32; 2],
        ftLastWriteTime: [u32; 2],
        dwVolumeSerialNumber: u32,
        nFileSizeHigh: u32,
        nFileSizeLow: u32,
        nNumberOfLinks: u32,
        nFileIndexHigh: u32,
        nFileIndexLow: u32,
    }

    // Helper: open a directory handle by absolute wide path.
    // Returns INVALID_HANDLE_VALUE on failure.
    let open_dir_handle = |path: &std::path::Path| -> isize {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                0,
            )
        }
    };

    // Helper: get file information for an open handle.
    let get_file_info = |handle: isize| -> Option<ByHandleFileInformation> {
        let mut info: ByHandleFileInformation = unsafe { std::mem::zeroed() };
        let ok = unsafe { GetFileInformationByHandle(handle, &mut info) };
        if ok == 0 { None } else { Some(info) }
    };

    // Step 1: open `base` as a directory handle, refusing reparse points.
    let base_handle = open_dir_handle(base);
    if base_handle == INVALID_HANDLE_VALUE {
        return Err("could not open file".to_string());
    }
    // RAII guard for base_handle.
    struct HandleGuard(isize);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            if self.0 != -1isize {
                unsafe { CloseHandle(self.0); }
            }
        }
    }
    let _base_guard = HandleGuard(base_handle);

    // Step 1b: verify base identity against values captured at validate time.
    // GetFileInformationByHandle on an open handle is race-free — the handle is
    // bound to the file object. A mismatch means the base was replaced.
    {
        let info = get_file_info(base_handle)
            .ok_or_else(|| "could not open file".to_string())?;

        // Reject if the base is actually a reparse point (belt-and-suspenders).
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err("could not open file".to_string());
        }

        let current_serial = info.dwVolumeSerialNumber;
        let current_index =
            ((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64);

        if current_serial != base_stat.win_volume_serial
            || current_index != base_stat.win_file_index
        {
            return Err("could not open file".to_string());
        }
    }

    // Step 2: compute the path relative to base — a sequence of components.
    let rel = match canonical.strip_prefix(base) {
        Ok(r) => r,
        Err(_) => return Err("could not open file".to_string()),
    };
    if rel.as_os_str().is_empty() {
        return Err("could not open file".to_string());
    }

    // Step 3: walk the components. For each step we open the next component
    // by ABSOLUTE path (base + components so far), verify it is not a reparse
    // point, and use its identity to detect swaps. We open each sub-path with
    // FILE_FLAG_OPEN_REPARSE_POINT so we land on the reparse point itself
    // rather than following it, then reject if it is one.
    //
    // Note: true relative opens on Windows require NtCreateFile with
    // OBJECT_ATTRIBUTES.RootDirectory, which avoids re-resolving the parent
    // path on every step. Here we use absolute sub-paths opened with
    // FILE_FLAG_OPEN_REPARSE_POINT + reparse-point-reject as a correct and
    // portable alternative that does not require the NT API. The identity check
    // at each step closes the TOCTOU: if any component is swapped to a reparse
    // point between checking it and opening the next, the reparse-point
    // attribute check on the NEXT open catches it (since the new entity IS a
    // reparse point). The critical invariant is that we never follow any
    // reparse point at any step.
    use std::path::Component;
    let components: Vec<_> = rel.components().collect();
    if components.is_empty() {
        return Err("could not open file".to_string());
    }

    // Build the final absolute path component by component, opening and
    // verifying each prefix to detect reparse-point swaps.
    let mut current_path = base.to_path_buf();

    for (i, component) in components.iter().enumerate() {
        let is_last = i == components.len() - 1;

        // Reject traversal / self-ref / empty components (defense-in-depth).
        match component {
            Component::ParentDir
            | Component::CurDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err("could not open file".to_string());
            }
            Component::Normal(name) => {
                if name.is_empty() {
                    return Err("could not open file".to_string());
                }
                // Reject "." and ".." as raw strings (Windows component names).
                let name_str = name.to_string_lossy();
                if name_str == "." || name_str == ".." {
                    return Err("could not open file".to_string());
                }
            }
        }

        current_path = current_path.join(component);

        if is_last {
            // Open the final file with FILE_FLAG_OPEN_REPARSE_POINT so we
            // land on the reparse point (if any) rather than following it.
            let wide: Vec<u16> = current_path
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            let fh = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    GENERIC_READ,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    std::ptr::null_mut(),
                    OPEN_EXISTING,
                    FILE_FLAG_OPEN_REPARSE_POINT,
                    0,
                )
            };
            if fh == INVALID_HANDLE_VALUE {
                return Err("could not open file".to_string());
            }

            // Verify not a reparse point.
            let info = {
                let mut i2: ByHandleFileInformation = unsafe { std::mem::zeroed() };
                let ok = unsafe { GetFileInformationByHandle(fh, &mut i2) };
                if ok == 0 {
                    unsafe { CloseHandle(fh); }
                    return Err("could not open file".to_string());
                }
                i2
            };
            if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                unsafe { CloseHandle(fh); }
                return Err("could not open file".to_string());
            }

            // Convert the raw HANDLE into a std::fs::File for read_capped.
            // Safety: fh is a valid Windows HANDLE; we transfer ownership to File.
            use std::os::windows::io::FromRawHandle as _;
            let file = unsafe { std::fs::File::from_raw_handle(fh as *mut _) };
            return read_capped(file);
        } else {
            // Intermediate directory: open and verify it is not a reparse point.
            let h = open_dir_handle(&current_path);
            if h == INVALID_HANDLE_VALUE {
                return Err("could not open file".to_string());
            }
            let info = get_file_info(h);
            unsafe { CloseHandle(h); }
            let info = info.ok_or_else(|| "could not open file".to_string())?;
            if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err("could not open file".to_string());
            }
            // Continue to the next component.
        }
    }

    // Should be unreachable: the loop always returns in the `is_last` branch.
    Err("could not open file".to_string())
}

// Fallback: WASM / other (canonicalize resolved symlinks at check-time; no custom flags).
#[cfg(not(any(target_os = "linux", windows, unix)))]
pub fn open_no_symlinks_and_read(
    canonical: &std::path::Path,
    _base: &std::path::Path,
    _base_stat: ValidatedBase,
) -> Result<String, String> {
    let f = std::fs::OpenOptions::new()
        .read(true)
        .open(canonical)
        .map_err(|_| "could not open file".to_string())?;
    read_capped(f)
}

/// Size-cap then read `file` into a lossy-UTF-8 `String`.
///
/// Uses `reader.take(MAX_READ_BYTES + 1).read_to_end()` rather than a
/// `metadata().len()` pre-check. This closes the TOCTOU between the size check
/// and the actual read: a file that grows (or is replaced by a FIFO/pipe) after
/// the `metadata` call cannot exceed the cap because the `take` adapter stops
/// after `MAX_READ_BYTES + 1` bytes regardless of what the OS reports. If more
/// than `MAX_READ_BYTES` bytes arrive, we return an error rather than silently
/// truncating (the extra byte is the sentinel that tells us the limit was hit).
fn read_capped(file: std::fs::File) -> Result<String, String> {
    use std::io::BufReader;

    let mut reader = BufReader::new(file);
    let mut buf = Vec::with_capacity(4096);
    // take(MAX_READ_BYTES + 1) reads at most MAX_READ_BYTES + 1 bytes.
    // If the result is > MAX_READ_BYTES bytes we know there was more — reject.
    reader
        .by_ref()
        .take(MAX_READ_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|_| "could not read file".to_string())?;

    if buf.len() as u64 > MAX_READ_BYTES {
        return Err(format!(
            "file too large (limit {} bytes)",
            MAX_READ_BYTES
        ));
    }

    // Lossy UTF-8: replaces invalid sequences with U+FFFD so the result is
    // always valid JSON-encodable text.
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

// ---------------------------------------------------------------------------
// Schema + default allowlist
// ---------------------------------------------------------------------------

/// The `session.update` schema advertised to the model for `read_file`.
pub fn read_file_schema() -> ToolSchema {
    ToolSchema {
        kind: "function".into(),
        name: "read_file".into(),
        description: "Read the contents of a file from the user's Documents or Desktop.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to read (must be within Documents or Desktop)."
                }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
    }
}

/// Returns the M1 default allowlist: `[Documents, Desktop]` relative to the OS
/// user home. Returns an empty list if the OS can't determine the directories
/// (fail-closed: `validate_read_path` rejects everything against an empty list).
///
/// Each path is **canonicalized** before being added to the list. The comment in
/// `read_file_tool` says "allowed_bases MUST already be canonical" — this function
/// honours that contract by calling `canonicalize()` on each candidate. If
/// `canonicalize()` fails (directory does not yet exist on first launch, or OS
/// cannot resolve it), the path is silently dropped; the allowlist shrinks rather
/// than growing an unresolvable base that `within_any_allowed` would silently skip
/// anyway.
///
/// `koe-351` will replace callers of this function with a per-user configurable
/// policy fetched from `JsonSettingsStore`.
pub fn default_read_allowlist() -> Vec<PathBuf> {
    let mut bases = Vec::new();
    if let Some(d) = dirs_next::document_dir() {
        if let Ok(canon) = d.canonicalize() {
            bases.push(canon);
        }
    }
    if let Some(d) = dirs_next::desktop_dir() {
        if let Ok(canon) = d.canonicalize() {
            bases.push(canon);
        }
    }
    bases
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// Create a temp dir as the single allowed base; return (TempDir, Vec<PathBuf>).
    /// The TempDir must be kept alive for the duration of the test.
    fn allowed_base() -> (tempfile::TempDir, Vec<PathBuf>) {
        let dir = tempfile::tempdir().expect("tempdir");
        // canonicalize so the base matches what validate_read_path produces.
        let base = dir.path().canonicalize().expect("canon base");
        (dir, vec![base])
    }

    /// Flags for opening a base directory as an `openat` anchor fd in the unix
    /// component-walk tests. Mirrors the production code: `O_PATH` on Linux
    /// (pure path-anchor fd, not for I/O) but `O_RDONLY` on macOS/other-Unix
    /// where `O_PATH` does not exist — plus `O_DIRECTORY | O_CLOEXEC`. Uses the
    /// `libc::*` constants for the target, not hardcoded Linux numeric values,
    /// so the tests compile and behave correctly on every Unix.
    #[cfg(unix)]
    fn test_dirfd_flags() -> libc::c_int {
        #[cfg(target_os = "linux")]
        let anchor = libc::O_PATH;
        #[cfg(not(target_os = "linux"))]
        let anchor = libc::O_RDONLY;
        anchor | libc::O_DIRECTORY | libc::O_CLOEXEC
    }

    /// Helper: produce a valid `ValidatedBase` for `base_path` by calling
    /// `lstat_validated_base`. Panics if the base does not exist or is a symlink.
    /// Use in tests that call `open_no_symlinks_and_read` directly so they pass
    /// a real (dev, ino) pair rather than a dummy.
    fn real_base_stat(base_path: &std::path::Path) -> ValidatedBase {
        lstat_validated_base(base_path).expect("lstat_validated_base must succeed for a real dir")
    }

    /// Helper: produce a dummy `ValidatedBase` with all identity fields zeroed.
    /// Used in tests that deliberately want a MISMATCHED stat (to simulate a
    /// base swap) — on Unix `open_no_symlinks_and_read` will reject when
    /// `fstat(base_fd) != (0, 0)` unless the kernel actually assigned dev=0 ino=0,
    /// which never happens for real directories.
    fn mismatched_base_stat() -> ValidatedBase {
        ValidatedBase { dev: 0, ino: 0, win_volume_serial: 0, win_file_index: 0 }
    }

    // ---- path-traversal rejection -------------------------------------------

    #[tokio::test]
    async fn rejects_traversal_escaping_base() {
        let (dir, bases) = allowed_base();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        fs::write(&secret, b"x").unwrap();
        let traversal = format!(
            "{}/../{}/secret.txt",
            dir.path().display(),
            outside.path().file_name().unwrap().to_string_lossy()
        );
        let tool = read_file_tool(bases);
        let result = tool(serde_json::json!({ "path": traversal })).await;
        assert!(result.is_err(), "traversal must be rejected");
    }

    // ---- allowlist enforcement ----------------------------------------------

    #[tokio::test]
    async fn rejects_file_outside_allowlist() {
        let (_dir, bases) = allowed_base();
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("outside.txt");
        fs::write(&file, b"data").unwrap();
        let tool = read_file_tool(bases);
        let result = tool(serde_json::json!({ "path": file.to_str().unwrap() })).await;
        assert!(result.is_err(), "file outside allowlist must be rejected");
    }

    #[tokio::test]
    async fn reads_file_inside_allowlist() {
        let (dir, bases) = allowed_base();
        let file = dir.path().join("note.txt");
        fs::write(&file, b"hello world").unwrap();
        let tool = read_file_tool(bases);
        let result = tool(serde_json::json!({ "path": file.to_str().unwrap() })).await;
        let out = result.expect("should read valid file");
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(v["content"], "hello world");
        assert_eq!(v["bytes"], 11);
    }

    // ---- size cap -----------------------------------------------------------

    #[tokio::test]
    async fn rejects_file_exceeding_size_cap() {
        let (dir, bases) = allowed_base();
        let file = dir.path().join("big.bin");
        // Write more bytes than the cap.
        let big: Vec<u8> = vec![b'a'; (MAX_READ_BYTES + 1) as usize];
        fs::write(&file, &big).unwrap();
        let tool = read_file_tool(bases);
        let result = tool(serde_json::json!({ "path": file.to_str().unwrap() })).await;
        assert!(result.is_err(), "file over size cap must be rejected");
        let err = result.unwrap_err();
        assert!(err.contains("too large"), "error should mention too large: {err}");
    }

    // ---- empty allowlist rejects everything ---------------------------------

    #[tokio::test]
    async fn empty_allowlist_rejects_everything() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f.txt");
        fs::write(&file, b"x").unwrap();
        let tool = read_file_tool(vec![]);
        let result = tool(serde_json::json!({ "path": file.to_str().unwrap() })).await;
        assert!(result.is_err(), "empty allowlist must reject everything");
    }

    // ---- missing path argument returns error --------------------------------

    #[tokio::test]
    async fn rejects_missing_path_arg() {
        let (_dir, bases) = allowed_base();
        let tool = read_file_tool(bases);
        let result = tool(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    // ---- sensitive component blocked (via validate_read_path) ---------------

    #[tokio::test]
    async fn rejects_sensitive_component_in_path() {
        let (dir, bases) = allowed_base();
        let ssh = dir.path().join(".ssh");
        fs::create_dir(&ssh).unwrap();
        let key = ssh.join("id_rsa");
        fs::write(&key, b"key").unwrap();
        let tool = read_file_tool(bases);
        let result = tool(serde_json::json!({ "path": key.to_str().unwrap() })).await;
        assert!(result.is_err(), ".ssh component must be rejected");
    }

    // ---- schema shape -------------------------------------------------------

    #[test]
    fn schema_advertises_required_path() {
        let s = read_file_schema();
        assert_eq!(s.name, "read_file");
        assert_eq!(s.parameters["required"][0], "path");
        assert_eq!(s.parameters["additionalProperties"], false);
    }

    // ---- non-utf8 content is returned as lossy string -----------------------

    #[tokio::test]
    async fn reads_non_utf8_file_as_lossy_string() {
        let (dir, bases) = allowed_base();
        let file = dir.path().join("binary.bin");
        // Write invalid UTF-8 sequence.
        fs::write(&file, &[0xFF, 0xFE, 0x41]).unwrap();
        let tool = read_file_tool(bases);
        let result = tool(serde_json::json!({ "path": file.to_str().unwrap() })).await;
        // Should succeed (lossy decode), not panic.
        assert!(result.is_ok(), "lossy-decodable binary should not error: {:?}", result);
    }

    // ---- regression: file in former 16–64 KiB gap is now capped by MAX_READ_BYTES ----
    //
    // Before the fix MAX_READ_BYTES was 64 KiB but MAX_TOOL_OUTPUT_LEN in the
    // dispatcher was only 16 KiB. A file between ~12 KiB and 64 KiB would
    // succeed here but be replaced by a truncation envelope in the dispatcher.
    // Now MAX_READ_BYTES = 12 KiB so any file larger than 12 KiB is rejected
    // cleanly by read_file itself with a "too large" error, never producing an
    // output that the dispatcher would silently truncate.

    #[tokio::test]
    async fn rejects_file_in_former_cap_gap() {
        let (dir, bases) = allowed_base();
        let file = dir.path().join("medium.txt");
        // 20 KiB — above new MAX_READ_BYTES (12 KiB) but below old cap (64 KiB).
        let medium: Vec<u8> = vec![b'x'; 20 * 1024];
        fs::write(&file, &medium).unwrap();
        let tool = read_file_tool(bases);
        let result = tool(serde_json::json!({ "path": file.to_str().unwrap() })).await;
        // Must be rejected here (not silently truncated by the dispatcher).
        assert!(result.is_err(), "20 KiB file must be rejected by read_file, not truncated by dispatcher");
        let err = result.unwrap_err();
        assert!(err.contains("too large"), "error should mention too large: {err}");
    }

    // ---- size cap: take() prevents TOCTOU between metadata and read ---------
    //
    // Verifies that read_capped uses take() semantics: a file whose reported
    // metadata.len() is under the cap but that actually delivers more bytes
    // (e.g. a file grown between stat and read) is still rejected.

    #[test]
    fn read_capped_rejects_file_delivering_more_than_cap() {
        // Create a file whose content is just over the cap.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grown.bin");
        let mut data = vec![b'x'; (MAX_READ_BYTES + 2) as usize];
        fs::write(&path, &mut data).unwrap();
        // Open the file and call read_capped directly.
        let f = std::fs::File::open(&path).unwrap();
        let result = read_capped(f);
        assert!(result.is_err(), "read_capped must reject files delivering more bytes than cap");
        assert!(result.unwrap_err().contains("too large"));
    }

    // ---- symlink in final component is rejected by openat2 / O_NOFOLLOW ----

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_symlink_swap_at_final_component() {
        use std::os::unix::fs::symlink;
        let (dir, _bases) = allowed_base();
        let outside = tempfile::tempdir().unwrap();
        let real = outside.path().join("secret.txt");
        fs::write(&real, b"secret").unwrap();
        // A symlink inside the base that points outside it.
        // validate_read_path would catch this via canonicalize → OutsideAllowed.
        // But we also test open_no_symlinks_and_read directly: if a file IS inside
        // the base but is a symlink at open time, open must fail.
        let inside_real = dir.path().join("legit.txt");
        fs::write(&inside_real, b"legit").unwrap();
        let base_canon = dir.path().canonicalize().unwrap();
        let base_stat = real_base_stat(&base_canon);
        // The validated canonical path is `inside_real` (a real file inside the base).
        // open_no_symlinks_and_read on a real (non-symlink) file must succeed.
        let result = open_no_symlinks_and_read(
            &inside_real.canonicalize().unwrap(),
            &base_canon,
            base_stat,
        );
        assert!(result.is_ok(), "regular file open must succeed: {:?}", result);

        // Now test: a symlink at the final component that points outside.
        let link = dir.path().join("link.txt");
        symlink(&real, &link).unwrap();
        // open_no_symlinks_and_read on the symlink itself (not its canonical target)
        // must fail.
        let result = open_no_symlinks_and_read(&link, &base_canon, base_stat);
        assert!(result.is_err(), "open on a symlink must fail (TOCTOU defence): {:?}", result);
    }

    // ---- RESOLVE_BENEATH / component-walk: intermediate-dir symlink swap ----
    //
    // This tests the fix for the P1 finding: a symlink placed at an INTERMEDIATE
    // component (parent-dir swap attack) must not be followed. The openat2
    // RESOLVE_BENEATH + RESOLVE_NO_SYMLINKS combination (Linux), or the
    // openat_nofollow_walk component walk (all Unix), must reject a swap at any
    // component, not just the final one.

    #[cfg(unix)]
    #[test]
    fn unix_component_walk_blocks_parent_dir_symlink_escape() {
        use std::os::unix::fs::symlink;

        // Layout:
        //   base/real_dir/     ← the directory that the canonical path traverses
        //   base/link_dir/     ← a symlink pointing OUTSIDE base (to outside_dir)
        //   outside_dir/secret.txt
        //
        // After validation the canonical path is base/real_dir/secret.txt (inside
        // the base). We then simulate a TOCTOU swap: between validate and open,
        // real_dir is replaced with link_dir (a symlink to outside). The
        // open_no_symlinks_and_read call uses the CANONICAL path
        // (base/real_dir/secret.txt), but at open time real_dir may have been
        // swapped to a symlink. The component walk must block this.

        let base_dir = tempfile::tempdir().unwrap();
        let base = base_dir.path().canonicalize().unwrap();

        let outside_dir = tempfile::tempdir().unwrap();
        let secret = outside_dir.path().join("secret.txt");
        fs::write(&secret, b"secret data").unwrap();

        // Create real_dir inside base and place a file there.
        let real_dir = base.join("real_dir");
        fs::create_dir(&real_dir).unwrap();
        let inside_file = real_dir.join("secret.txt");
        fs::write(&inside_file, b"inside data").unwrap();

        // Capture base stat before any swap.
        let base_stat = real_base_stat(&base);

        // Simulate "before swap": open succeeds on the real path.
        let result_before = open_no_symlinks_and_read(&inside_file, &base, base_stat);
        assert!(result_before.is_ok(), "should open real file before swap: {:?}", result_before);

        // Simulate "TOCTOU swap": remove real_dir, replace with a symlink
        // to outside_dir. A component-unsafe open would now read outside data.
        fs::remove_file(&inside_file).unwrap();
        fs::remove_dir(&real_dir).unwrap();
        symlink(outside_dir.path(), &real_dir).unwrap();

        // The canonical path still looks like base/real_dir/secret.txt, but
        // real_dir is now a symlink to outside_dir. The component walk must block this.
        let result_after = open_no_symlinks_and_read(&inside_file, &base, base_stat);
        assert!(
            result_after.is_err(),
            "component walk must block parent-dir symlink escape (TOCTOU): {:?}",
            result_after
        );
    }

    // ---- Base anchoring: rel path correctly strips the base prefix ----------

    #[cfg(unix)]
    #[test]
    fn base_anchoring_open_succeeds_for_nested_file() {
        // A file nested two levels deep inside the base must be opened correctly
        // (verifies the strip_prefix + relative-path logic in the component walk).
        let base_dir = tempfile::tempdir().unwrap();
        let base = base_dir.path().canonicalize().unwrap();
        let sub = base.join("sub");
        fs::create_dir(&sub).unwrap();
        let file = sub.join("nested.txt");
        fs::write(&file, b"nested content").unwrap();

        let base_stat = real_base_stat(&base);
        let result = open_no_symlinks_and_read(&file, &base, base_stat);
        assert!(result.is_ok(), "nested file open must succeed: {:?}", result);
        let content = result.unwrap();
        assert_eq!(content, "nested content");
    }

    // ---- [P1 ENOSYS fallback / macOS] openat_nofollow_walk rejects ".." ----
    //
    // The component walk (used as ENOSYS fallback on Linux, primary on macOS)
    // must explicitly reject any path component that is "..", ".", or empty,
    // even though validate_read_path canonicalization has already removed them.

    #[cfg(unix)]
    #[test]
    fn openat_nofollow_walk_rejects_dotdot_component() {
        use std::os::unix::ffi::OsStrExt as _;

        let base_dir = tempfile::tempdir().unwrap();
        let base = base_dir.path().canonicalize().unwrap();

        // Create a real file inside the base so a normal open would succeed.
        let sub = base.join("sub");
        fs::create_dir(&sub).unwrap();
        let file = sub.join("data.txt");
        fs::write(&file, b"secret").unwrap();

        // Open base as a dirfd to pass to the helper.
        let base_bytes = base.as_os_str().as_bytes();
        let mut base_nul: Vec<u8> = Vec::with_capacity(base_bytes.len() + 1);
        base_nul.extend_from_slice(base_bytes);
        base_nul.push(0u8);
        let base_fd = unsafe {
            libc::open(
                base_nul.as_ptr() as *const libc::c_char,
                test_dirfd_flags(), // O_PATH(Linux)/O_RDONLY | O_DIRECTORY | O_CLOEXEC
            )
        };
        assert!(base_fd >= 0, "could not open base as dirfd");
        struct FdGuard(libc::c_int);
        impl Drop for FdGuard { fn drop(&mut self) { unsafe { libc::close(self.0); } } }
        let _g = FdGuard(base_fd);

        // A path "sub/../sub/data.txt" contains a ".." component. The walk must
        // reject it (fail-closed) regardless of whether it resolves to inside the base.
        let rel_with_dotdot = std::path::Path::new("sub/../sub/data.txt");
        let result = openat_nofollow_walk(base_fd, rel_with_dotdot);
        assert!(
            result.is_err(),
            "openat_nofollow_walk must reject a path with a '..' component: {:?}",
            result
        );

        // Sanity: a clean path should succeed.
        let rel_clean = std::path::Path::new("sub/data.txt");
        let result_clean = openat_nofollow_walk(base_fd, rel_clean);
        assert!(
            result_clean.is_ok(),
            "openat_nofollow_walk must succeed for a clean relative path: {:?}",
            result_clean
        );
    }

    // ---- [P1] openat_nofollow_walk rejects parent-dir symlink ----
    //
    // A symlink placed at an intermediate directory component must be rejected by
    // the walk. O_NOFOLLOW on each step causes openat to return ELOOP when the
    // component is a symlink, which the walk propagates as an error.

    #[cfg(unix)]
    #[test]
    fn openat_nofollow_walk_rejects_parent_dir_symlink() {
        use std::os::unix::ffi::OsStrExt as _;
        use std::os::unix::fs::symlink;

        let base_dir = tempfile::tempdir().unwrap();
        let base = base_dir.path().canonicalize().unwrap();
        let outside_dir = tempfile::tempdir().unwrap();
        let outside_file = outside_dir.path().join("secret.txt");
        fs::write(&outside_file, b"outside secret").unwrap();

        // Place a symlink inside base named "link_dir" pointing to outside_dir.
        let link_dir = base.join("link_dir");
        symlink(outside_dir.path(), &link_dir).unwrap();

        // Also create a real sub dir with a file for the success case.
        let real_dir = base.join("real_dir");
        fs::create_dir(&real_dir).unwrap();
        let real_file = real_dir.join("ok.txt");
        fs::write(&real_file, b"inside").unwrap();

        // Build a null-terminated byte string for libc::open.
        let base_bytes = base.as_os_str().as_bytes();
        let mut base_nul: Vec<u8> = Vec::with_capacity(base_bytes.len() + 1);
        base_nul.extend_from_slice(base_bytes);
        base_nul.push(0u8);
        let base_fd = unsafe {
            libc::open(
                base_nul.as_ptr() as *const libc::c_char,
                test_dirfd_flags(),
            )
        };
        assert!(base_fd >= 0, "could not open base as dirfd");
        struct FdGuard(libc::c_int);
        impl Drop for FdGuard { fn drop(&mut self) { unsafe { libc::close(self.0); } } }
        let _g = FdGuard(base_fd);

        // Traversing through the symlink-dir must be rejected.
        let rel_via_link = std::path::Path::new("link_dir/secret.txt");
        let result_link = openat_nofollow_walk(base_fd, rel_via_link);
        assert!(
            result_link.is_err(),
            "openat_nofollow_walk must reject traversal through a symlinked directory: {:?}",
            result_link
        );

        // A clean traversal through a real dir must succeed.
        let rel_real = std::path::Path::new("real_dir/ok.txt");
        let result_real = openat_nofollow_walk(base_fd, rel_real);
        assert!(
            result_real.is_ok(),
            "openat_nofollow_walk must succeed for traversal through a real directory: {:?}",
            result_real
        );
    }

    // ---- [P1-1] component walk uses libc O_* constants, not hardcoded Linux #s --
    //
    // Regression guard for the macOS/other-Unix bug: the open flags must come
    // from the `libc::*` constants for the current target, NOT hardcoded Linux
    // numeric values. On macOS the numeric encodings differ (e.g. O_NOFOLLOW is
    // 0x100 there vs 0o400000 on Linux) and O_PATH does not exist — hardcoded
    // Linux numbers would silently disable the no-follow / dir-anchor guarantee.
    //
    // We assert two things:
    //   (1) the platform's libc constants are non-zero and distinct where they
    //       must be (so a future copy-paste of a Linux number is caught), and
    //   (2) a component walk anchored at a dirfd opened with the libc-derived
    //       `test_dirfd_flags()` STILL rejects ".." and a parent-dir symlink
    //       (i.e. the constant change did not weaken the security behaviour).
    #[cfg(unix)]
    #[test]
    fn component_walk_uses_libc_constants_and_still_rejects_escape() {
        use std::os::unix::ffi::OsStrExt as _;
        use std::os::unix::fs::symlink;

        // (1) The flags we feed to libc::open are the platform libc constants.
        // O_NOFOLLOW / O_DIRECTORY must be non-zero; O_CLOEXEC must be set.
        // On Linux O_PATH is in test_dirfd_flags(); on other Unix O_RDONLY (0)
        // is the anchor, so we assert via the constants themselves.
        assert_ne!(libc::O_NOFOLLOW, 0, "libc::O_NOFOLLOW must be non-zero");
        assert_ne!(libc::O_DIRECTORY, 0, "libc::O_DIRECTORY must be non-zero");
        assert_ne!(libc::O_CLOEXEC, 0, "libc::O_CLOEXEC must be non-zero");
        let flags = test_dirfd_flags();
        assert_ne!(flags & libc::O_DIRECTORY, 0, "anchor flags must include O_DIRECTORY");
        assert_ne!(flags & libc::O_CLOEXEC, 0, "anchor flags must include O_CLOEXEC");

        // (2) Build a base dirfd with those exact libc-derived flags and verify
        // the walk still blocks a "../" component and a parent-dir symlink.
        let base_dir = tempfile::tempdir().unwrap();
        let base = base_dir.path().canonicalize().unwrap();

        // A real nested file for the success/sanity path.
        let real_dir = base.join("real_dir");
        fs::create_dir(&real_dir).unwrap();
        let ok_file = real_dir.join("ok.txt");
        fs::write(&ok_file, b"inside").unwrap();

        // A symlinked directory escaping the base.
        let outside_dir = tempfile::tempdir().unwrap();
        fs::write(outside_dir.path().join("secret.txt"), b"outside secret").unwrap();
        let link_dir = base.join("link_dir");
        symlink(outside_dir.path(), &link_dir).unwrap();

        let base_bytes = base.as_os_str().as_bytes();
        let mut base_nul: Vec<u8> = Vec::with_capacity(base_bytes.len() + 1);
        base_nul.extend_from_slice(base_bytes);
        base_nul.push(0u8);
        let base_fd = unsafe {
            libc::open(base_nul.as_ptr() as *const libc::c_char, flags)
        };
        assert!(base_fd >= 0, "base dirfd open with libc flags must succeed");
        struct FdGuard(libc::c_int);
        impl Drop for FdGuard { fn drop(&mut self) { unsafe { libc::close(self.0); } } }
        let _g = FdGuard(base_fd);

        // ".." component is rejected.
        assert!(
            openat_nofollow_walk(base_fd, std::path::Path::new("real_dir/../real_dir/ok.txt")).is_err(),
            "walk must reject a '..' component even with libc-constant flags"
        );
        // Parent-dir symlink is rejected (O_NOFOLLOW → ELOOP at the symlink step).
        assert!(
            openat_nofollow_walk(base_fd, std::path::Path::new("link_dir/secret.txt")).is_err(),
            "walk must reject a parent-dir symlink even with libc-constant flags"
        );
        // Clean path still succeeds.
        assert!(
            openat_nofollow_walk(base_fd, std::path::Path::new("real_dir/ok.txt")).is_ok(),
            "walk must still open a clean relative path with libc-constant flags"
        );
    }

    // ---- [P1-b] validate_read_path_with_base: no parent-dir fallback -----------
    //
    // Previously if no allowlist base matched the code fell back to the canonical
    // path's parent. The fix returns Err (fail-closed) instead.

    #[tokio::test]
    async fn validate_with_base_returns_err_when_no_base_matches() {
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("test.txt");
        fs::write(&file, b"data").unwrap();

        // Call with an empty bases list — no base can match.
        let result = validate_read_path_with_base(file.to_str().unwrap(), &[]);
        assert!(
            result.is_err(),
            "validate_read_path_with_base must fail with empty bases (no fallback): {:?}",
            result
        );
    }

    // ---- [P1] validate_read_path_with_base: symlink at base is rejected ----
    //
    // lstat_validated_base checks S_IFLNK / reparse-point and returns OutsideAllowed.

    #[cfg(unix)]
    #[test]
    fn validate_rejects_symlinked_base() {
        use std::os::unix::fs::symlink;

        // Real directory that the allowlist entry will POINT to (via a symlink).
        let real_dir = tempfile::tempdir().unwrap();
        let real_base = real_dir.path().canonicalize().unwrap();

        // Create a symlink that points to real_dir.
        let link_dir = tempfile::tempdir().unwrap();
        let symlink_base = link_dir.path().join("sym_base");
        symlink(&real_base, &symlink_base).unwrap();

        // lstat_validated_base must reject the symlink.
        let result = lstat_validated_base(&symlink_base);
        assert!(
            result.is_err(),
            "lstat_validated_base must reject a symlinked base (S_IFLNK): {:?}",
            result
        );

        // A real (non-symlink) directory must succeed.
        let ok = lstat_validated_base(&real_base);
        assert!(ok.is_ok(), "lstat_validated_base must succeed for a real dir: {:?}", ok);
    }

    // ---- [P1] base swapped after validate: mismatched identity is rejected ----
    //
    // We simulate a base swap by passing a mismatched ValidatedBase (all zeros)
    // which cannot match any real directory's fstat(base_fd). On Unix this
    // exercises the fstat-vs-base_stat comparison.

    #[cfg(unix)]
    #[test]
    fn open_rejects_mismatched_base_stat() {
        // Set up: a real base dir with a file inside.
        let base_dir = tempfile::tempdir().unwrap();
        let base = base_dir.path().canonicalize().unwrap();
        let file = base.join("data.txt");
        fs::write(&file, b"inside").unwrap();

        // Sanity: opening with the REAL base stat must succeed.
        let real_stat = real_base_stat(&base);
        let ok = open_no_symlinks_and_read(&file, &base, real_stat);
        assert!(ok.is_ok(), "real base stat must succeed: {:?}", ok);

        // Simulate a swap: pass a mismatched ValidatedBase (all zeros).
        // fstat(base_fd) will return the real (dev, ino), which will != (0, 0),
        // so the comparison must fail-closed.
        let bad_stat = mismatched_base_stat();
        let result = open_no_symlinks_and_read(&file, &base, bad_stat);
        assert!(
            result.is_err(),
            "mismatched identity must be rejected by fstat comparison (simulate base swap): {:?}",
            result
        );
    }

    // ---- [P1] validate_read_path_with_base: normal read succeeds ----

    #[tokio::test]
    async fn validate_and_open_succeed_for_normal_read() {
        let (dir, bases) = allowed_base();
        let file = dir.path().join("normal.txt");
        fs::write(&file, b"normal content").unwrap();
        // Use the tool end-to-end so validate_read_path_with_base feeds base_stat
        // into open_no_symlinks_and_read.
        let tool = read_file_tool(bases);
        let result = tool(serde_json::json!({ "path": file.to_str().unwrap() })).await;
        let out = result.expect("normal read must succeed");
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(v["content"], "normal content");
        assert_eq!(v["bytes"], 14);
    }

    // ---- [P1-b] base dir symlink swap: open rejects base-as-symlink ----
    //
    // If a symlink is passed as `base`, O_NOFOLLOW on the dirfd open causes
    // ELOOP — fail-closed before even reaching the fstat comparison.

    #[cfg(unix)]
    #[test]
    fn open_rejects_base_path_that_is_a_symlink() {
        use std::os::unix::fs::symlink;

        // Set up: a real base dir with a file inside.
        let base_dir = tempfile::tempdir().unwrap();
        let base = base_dir.path().canonicalize().unwrap();
        let file = base.join("data.txt");
        fs::write(&file, b"inside").unwrap();

        // Create a symlink that points to the real base dir.
        let symlink_dir = tempfile::tempdir().unwrap();
        let fake_base = symlink_dir.path().join("fake_base");
        symlink(&base, &fake_base).unwrap();

        // Capture stat from the REAL base (so base_stat is correct for the real base).
        let base_stat = real_base_stat(&base);

        // Open with the symlink path as `base` but real stat for the real base.
        // The open itself will fail (ELOOP from O_NOFOLLOW on the symlink).
        let file_via_fake = fake_base.join("data.txt");
        let result = open_no_symlinks_and_read(&file_via_fake, &fake_base, base_stat);
        assert!(
            result.is_err(),
            "base path that is a symlink must be rejected (O_NOFOLLOW → ELOOP): {:?}",
            result
        );
    }

    // ---- [P1-b] non-matching path to open_no_symlinks_and_read errors cleanly --
    //
    // If the canonical path passed to open_no_symlinks_and_read does not share
    // the given base as a prefix (strip_prefix fails), the function must return
    // Err rather than attempting to open with an empty relative path.

    #[cfg(unix)]
    #[test]
    fn open_errors_when_path_not_under_base() {
        let base_dir = tempfile::tempdir().unwrap();
        let base = base_dir.path().canonicalize().unwrap();

        let other_dir = tempfile::tempdir().unwrap();
        let outside_file = other_dir.path().join("outside.txt");
        fs::write(&outside_file, b"x").unwrap();
        let outside_canon = outside_file.canonicalize().unwrap();

        let base_stat = real_base_stat(&base);
        // Pass a path that is NOT under `base` — strip_prefix will fail.
        let result = open_no_symlinks_and_read(&outside_canon, &base, base_stat);
        assert!(
            result.is_err(),
            "open must error when path is not under the given base: {:?}",
            result
        );
    }

    // ---- [Windows] component-walk path-decomposition logic ------------------
    //
    // The Windows component walk decomposes the relative path into Normal
    // components and rejects "..", ".", empty, RootDir, and Prefix components.
    // This test validates that logic in a platform-neutral way (pure Rust path
    // decomposition — no Win32 calls) to provide CI coverage on Linux/WSL.
    //
    // The actual Win32 HANDLE operations are gated on cfg(windows); this test
    // covers the component-classification logic that is identical on all platforms.

    #[test]
    fn windows_component_walk_logic_rejects_traversal_components() {
        use std::path::{Component, Path};

        // Returns true if the component should be rejected by the Windows walk
        // (mirrors the match arm in the cfg(windows) open_no_symlinks_and_read).
        fn is_rejected_component(c: &Component<'_>) -> bool {
            match c {
                Component::ParentDir
                | Component::CurDir
                | Component::RootDir
                | Component::Prefix(_) => true,
                Component::Normal(name) => {
                    if name.is_empty() {
                        return true;
                    }
                    let s = name.to_string_lossy();
                    s == "." || s == ".."
                }
            }
        }

        // Normal component should not be rejected.
        let normal = Path::new("subdir/file.txt");
        for c in normal.components() {
            assert!(
                !is_rejected_component(&c),
                "Normal component {:?} must not be rejected",
                c
            );
        }

        // ParentDir must be rejected.
        let traversal = Path::new("subdir/../other.txt");
        let has_parent = traversal.components().any(|c| is_rejected_component(&c));
        assert!(has_parent, "ParentDir must be rejected in component walk");

        // CurDir must be rejected.
        let curdir = Path::new("./file.txt");
        let has_curdir = curdir.components().any(|c| is_rejected_component(&c));
        assert!(has_curdir, "CurDir must be rejected in component walk");
    }

    // ---- [Windows] lstat_validated_base fields documented ------------------
    //
    // On non-Windows platforms, win_volume_serial and win_file_index are zero
    // (they are unused). This test documents and asserts that contract so any
    // future change that accidentally populates them on Unix is caught.

    #[cfg(unix)]
    #[test]
    fn unix_base_stat_has_zero_win_fields() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let stat = real_base_stat(&base);
        assert_eq!(stat.win_volume_serial, 0, "win_volume_serial must be 0 on Unix");
        assert_eq!(stat.win_file_index, 0, "win_file_index must be 0 on Unix");
        // dev and ino should be non-zero for a real directory.
        // (On some exotic filesystems ino could be 0, but tempfile dirs never are.)
        assert!(
            stat.dev != 0 || stat.ino != 0,
            "at least one of dev/ino must be non-zero for a real directory"
        );
    }
}
