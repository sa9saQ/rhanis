//! User-configurable permission policy (rhanis-351).
//!
//! Layers a user-defined allow/deny policy ON TOP of the built-in three-tier
//! risk gate (`approval_gate::classify` → SAFE/CAUTION/DANGER). The policy only
//! changes the **approval decision** (run now / human gate); it never replaces
//! the real-IO defenses (`validation.rs` openat2/O_NOFOLLOW at execution,
//! `tool_dispatcher` shell DENY/ALLOW list). Defense in depth.
//!
//! # Safety model (the whole point — fail-closed)
//! Priority is **DENY > ALLOW > DEFAULT**, and anything undecidable (an
//! unresolvable path, a non-http URL, a relative path, a load failure) falls to
//! the deny side (`RequireApproval`). The built-in sensitive baseline
//! (`validation::contains_sensitive` — `.ssh`/`.env`/system dirs/…) is checked
//! BEFORE the allowlist, so a user cannot allow-list their way into a protected
//! location: the baseline always wins.
//!
//! [`evaluate`] / [`decide`] are pure (given the filesystem): they read no global
//! state and never log/emit the path or URL. The only output is a
//! [`PolicyDecision`] enum the dispatcher (rhanis-2gy) composes with the risk tier.
//!
//! transaction N/A · idempotency_key N/A (read-only approval policy, not billing).

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::{Host, Url};

use crate::approval_gate::ApprovalRisk;
use crate::validation::{contains_sensitive, is_within, resolve_for_boundary};

/// Defensive bounds for a (possibly hand-edited / tampered) policy file. The UI
/// keeps real input well under these; the caps exist so a malicious settings
/// file cannot blow up memory or iteration cost. Enforced by
/// [`validate_permission_policy`] (settings load + the `set_permission_policy`
/// command), so the evaluator never sees an unbounded list.
const MAX_ENTRIES: usize = 256;
/// Max length of a single folder-path entry (mirrors `validation::MAX_PATH_LENGTH`).
const MAX_PATH_LEN: usize = 4096;
/// Max length of a single host entry (a DNS name is ≤ 253 octets).
const MAX_HOST_LEN: usize = 253;

// ---------------------------------------------------------------------------
// Persisted policy (serde — part of settings_store::AppSettings)
// ---------------------------------------------------------------------------

/// The user's permission policy. Persisted inside `AppSettings`
/// (`settings_store.rs`) as JSON. Every field defaults to empty/false, and an
/// **empty policy auto-approves nothing** (the safe default): folders fall to the
/// existing tier behaviour, URLs are gated (strict default).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionPolicy {
    /// Folders whose contents are auto-approved (green light). Each carries a
    /// per-folder DANGER opt-in (see [`AllowedFolder`]).
    #[serde(default)]
    pub allowed_folders: Vec<AllowedFolder>,
    /// Folders that must NEVER auto-run: an operation whose target resolves
    /// inside one of these always requires a human decision (禁止 — wins over
    /// any allow). Stored as raw path strings; canonicalized at evaluation.
    #[serde(default)]
    pub denied_folders: Vec<String>,
    /// URL hosts whose `open_url` is auto-approved (suffix + dot-boundary match).
    #[serde(default)]
    pub allowed_url_hosts: Vec<String>,
    /// URL hosts that must always be confirmed (禁止 — wins over allow and over
    /// [`allow_all_urls`](PermissionPolicy::allow_all_urls)).
    #[serde(default)]
    pub denied_url_hosts: Vec<String>,
    /// Explicit opt-in: auto-approve `open_url` for ANY http/https host (except a
    /// denied one). Off by default — the strict URL default confirms unlisted
    /// hosts until the user either allow-lists them or flips this on.
    #[serde(default)]
    pub allow_all_urls: bool,
}

/// One allowed folder plus whether DANGER operations (delete/…) inside it are
/// also auto-approved. `allow_danger` defaults to `false`: even inside an allowed
/// folder, a DANGER op keeps the human gate unless the user explicitly opted that
/// folder in (the 2026-05-30 user decision Q2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowedFolder {
    pub path: String,
    #[serde(default)]
    pub allow_danger: bool,
}

// ---------------------------------------------------------------------------
// Decision + provider state
// ---------------------------------------------------------------------------

/// The policy layer's verdict for one tool call, composed with the risk tier by
/// the dispatcher: `AutoApprove` → skip the gate; `Default` → keep the built-in
/// behaviour (DANGER gates, SAFE/CAUTION run); `RequireApproval` → force the
/// human gate even for a tier that would otherwise run immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDecision {
    AutoApprove,
    Default,
    RequireApproval,
}

/// The policy as seen by the dispatcher at call time. `Unavailable` means the
/// settings file could not be loaded (corrupt/unreadable); in that state we must
/// NOT drop the user's deny protections, so any policy-relevant target is forced
/// to `RequireApproval` (fail-closed) — distinct from a successfully-loaded but
/// empty policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyState {
    Loaded(PermissionPolicy),
    Unavailable,
}

/// Supplies the current policy to the dispatcher. The real implementation
/// (`settings_store::SettingsPolicyProvider`) reads the settings file on each
/// dispatch so a UI edit takes effect immediately; a load failure maps to
/// [`PolicyState::Unavailable`] (fail-closed).
pub trait PolicyProvider: Send + Sync {
    fn current_policy(&self) -> PolicyState;
}

// ---------------------------------------------------------------------------
// Target extraction
// ---------------------------------------------------------------------------

/// What the policy reasons about for a given tool call.
///
/// `pub(crate)`: display_descriptor's parity test (rhanis-whf) locks its tool →
/// arg-key map to this one, so the modal can never show a different string
/// than the policy judges.
pub(crate) enum PolicyTarget<'a> {
    /// A filesystem path the folder allow/deny lists apply to.
    Path(&'a str),
    /// A URL the host allow/deny lists apply to.
    Url(&'a str),
    /// Not policy-governed (no path/url, or a tool we never auto-approve).
    None,
}

/// Maps a tool call to its policy target. Tools not listed (including
/// `external_upload`, `run_command`, `open_app`, `web_search`, `write_note`,
/// `take_screenshot`) return `None` → `Default` (existing tier behaviour):
///
/// - `external_upload` (DANGER exfiltration) is deliberately NEVER auto-approved
///   by any allow-list, so it is excluded here and always falls to the DANGER
///   gate. The most dangerous send path must stay human-confirmed.
/// - `run_command` is governed by its own shell DENY/ALLOW list + the DANGER gate
///   (unchanged by this layer), so it is not a folder/url target.
pub(crate) fn policy_target<'a>(tool: &str, args: &'a Value) -> PolicyTarget<'a> {
    match tool {
        "read_file" | "write_file" | "delete_file" => match args.get("path").and_then(Value::as_str)
        {
            Some(p) => PolicyTarget::Path(p),
            None => PolicyTarget::None,
        },
        "open_url" => match args.get("url").and_then(Value::as_str) {
            Some(u) => PolicyTarget::Url(u),
            None => PolicyTarget::None,
        },
        _ => PolicyTarget::None,
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Composes the policy state with a tool call into a [`PolicyDecision`]. This is
/// the single entry the dispatcher calls.
///
/// `Unavailable` (settings load failed) forces any policy-relevant target to
/// `RequireApproval` so a transient load failure cannot silently drop the user's
/// deny list; a target-free tool still falls to `Default` (it is not governed by
/// the policy and is already covered by its tier / shell list).
pub fn decide(state: &PolicyState, tool: &str, risk: ApprovalRisk, args: &Value) -> PolicyDecision {
    let target = policy_target(tool, args);
    match state {
        PolicyState::Unavailable => match target {
            PolicyTarget::None => PolicyDecision::Default,
            PolicyTarget::Path(_) | PolicyTarget::Url(_) => PolicyDecision::RequireApproval,
        },
        PolicyState::Loaded(policy) => match target {
            PolicyTarget::None => PolicyDecision::Default,
            PolicyTarget::Path(raw) => evaluate_path(policy, risk, raw),
            PolicyTarget::Url(raw) => evaluate_url(policy, raw),
        },
    }
}

/// Pure evaluation against a loaded policy (test entry point). `decide` wraps
/// this with the `Unavailable` handling.
#[cfg(test)]
pub fn evaluate_policy(
    policy: &PermissionPolicy,
    tool: &str,
    risk: ApprovalRisk,
    args: &Value,
) -> PolicyDecision {
    decide(&PolicyState::Loaded(policy.clone()), tool, risk, args)
}

// ---------------------------------------------------------------------------
// Path evaluation
// ---------------------------------------------------------------------------

fn evaluate_path(policy: &PermissionPolicy, risk: ApprovalRisk, raw: &str) -> PolicyDecision {
    // A relative path would resolve against the process CWD — an ambiguous,
    // attacker-influenceable base. Refuse to auto-approve it (fail-closed).
    if !Path::new(raw).is_absolute() {
        return PolicyDecision::RequireApproval;
    }
    // Resolve `..`/symlinks. None = unresolvable OR a symlink leaf (dangling) →
    // we cannot prove where it lands, so confirm. (Real IO is separately
    // O_NOFOLLOW-guarded; this is the approval-relaxation gate only.)
    let canon = match resolve_for_boundary(raw) {
        Some(c) => c,
        None => return PolicyDecision::RequireApproval,
    };

    // Built-in baseline — NOT overridable by the allowlist. Always wins.
    if contains_sensitive(&canon) {
        return PolicyDecision::RequireApproval;
    }
    // 禁止 (user deny) — wins over allow.
    if policy.denied_folders.iter().any(|d| folder_contains(d, &canon)) {
        return PolicyDecision::RequireApproval;
    }
    // 許可 (user allow).
    let in_allowed = policy
        .allowed_folders
        .iter()
        .any(|f| folder_contains(&f.path, &canon));
    if !in_allowed {
        return PolicyDecision::Default;
    }
    if risk == ApprovalRisk::Danger {
        // DANGER inside an allowed folder auto-runs ONLY if at least one allowed
        // folder containing the target carries the explicit per-folder opt-in.
        let danger_optin = policy
            .allowed_folders
            .iter()
            .any(|f| f.allow_danger && folder_contains(&f.path, &canon));
        if danger_optin {
            PolicyDecision::AutoApprove
        } else {
            PolicyDecision::Default // keep the DANGER human gate
        }
    } else {
        PolicyDecision::AutoApprove
    }
}

/// True if `canon` (an already-canonicalized target) is `base` itself or inside
/// it. `base` is a raw policy string: empty → no match; unresolvable → no match
/// (fail-closed — an unresolvable allow never grants, and an unresolvable deny
/// cannot contain a real target anyway, since a target can only exist inside a
/// directory that itself resolves).
fn folder_contains(base: &str, canon: &Path) -> bool {
    let base = base.trim();
    // A policy folder boundary must be an ABSOLUTE path the user chose. A relative
    // entry would canonicalize against the process CWD — never a trustworthy
    // boundary — so it can never grant an allow. Fail-closed (R-C[P2]).
    if base.is_empty() || !Path::new(base).is_absolute() {
        return false;
    }
    match Path::new(base).canonicalize() {
        Ok(base_canon) => is_within(canon, &base_canon),
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// URL evaluation
// ---------------------------------------------------------------------------

fn evaluate_url(policy: &PermissionPolicy, raw: &str) -> PolicyDecision {
    // Parse + require an http/https host. Anything else (file:, javascript:,
    // unparseable, hostless) → confirm (fail-closed).
    let host = match url_host(raw) {
        Some(h) => h,
        None => return PolicyDecision::RequireApproval,
    };
    // 禁止 (user deny) — wins over allow AND over allow_all_urls.
    if policy
        .denied_url_hosts
        .iter()
        .any(|d| host_entry(d).is_some_and(|e| host_matches(&host, &e)))
    {
        return PolicyDecision::RequireApproval;
    }
    // 許可: an explicit allow-list match, or the global "all URLs" opt-in.
    let allowed = policy.allow_all_urls
        || policy
            .allowed_url_hosts
            .iter()
            .any(|a| host_entry(a).is_some_and(|e| host_matches(&host, &e)));
    if allowed {
        PolicyDecision::AutoApprove
    } else {
        // Strict default: an unlisted host is confirmed (the "全URL許可" toggle is
        // the one-click escape hatch).
        PolicyDecision::RequireApproval
    }
}

/// Parses `raw` and returns its host ONLY if the scheme is http/https. The `url`
/// crate applies IDNA (so an IDN host is punycode/ASCII) and parses the authority
/// correctly, so `https://openai.com@evil.com/` yields host `evil.com` (the
/// userinfo cannot spoof the host).
///
/// `pub(crate)`: display_descriptor (rhanis-whf) derives the human-shown host
/// through this SAME parser, so the modal displays exactly what the policy judges.
pub(crate) fn url_host(raw: &str) -> Option<Host<String>> {
    let u = Url::parse(raw).ok()?;
    match u.scheme() {
        "http" | "https" => {}
        _ => return None,
    }
    u.host().map(|h| h.to_owned())
}

/// Parses a policy host ENTRY into a `Host`, applying the SAME IDNA/normalization
/// the candidate went through, so both sides compare in one canonical form.
/// Rejects structurally-invalid entries (slash/userinfo/port/whitespace) so a
/// malformed deny entry cannot silently no-op.
fn host_entry(entry: &str) -> Option<Host<String>> {
    let e = entry.trim().trim_end_matches('.');
    if e.is_empty() || e.contains('/') || e.contains('@') || e.chars().any(char::is_whitespace) {
        return None;
    }
    Host::parse(e).ok()
}

/// Host match: domains match on exact equality OR a dot-bounded suffix
/// (`api.openai.com` matches `openai.com`, but `evil-openai.com` does NOT). IPs
/// match only on exact equality. Both sides are already IDNA/lowercased by the
/// `url` crate; the extra lowercase is belt-and-suspenders.
fn host_matches(candidate: &Host<String>, entry: &Host<String>) -> bool {
    match (candidate, entry) {
        (Host::Domain(c), Host::Domain(a)) => {
            let c = c.trim_end_matches('.').to_ascii_lowercase();
            let a = a.trim_end_matches('.').to_ascii_lowercase();
            !a.is_empty() && (c == a || c.ends_with(&format!(".{a}")))
        }
        (Host::Ipv4(c), Host::Ipv4(a)) => c == a,
        (Host::Ipv6(c), Host::Ipv6(a)) => c == a,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Validation (settings load + set_permission_policy command)
// ---------------------------------------------------------------------------

/// Validates a policy's bounds + host well-formedness. Fixed, leak-free error
/// strings (no path/host echoed). Called on the settings READ path (→ `Corrupt`,
/// fail-closed on a tampered file) and by the `set_permission_policy` command
/// (→ user-facing rejection). Empty path/host entries are allowed (harmless
/// no-ops at evaluation); only an over-cap list or a malformed (non-empty) host
/// is rejected.
pub fn validate_permission_policy(p: &PermissionPolicy) -> Result<(), &'static str> {
    if p.allowed_folders.len() > MAX_ENTRIES
        || p.denied_folders.len() > MAX_ENTRIES
        || p.allowed_url_hosts.len() > MAX_ENTRIES
        || p.denied_url_hosts.len() > MAX_ENTRIES
    {
        return Err("permission policy has too many entries");
    }
    for f in &p.allowed_folders {
        if f.path.len() > MAX_PATH_LEN {
            return Err("permission policy path is too long");
        }
    }
    for d in &p.denied_folders {
        if d.len() > MAX_PATH_LEN {
            return Err("permission policy path is too long");
        }
    }
    for h in p.allowed_url_hosts.iter().chain(p.denied_url_hosts.iter()) {
        if h.len() > MAX_HOST_LEN {
            return Err("permission policy host is too long");
        }
        // A non-empty host must be structurally valid, so a malformed deny entry
        // can't no-op and let an allow win. Empty entries are tolerated no-ops.
        if !h.trim().is_empty() && host_entry(h).is_none() {
            return Err("permission policy host is invalid");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn args_path(p: &str) -> Value {
        serde_json::json!({ "path": p })
    }
    fn args_url(u: &str) -> Value {
        serde_json::json!({ "url": u })
    }

    /// A canonicalized temp dir used as an allowed/denied base.
    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }
    fn canon(p: &Path) -> PathBuf {
        p.canonicalize().expect("canon")
    }

    fn allow_folder(path: &Path, allow_danger: bool) -> AllowedFolder {
        AllowedFolder {
            path: canon(path).to_string_lossy().into_owned(),
            allow_danger,
        }
    }

    // ---- baseline always wins (allowlist cannot unlock protected dirs) -------

    #[test]
    fn baseline_sensitive_overrides_allow() {
        for sensitive in [".ssh", ".env", ".env.local", ".aws", ".git"] {
            let dir = tempdir();
            let sub = dir.path().join(sensitive);
            fs::create_dir(&sub).unwrap();
            let file = sub.join("secret");
            fs::write(&file, b"x").unwrap();
            // The whole temp dir is allow-listed WITH danger opt-in …
            let policy = PermissionPolicy {
                allowed_folders: vec![allow_folder(dir.path(), true)],
                ..Default::default()
            };
            // … yet a file under the sensitive component is still gated.
            assert_eq!(
                evaluate_policy(&policy, "read_file", ApprovalRisk::Safe, &args_path(file.to_str().unwrap())),
                PolicyDecision::RequireApproval,
                "{sensitive} must stay protected even inside an allowed+danger folder"
            );
        }
    }

    // ---- deny > allow --------------------------------------------------------

    #[test]
    fn deny_wins_over_allow() {
        let dir = tempdir();
        let file = dir.path().join("note.txt");
        fs::write(&file, b"x").unwrap();
        // Same folder appears in BOTH allow and deny → deny wins.
        let policy = PermissionPolicy {
            allowed_folders: vec![allow_folder(dir.path(), false)],
            denied_folders: vec![canon(dir.path()).to_string_lossy().into_owned()],
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "read_file", ApprovalRisk::Safe, &args_path(file.to_str().unwrap())),
            PolicyDecision::RequireApproval
        );
    }

    // ---- allow: non-danger auto-approves; danger needs opt-in ----------------

    #[test]
    fn allowed_folder_non_danger_auto_approves() {
        let dir = tempdir();
        let file = dir.path().join("note.txt");
        fs::write(&file, b"x").unwrap();
        let policy = PermissionPolicy {
            allowed_folders: vec![allow_folder(dir.path(), false)],
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "read_file", ApprovalRisk::Safe, &args_path(file.to_str().unwrap())),
            PolicyDecision::AutoApprove
        );
    }

    #[test]
    fn allowed_folder_danger_without_optin_stays_default() {
        let dir = tempdir();
        let file = dir.path().join("doomed.txt");
        fs::write(&file, b"x").unwrap();
        let policy = PermissionPolicy {
            allowed_folders: vec![allow_folder(dir.path(), false)],
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "delete_file", ApprovalRisk::Danger, &args_path(file.to_str().unwrap())),
            PolicyDecision::Default,
            "DANGER in an allowed folder keeps the gate unless opted in"
        );
    }

    #[test]
    fn allowed_folder_danger_with_optin_auto_approves() {
        let dir = tempdir();
        let file = dir.path().join("doomed.txt");
        fs::write(&file, b"x").unwrap();
        let policy = PermissionPolicy {
            allowed_folders: vec![allow_folder(dir.path(), true)],
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "delete_file", ApprovalRisk::Danger, &args_path(file.to_str().unwrap())),
            PolicyDecision::AutoApprove
        );
    }

    #[test]
    fn danger_optin_any_overlapping_folder_grants() {
        // A broad non-opt-in folder AND a narrow opt-in subfolder both contain the
        // target → the explicit narrow opt-in grants the danger auto-approve.
        let dir = tempdir();
        let sub = dir.path().join("scratch");
        fs::create_dir(&sub).unwrap();
        let file = sub.join("doomed.txt");
        fs::write(&file, b"x").unwrap();
        let policy = PermissionPolicy {
            allowed_folders: vec![allow_folder(dir.path(), false), allow_folder(&sub, true)],
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "delete_file", ApprovalRisk::Danger, &args_path(file.to_str().unwrap())),
            PolicyDecision::AutoApprove
        );
    }

    // ---- traversal / symlink / relative escape -------------------------------

    #[test]
    fn traversal_escaping_allowed_folder_is_not_auto_approved() {
        let dir = tempdir();
        let outside = tempdir();
        let secret = outside.path().join("secret.txt");
        fs::write(&secret, b"x").unwrap();
        let policy = PermissionPolicy {
            allowed_folders: vec![allow_folder(dir.path(), true)],
            ..Default::default()
        };
        let traversal = format!(
            "{}/../{}/secret.txt",
            canon(dir.path()).display(),
            outside.path().file_name().unwrap().to_string_lossy()
        );
        // Resolves OUTSIDE the allowed folder → not auto-approved.
        assert_ne!(
            evaluate_policy(&policy, "read_file", ApprovalRisk::Safe, &args_path(&traversal)),
            PolicyDecision::AutoApprove
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escaping_allowed_folder_is_not_auto_approved() {
        use std::os::unix::fs::symlink;
        let dir = tempdir();
        let outside = tempdir();
        let real = outside.path().join("secret.txt");
        fs::write(&real, b"x").unwrap();
        let link = dir.path().join("link.txt");
        symlink(&real, &link).unwrap();
        let policy = PermissionPolicy {
            allowed_folders: vec![allow_folder(dir.path(), true)],
            ..Default::default()
        };
        // canonicalize follows the link OUT of the allowed folder.
        assert_ne!(
            evaluate_policy(&policy, "read_file", ApprovalRisk::Safe, &args_path(link.to_str().unwrap())),
            PolicyDecision::AutoApprove
        );
    }

    #[cfg(unix)]
    #[test]
    fn dangling_symlink_leaf_requires_approval() {
        use std::os::unix::fs::symlink;
        let dir = tempdir();
        let outside = tempdir();
        let missing = outside.path().join("will-create.txt");
        let link = dir.path().join("new.txt");
        symlink(&missing, &link).unwrap(); // dangling symlink leaf inside allowed
        let policy = PermissionPolicy {
            allowed_folders: vec![allow_folder(dir.path(), true)],
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "write_file", ApprovalRisk::Caution, &args_path(link.to_str().unwrap())),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn relative_path_requires_approval() {
        let policy = PermissionPolicy::default();
        assert_eq!(
            evaluate_policy(&policy, "read_file", ApprovalRisk::Safe, &args_path("relative/x.txt")),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn unresolvable_absolute_path_requires_approval() {
        let policy = PermissionPolicy::default();
        // Absolute but the parent does not exist → unresolvable → confirm.
        let bogus = if cfg!(windows) {
            "C:\\rhanis-does-not-exist-xyz\\nope\\file.txt"
        } else {
            "/rhanis-does-not-exist-xyz/nope/file.txt"
        };
        assert_eq!(
            evaluate_policy(&policy, "read_file", ApprovalRisk::Safe, &args_path(bogus)),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn new_file_in_allowed_folder_auto_approves() {
        // A not-yet-existing file whose parent IS the allowed folder.
        let dir = tempdir();
        let new = dir.path().join("fresh.txt");
        let policy = PermissionPolicy {
            allowed_folders: vec![allow_folder(dir.path(), false)],
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "write_file", ApprovalRisk::Caution, &args_path(new.to_str().unwrap())),
            PolicyDecision::AutoApprove
        );
    }

    // ---- deny unresolvable base (R-A[P2] invariant) --------------------------

    #[test]
    fn unresolvable_deny_base_does_not_falsely_gate_unrelated_target() {
        // A denied folder that does not resolve cannot contain a real target, so
        // an unrelated target inside an allowed folder still auto-approves.
        let dir = tempdir();
        let file = dir.path().join("note.txt");
        fs::write(&file, b"x").unwrap();
        let policy = PermissionPolicy {
            allowed_folders: vec![allow_folder(dir.path(), false)],
            denied_folders: vec![format!("{}/never-existed", canon(dir.path()).display())],
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "read_file", ApprovalRisk::Safe, &args_path(file.to_str().unwrap())),
            PolicyDecision::AutoApprove
        );
    }

    #[test]
    fn relative_policy_folder_base_never_grants() {
        // A relative allowed-folder entry (e.g. "." / "work") would canonicalize
        // against the process CWD — it must NOT grant an allow (R-C[P2]).
        let dir = tempdir();
        let file = dir.path().join("note.txt");
        fs::write(&file, b"x").unwrap();
        let policy = PermissionPolicy {
            allowed_folders: vec![
                AllowedFolder { path: ".".into(), allow_danger: true },
                AllowedFolder { path: "work".into(), allow_danger: true },
            ],
            ..Default::default()
        };
        // Even if the file happens to be reachable from CWD, a relative base can't
        // auto-approve it → falls through to Default (existing tier).
        assert_eq!(
            evaluate_policy(&policy, "read_file", ApprovalRisk::Safe, &args_path(file.to_str().unwrap())),
            PolicyDecision::Default
        );
    }

    #[test]
    fn existing_deny_subdir_gates_target_inside_it() {
        // When the deny zone exists, a target inside it is gated even though the
        // parent is allowed (deny > allow).
        let dir = tempdir();
        let secret_dir = dir.path().join("secret");
        fs::create_dir(&secret_dir).unwrap();
        let file = secret_dir.join("inside.txt");
        fs::write(&file, b"x").unwrap();
        let policy = PermissionPolicy {
            allowed_folders: vec![allow_folder(dir.path(), true)],
            denied_folders: vec![canon(&secret_dir).to_string_lossy().into_owned()],
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "read_file", ApprovalRisk::Safe, &args_path(file.to_str().unwrap())),
            PolicyDecision::RequireApproval
        );
    }

    // ---- URL: substring bypass, scheme, all-urls, deny, userinfo, IDN --------

    #[test]
    fn url_substring_bypass_is_rejected() {
        let policy = PermissionPolicy {
            allowed_url_hosts: vec!["openai.com".into()],
            ..Default::default()
        };
        // exact + subdomain match
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://openai.com/x")),
            PolicyDecision::AutoApprove
        );
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://api.openai.com/x")),
            PolicyDecision::AutoApprove
        );
        // substring lookalike must NOT match (strict default → confirm)
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://evil-openai.com/x")),
            PolicyDecision::RequireApproval
        );
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://openai.com.evil.com/x")),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn url_userinfo_cannot_spoof_host() {
        let policy = PermissionPolicy {
            allowed_url_hosts: vec!["openai.com".into()],
            ..Default::default()
        };
        // The real host is evil.com; the userinfo "openai.com@" must not match.
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://openai.com@evil.com/")),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn url_case_and_trailing_dot_match() {
        let policy = PermissionPolicy {
            allowed_url_hosts: vec!["OpenAI.com".into()],
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://API.OPENAI.COM./x")),
            PolicyDecision::AutoApprove
        );
    }

    #[test]
    fn url_non_http_scheme_requires_approval() {
        let policy = PermissionPolicy {
            allow_all_urls: true,
            ..Default::default()
        };
        for u in ["file:///etc/passwd", "javascript:alert(1)", "ftp://x/y", "not a url"] {
            assert_eq!(
                evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url(u)),
                PolicyDecision::RequireApproval,
                "{u} must be confirmed"
            );
        }
    }

    #[test]
    fn url_strict_default_confirms_unlisted() {
        let policy = PermissionPolicy::default(); // empty
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://example.com/")),
            PolicyDecision::RequireApproval
        );
    }

    #[test]
    fn url_all_urls_toggle_auto_approves_http_https() {
        let policy = PermissionPolicy {
            allow_all_urls: true,
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://anything.example/")),
            PolicyDecision::AutoApprove
        );
    }

    #[test]
    fn url_deny_wins_over_all_urls() {
        let policy = PermissionPolicy {
            allow_all_urls: true,
            denied_url_hosts: vec!["evil.com".into()],
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://sub.evil.com/")),
            PolicyDecision::RequireApproval
        );
        // a different host still auto-approves under all-urls
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://good.com/")),
            PolicyDecision::AutoApprove
        );
    }

    #[test]
    fn idn_homograph_normalizes_to_punycode() {
        // An IDN allow entry must match the punycode host the url crate produces.
        let policy = PermissionPolicy {
            allowed_url_hosts: vec!["bücher.example".into()],
            ..Default::default()
        };
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://xn--bcher-kva.example/x")),
            PolicyDecision::AutoApprove
        );
        // an unrelated punycode host is NOT matched
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://xn--other-kva.example/x")),
            PolicyDecision::RequireApproval
        );
    }

    // ---- external_upload is never auto-approved ------------------------------

    #[test]
    fn external_upload_never_auto_approved_even_with_all_urls() {
        let policy = PermissionPolicy {
            allow_all_urls: true,
            ..Default::default()
        };
        // Even with a url arg + allow_all_urls, external_upload has no policy
        // target → Default → the dispatcher keeps the DANGER gate.
        assert_eq!(
            evaluate_policy(&policy, "external_upload", ApprovalRisk::Danger, &args_url("https://anywhere.example/")),
            PolicyDecision::Default
        );
    }

    // ---- target-free tools / unknown tools -----------------------------------

    #[test]
    fn target_free_and_unknown_tools_are_default() {
        let policy = PermissionPolicy {
            allow_all_urls: true,
            allowed_folders: vec![],
            ..Default::default()
        };
        for tool in ["run_command", "open_app", "web_search", "write_note", "take_screenshot", "totally_unknown"] {
            assert_eq!(
                evaluate_policy(&policy, tool, ApprovalRisk::Danger, &serde_json::json!({})),
                PolicyDecision::Default,
                "{tool} is not policy-governed"
            );
        }
    }

    #[test]
    fn empty_policy_folders_default_urls_strict() {
        let policy = PermissionPolicy::default();
        let dir = tempdir();
        let file = dir.path().join("x.txt");
        fs::write(&file, b"x").unwrap();
        // folder target, empty policy → Default (existing tier)
        assert_eq!(
            evaluate_policy(&policy, "read_file", ApprovalRisk::Safe, &args_path(file.to_str().unwrap())),
            PolicyDecision::Default
        );
        // url target, empty policy → strict confirm
        assert_eq!(
            evaluate_policy(&policy, "open_url", ApprovalRisk::Caution, &args_url("https://x.example/")),
            PolicyDecision::RequireApproval
        );
    }

    // ---- decide(): Unavailable + None target ---------------------------------

    #[test]
    fn unavailable_state_forces_approval_for_targets_only() {
        let state = PolicyState::Unavailable;
        let dir = tempdir();
        let file = dir.path().join("x.txt");
        fs::write(&file, b"x").unwrap();
        // path target → RequireApproval (keeps the deny protections)
        assert_eq!(
            decide(&state, "read_file", ApprovalRisk::Safe, &args_path(file.to_str().unwrap())),
            PolicyDecision::RequireApproval
        );
        // url target → RequireApproval
        assert_eq!(
            decide(&state, "open_url", ApprovalRisk::Caution, &args_url("https://x.example/")),
            PolicyDecision::RequireApproval
        );
        // target-free tool → Default (not policy-governed; tier/shell list cover it)
        assert_eq!(
            decide(&state, "run_command", ApprovalRisk::Danger, &serde_json::json!({})),
            PolicyDecision::Default
        );
    }

    // ---- validate_permission_policy ------------------------------------------

    #[test]
    fn validate_accepts_empty_and_reasonable() {
        assert!(validate_permission_policy(&PermissionPolicy::default()).is_ok());
        let p = PermissionPolicy {
            allowed_folders: vec![AllowedFolder { path: "/home/u/work".into(), allow_danger: true }],
            denied_folders: vec!["/home/u/secret".into()],
            allowed_url_hosts: vec!["openai.com".into(), "bücher.example".into()],
            denied_url_hosts: vec!["evil.com".into()],
            allow_all_urls: false,
        };
        assert!(validate_permission_policy(&p).is_ok());
    }

    #[test]
    fn validate_rejects_too_many_entries() {
        let p = PermissionPolicy {
            denied_folders: (0..MAX_ENTRIES + 1).map(|i| format!("/x/{i}")).collect(),
            ..Default::default()
        };
        assert!(validate_permission_policy(&p).is_err());
    }

    #[test]
    fn validate_rejects_malformed_host() {
        for bad in ["http://openai.com", "openai.com/path", "user@openai.com", "openai.com:443", "has space"] {
            let p = PermissionPolicy {
                denied_url_hosts: vec![bad.into()],
                ..Default::default()
            };
            assert!(
                validate_permission_policy(&p).is_err(),
                "malformed host {bad:?} must be rejected"
            );
        }
        // empty host entry is a tolerated no-op
        let p = PermissionPolicy {
            allowed_url_hosts: vec!["".into()],
            ..Default::default()
        };
        assert!(validate_permission_policy(&p).is_ok());
    }

    #[test]
    fn validate_error_messages_are_leak_free() {
        let p = PermissionPolicy {
            denied_url_hosts: vec!["bad host/with/path".into()],
            ..Default::default()
        };
        let e = validate_permission_policy(&p).unwrap_err();
        assert!(!e.contains('/'));
        assert!(!e.is_empty());
    }

    // ---- serde round-trip + migration default --------------------------------

    #[test]
    fn policy_serde_round_trips() {
        let p = PermissionPolicy {
            allowed_folders: vec![AllowedFolder { path: "/a".into(), allow_danger: true }],
            denied_folders: vec!["/b".into()],
            allowed_url_hosts: vec!["openai.com".into()],
            denied_url_hosts: vec!["evil.com".into()],
            allow_all_urls: true,
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: PermissionPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn policy_deserializes_from_partial_and_empty() {
        // Missing fields default (forward/backward compat).
        let p: PermissionPolicy = serde_json::from_str("{}").unwrap();
        assert_eq!(p, PermissionPolicy::default());
        // allow_danger defaults to false when omitted.
        let f: AllowedFolder = serde_json::from_str(r#"{"path":"/a"}"#).unwrap();
        assert!(!f.allow_danger);
    }
}
