//! Safe target descriptors for human-facing disclosure (koe-whf).
//!
//! The approval modal and the ActivityLog previously showed only `run {tool}` —
//! the operator approved a DANGER op without seeing WHAT it targets, which
//! weakens both the last human barrier against prompt injection and the
//! glass-box disclosure thesis. This module derives a **safe but meaningful**
//! descriptor from the (model-controlled, attacker-influenceable) tool args:
//!
//! - path tools (`read_file` / `write_file` / `delete_file`): home-relative
//!   path (`~/Documents/x.txt`) so the home prefix (and with it the OS
//!   username) is never echoed; matching is lexical and component-wise, so
//!   mixed (`C:/Users/…`) or doubled (`//home/…`) separators still relativize.
//!   Outside the home dir only the last two components (`…/dir/file`). A `..`
//!   component additionally appends an un-elidable `(parent traversal)` marker
//!   — the tail/middle reductions could otherwise hide it, and a traversal
//!   attempt is exactly what the human gate must see (the policy/IO layers
//!   fail closed independently).
//! - `run_command`: the first whitespace token only (the executable the
//!   allow-list will judge), never the full argv (argv may carry secrets). A
//!   separator-bearing first token (never run — the allow-list rejects it) is
//!   displayed through the path rules above so an absolute token cannot leak
//!   the username either.
//! - `open_url` / `external_upload`: the host only, via the SAME parser the
//!   permission policy matches against ([`crate::permission_policy::url_host`]):
//!   IDN → punycode (anti-homograph), userinfo cannot spoof the host, and the
//!   path/query/fragment (where tokens hide) never appear. An explicit
//!   non-default port IS shown (`host:8080`) — it changes where the browser
//!   lands, so omitting it would under-disclose.
//! - `open_app`: the app name string (`name` key only — align with the real
//!   schema when the tool lands, koe-p1a).
//! - content-only / target-free tools (`write_note`, `web_search`,
//!   `take_screenshot`, unknown): NO descriptor — note text and search queries
//!   are *content*, not a target, and must never be echoed into summaries.
//!
//! Display hygiene: control chars and invisible/bidi formatting chars are
//! REPLACED with U+FFFD (not stripped) — a model-supplied RTL override can
//! neither reorder the rendering (`txt.exe` as `exe.txt`) nor be silently
//! laundered out into a clean-looking descriptor; the human sees the tamper
//! marks. Descriptors are middle-truncated to a fixed char budget that always
//! preserves the tail (the basename is the part the human needs).

use serde_json::Value;
use url::Url;

/// Char budget for one descriptor (well under approval_gate's 500-byte
/// `MAX_SUMMARY_LEN` belt-and-suspenders cap).
const MAX_DESCRIPTOR_CHARS: usize = 96;

/// Builds the human-facing summary for a tool call: `run {tool}: {descriptor}`
/// when a safe descriptor can be derived, plain `run {tool}` otherwise.
/// Consumed by the tool_dispatcher for the phase events AND the approval
/// request, so the modal and the ActivityLog always tell the same story.
pub(crate) fn run_summary(tool: &str, args: &Value) -> String {
    // The tool name is model-controlled too — an unknown (hostile) name reaches
    // the DANGER modal fail-closed. Same hygiene as the descriptor; the broader
    // payload-field hardening is koe-eh4.
    let tool = sanitize_display(tool);
    match descriptor(&tool, args) {
        Some(d) => format!("run {tool}: {d}"),
        None => format!("run {tool}"),
    }
}

/// Derives the safe descriptor for one tool call, or `None` for tools whose
/// args are content (never displayed) or when the expected arg is missing /
/// not a string / empty (fail-quiet: the summary falls back to the plain
/// pre-koe-whf `run {tool}` floor — a malformed arg must not break dispatch,
/// and the gate itself never depends on this string).
fn descriptor(tool: &str, args: &Value) -> Option<String> {
    let raw = match tool {
        // Keep these key names in lockstep with `permission_policy::policy_target`
        // — the parity test below locks the two maps together.
        "read_file" | "write_file" | "delete_file" => path_descriptor(str_arg(args, "path")?),
        "open_url" | "external_upload" => host_descriptor(str_arg(args, "url")?),
        "run_command" => command_descriptor(str_arg(args, "command")?),
        "open_app" => str_arg(args, "name").map(str::to_string),
        // write_note text / web_search query / screenshot: content or
        // target-free — never echoed (see module doc).
        _ => None,
    }?;
    let clean = sanitize_display(&raw);
    if clean.is_empty() {
        return None;
    }
    Some(cap_middle(&clean, MAX_DESCRIPTOR_CHARS))
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    match args.get(key).and_then(Value::as_str) {
        Some(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}

// ---- path -------------------------------------------------------------------

/// Home-relative form when the path is lexically inside the home dir (the home
/// prefix / username never shown); otherwise the last two components. Lexical
/// only — NO canonicalization, so a `..` traversal is displayed as asked
/// rather than resolved away into a deceptively clean display; because the
/// tail/middle reductions can drop a `..` segment, its presence is also
/// surfaced as an explicit suffix marker that survives truncation (tail-kept).
fn path_descriptor(raw: &str) -> Option<String> {
    let raw = strip_verbatim(raw);
    let body = home_relative(raw).or_else(|| tail_components(raw))?;
    let has_traversal = raw.split(['/', '\\']).any(|c| c == "..");
    Some(if has_traversal {
        format!("{body} (parent traversal)")
    } else {
        body
    })
}

fn home_relative(raw: &str) -> Option<String> {
    let home = dirs_next::home_dir()?;
    home_relative_to(raw, &home.to_string_lossy(), cfg!(windows))
}

/// Pure core of [`home_relative`] (unit-testable with fixed fixtures,
/// including the Windows case-fold branch from Linux CI). Component-wise
/// lexical prefix match: separators may be `/` or `\`, duplicates collapse,
/// and on Windows (`fold_ascii_case`) components compare case-insensitively.
/// Both sides must be lexically absolute — otherwise a RELATIVE arg like
/// `home/user/x` (which resolves against the CWD, not `/home/user`) could
/// masquerade as `~/x`.
fn home_relative_to(raw: &str, home: &str, fold_ascii_case: bool) -> Option<String> {
    if !lexically_absolute(raw) || !lexically_absolute(home) {
        return None;
    }
    let eq = |a: &str, b: &str| {
        if fold_ascii_case {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    let home_comps = components(home);
    let raw_comps = components(raw);
    if home_comps.is_empty() || raw_comps.len() < home_comps.len() {
        return None;
    }
    if !home_comps.iter().zip(&raw_comps).all(|(h, r)| eq(h, r)) {
        return None;
    }
    let rest = &raw_comps[home_comps.len()..];
    if rest.is_empty() {
        return Some("~".to_string());
    }
    Some(format!("~/{}", rest.join("/")))
}

fn components(p: &str) -> Vec<&str> {
    p.split(['/', '\\']).filter(|c| !c.is_empty()).collect()
}

/// Strips the Windows verbatim / device namespace prefix (`\\?\`, `\\.\`).
/// Left in place it would defeat the home match (first component `?` / `.`)
/// and leak the username through the tail fallback for a file in the home root.
fn strip_verbatim(raw: &str) -> &str {
    raw.strip_prefix(r"\\?\")
        .or_else(|| raw.strip_prefix(r"\\.\"))
        .unwrap_or(raw)
}

/// Lexically absolute: rooted (`/`, `\`) or drive-lettered (`C:`). Display
/// classification only — never used for IO.
fn lexically_absolute(p: &str) -> bool {
    let b = p.as_bytes();
    p.starts_with('/')
        || p.starts_with('\\')
        || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
}

/// Last two path components prefixed with `…/` (or the bare name for a
/// separator-free relative arg). Enough for the human to recognise WHAT is
/// touched without echoing the full absolute path.
fn tail_components(raw: &str) -> Option<String> {
    let comps = components(raw);
    let had_separator = raw.contains(['/', '\\']);
    match comps.as_slice() {
        [] => None,
        [only] if !had_separator => Some((*only).to_string()),
        [only] => Some(format!("…/{only}")),
        [.., parent, base] => Some(format!("…/{parent}/{base}")),
    }
}

// ---- url / command ------------------------------------------------------------

/// Host (plus any explicit non-default port), through the SAME parser the
/// permission policy uses — one canonical form for what the policy judges and
/// what the human sees. The `url` crate strips scheme-default ports, so a
/// surviving `port()` is always the non-default kind worth disclosing.
fn host_descriptor(raw: &str) -> Option<String> {
    let host = crate::permission_policy::url_host(raw)?;
    let port = Url::parse(raw).ok().and_then(|u| u.port());
    Some(match port {
        Some(p) => format!("{host}:{p}"),
        None => host.to_string(),
    })
}

/// First whitespace token = the executable the allow-list will judge. The rest
/// of the argv NEVER appears (it can carry tokens/keys as arguments). A token
/// with a path separator is never executed (the allow-list rejects it) but it
/// is still shown — through the path rules, so an absolute first token cannot
/// leak the home prefix / username into the modal.
fn command_descriptor(raw: &str) -> Option<String> {
    let tok = raw.split_whitespace().next()?;
    if tok.contains(['/', '\\']) {
        return path_descriptor(tok);
    }
    Some(tok.to_string())
}

// ---- display hygiene ----------------------------------------------------------

/// Replaces control chars and invisible/bidi formatting chars with U+FFFD so a
/// model-supplied string can neither reorder/hide its rendering in the modal /
/// ActivityLog (display spoofing) nor be silently laundered into a clean
/// descriptor — the replacement marks make tampering visible to the human.
fn sanitize_display(s: &str) -> String {
    s.chars()
        .map(|c| if is_display_hostile(c) { '\u{FFFD}' } else { c })
        .collect()
}

/// Control chars plus the invisible / direction-altering format characters a
/// crafted arg could use to spoof the rendering. Intentionally NOT a blanket
/// "non-ASCII" filter — legitimate CJK / accented filenames must display as-is.
fn is_display_hostile(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            // soft hyphen | Arabic letter mark | Hangul fillers | Mongolian VS
            '\u{00AD}' | '\u{061C}' | '\u{115F}' | '\u{1160}' | '\u{180E}'
            // zero-width + bidi marks | bidi embedding/override | line/para sep
            | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2028}' | '\u{2029}'
            // bidi isolates | word joiner + invisible operators | Hangul filler
            | '\u{2066}'..='\u{2069}' | '\u{2060}'..='\u{2064}' | '\u{3164}'
            // variation selectors | BOM/ZWNBSP | interlinear | tag block
            | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}' | '\u{FFF9}'..='\u{FFFB}'
            | '\u{E0000}'..='\u{E007F}'
        )
}

/// Caps to `max_chars` keeping head AND tail (`abc…yz`): for a path the
/// basename at the END is what the human must judge, so end-truncation would
/// cut exactly the wrong part.
fn cap_middle(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let head = max_chars / 3;
    let tail = max_chars.saturating_sub(head + 1); // -1 for the ellipsis
    let head_s: String = s.chars().take(head).collect();
    let tail_s: String = s.chars().skip(count - tail).collect();
    format!("{head_s}…{tail_s}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn home() -> std::path::PathBuf {
        dirs_next::home_dir().expect("test env has a home dir")
    }

    // ---- path: home-relative (home prefix / username never shown) ----

    #[test]
    fn path_inside_home_is_home_relative_and_hides_username() {
        let p = home().join("Documents").join("notes.txt");
        let s = run_summary("delete_file", &json!({ "path": p.to_string_lossy() }));
        assert!(s.starts_with("run delete_file: ~"), "{s}");
        assert!(s.contains("notes.txt"), "{s}");
        let home_s = home().to_string_lossy().to_string();
        assert!(!s.contains(&home_s), "must not echo the home prefix: {s}");
    }

    #[test]
    fn path_outside_home_shows_last_two_components_only() {
        let s = run_summary("write_file", &json!({ "path": "/var/log/app/run.log" }));
        assert_eq!(s, "run write_file: …/app/run.log");
    }

    #[test]
    fn sibling_of_home_is_not_disguised_as_home() {
        let p = format!("{}-evil/x.txt", home().to_string_lossy());
        let s = run_summary("delete_file", &json!({ "path": p }));
        assert!(!s.contains("~/x.txt"), "{s}");
        assert!(s.contains("x.txt"), "{s}");
    }

    #[test]
    fn traversal_is_marked_un_elidably() {
        let p = home().join("Documents").join("..").join("..").join("x");
        let s = run_summary("delete_file", &json!({ "path": p.to_string_lossy() }));
        assert!(s.contains("(parent traversal)"), "{s}");
        // Out-of-home: the tail reduction drops the leading `..`, the marker
        // must still flag it.
        let s2 = run_summary("delete_file", &json!({ "path": "/a/../../etc/passwd" }));
        assert!(s2.contains("…/etc/passwd"), "{s2}");
        assert!(s2.contains("(parent traversal)"), "{s2}");
    }

    #[test]
    fn bare_relative_name_shown_as_is() {
        let s = run_summary("read_file", &json!({ "path": "notes.txt" }));
        assert_eq!(s, "run read_file: notes.txt");
    }

    #[test]
    fn windows_style_path_splits_on_backslash() {
        let s = run_summary("write_file", &json!({ "path": r"D:\work\proj\out.txt" }));
        assert_eq!(s, "run write_file: …/proj/out.txt");
    }

    // ---- home_relative_to: pure fixtures (incl. the Windows fold from Linux) ----

    #[test]
    fn home_relative_matches_mixed_and_doubled_separators() {
        assert_eq!(
            home_relative_to("//home/user/docs//a.txt", "/home/user", false),
            Some("~/docs/a.txt".to_string())
        );
        assert_eq!(
            home_relative_to(r"C:/Users/Alice/Secret/file.txt", r"C:\Users\Alice", true),
            Some("~/Secret/file.txt".to_string())
        );
    }

    #[test]
    fn home_relative_folds_case_only_when_asked() {
        assert_eq!(
            home_relative_to(r"c:\users\alice\x.txt", r"C:\Users\Alice", true),
            Some("~/x.txt".to_string())
        );
        assert_eq!(home_relative_to("/HOME/USER/x.txt", "/home/user", false), None);
    }

    #[test]
    fn home_relative_requires_lexically_absolute_arg() {
        // A relative spelling of the home path resolves against the CWD — it
        // must NOT masquerade as `~/x.txt`.
        assert_eq!(home_relative_to("home/user/x.txt", "/home/user", false), None);
    }

    #[test]
    fn home_relative_exact_home_is_tilde() {
        assert_eq!(home_relative_to("/home/user/", "/home/user", false), Some("~".to_string()));
    }

    #[test]
    fn windows_verbatim_prefix_is_stripped_before_home_match() {
        assert_eq!(strip_verbatim(r"\\?\C:\Users\Alice\f.txt"), r"C:\Users\Alice\f.txt");
        assert_eq!(strip_verbatim(r"\\.\C:\x"), r"C:\x");
        assert_eq!(strip_verbatim("/home/user/x"), "/home/user/x");
        // The stripped form home-relativizes (the un-stripped one would not).
        assert_eq!(
            home_relative_to(strip_verbatim(r"\\?\C:\Users\Alice\f.txt"), r"C:\Users\Alice", true),
            Some("~/f.txt".to_string())
        );
    }

    // ---- url: host only, canonical ----

    #[test]
    fn url_shows_host_only_no_userinfo_path_query() {
        let s = run_summary(
            "open_url",
            &json!({ "url": "https://login:token-abc@evil.example/p/a?key=secret#f" }),
        );
        assert_eq!(s, "run open_url: evil.example");
    }

    #[test]
    fn url_shows_explicit_non_default_port() {
        let s = run_summary("open_url", &json!({ "url": "https://bank.example:8443/x" }));
        assert_eq!(s, "run open_url: bank.example:8443");
        // Scheme-default port is normalized away by the url crate — not shown.
        let s2 = run_summary("open_url", &json!({ "url": "https://bank.example:443/x" }));
        assert_eq!(s2, "run open_url: bank.example");
    }

    #[test]
    fn idn_host_renders_as_punycode() {
        // Cyrillic "е" homograph — must NOT render lookalike unicode.
        let s = run_summary("open_url", &json!({ "url": "https://еxample.com/" }));
        assert!(s.contains("xn--"), "IDN must display as punycode: {s}");
    }

    #[test]
    fn non_http_scheme_yields_no_descriptor() {
        let s = run_summary("open_url", &json!({ "url": "file:///etc/passwd" }));
        assert_eq!(s, "run open_url");
    }

    #[test]
    fn external_upload_shows_host_only() {
        let s = run_summary("external_upload", &json!({ "url": "https://api.host.io/up" }));
        assert_eq!(s, "run external_upload: api.host.io");
    }

    // ---- run_command: first token only ----

    #[test]
    fn command_shows_first_token_only() {
        let s = run_summary("run_command", &json!({ "command": "ls -la /home/user/.ssh" }));
        assert_eq!(s, "run run_command: ls");
    }

    #[test]
    fn command_absolute_first_token_is_home_relativized() {
        let cmd = format!("{} --flag", home().join(".ssh").join("id_rsa").to_string_lossy());
        let s = run_summary("run_command", &json!({ "command": cmd }));
        assert!(s.contains("id_rsa"), "{s}");
        let home_s = home().to_string_lossy().to_string();
        assert!(!s.contains(&home_s), "must not echo the home prefix: {s}");
        assert!(!s.contains("--flag"), "argv must not leak: {s}");
    }

    // ---- open_app ----

    #[test]
    fn open_app_shows_name() {
        let s = run_summary("open_app", &json!({ "name": "Notepad" }));
        assert_eq!(s, "run open_app: Notepad");
    }

    // ---- parity: descriptor reads the same key the policy judges ----

    #[test]
    fn descriptor_reads_the_same_key_the_policy_judges() {
        use crate::permission_policy::{policy_target, PolicyTarget};
        // Decoy values under every OTHER key: if either side's key map drifts,
        // the descriptor would show a decoy and this test fails.
        let args = json!({
            "path": "/var/data/real-target.txt",
            "url": "https://real-host.example/x",
            "command": "realcmd -v",
        });
        for tool in ["read_file", "write_file", "delete_file"] {
            match policy_target(tool, &args) {
                PolicyTarget::Path(p) => {
                    let d = descriptor(tool, &args).expect("path descriptor");
                    let base = p.rsplit(['/', '\\']).next().unwrap();
                    assert!(d.contains(base), "{tool}: {d} must derive from policy path {p}");
                    assert!(!d.contains("real-host"), "{tool}: {d} shows a decoy key");
                }
                _ => panic!("{tool} must be a Path policy target"),
            }
        }
        match policy_target("open_url", &args) {
            PolicyTarget::Url(u) => {
                let d = descriptor("open_url", &args).expect("url descriptor");
                let host = crate::permission_policy::url_host(u).unwrap().to_string();
                assert_eq!(d, host, "open_url descriptor must show the policy-judged host");
            }
            _ => panic!("open_url must be a Url policy target"),
        }
    }

    // ---- content-only / fallback ----

    #[test]
    fn content_tools_and_missing_args_fall_back_to_tool_name() {
        assert_eq!(
            run_summary("write_note", &json!({ "text": "/home/user/.ssh/id_rsa" })),
            "run write_note"
        );
        assert_eq!(
            run_summary("web_search", &json!({ "query": "my private question" })),
            "run web_search"
        );
        assert_eq!(run_summary("delete_file", &json!({})), "run delete_file");
        assert_eq!(run_summary("delete_file", &json!({ "path": 42 })), "run delete_file");
        assert_eq!(run_summary("delete_file", &json!({ "path": "" })), "run delete_file");
        assert_eq!(run_summary("unknown_tool", &json!({ "path": "/x/y" })), "run unknown_tool");
    }

    // ---- display hygiene ----

    #[test]
    fn hostile_chars_are_replaced_with_visible_tamper_marks() {
        let s = run_summary(
            "open_app",
            &json!({ "name": "evil\u{202E}exe.txt\u{0007}\u{200B}" }),
        );
        assert!(!s.contains('\u{202E}'), "{s:?}");
        assert!(!s.contains('\u{0007}'), "{s:?}");
        assert!(!s.contains('\u{200B}'), "{s:?}");
        // Replaced, not stripped: tampering stays visible instead of being
        // laundered into a clean-looking name.
        assert!(s.contains("evil\u{FFFD}exe.txt"), "{s:?}");
    }

    #[test]
    fn extended_invisible_format_chars_are_marked() {
        let s = run_summary(
            "open_app",
            &json!({ "name": "a\u{2060}b\u{00AD}c\u{E0041}d\u{FE0F}" }),
        );
        assert_eq!(s, "run open_app: a\u{FFFD}b\u{FFFD}c\u{FFFD}d\u{FFFD}");
    }

    #[test]
    fn hostile_only_name_shows_tamper_marks_not_a_clean_fallback() {
        let s = run_summary("open_app", &json!({ "name": "\u{202E}\u{0000}\n" }));
        assert_eq!(s, "run open_app: \u{FFFD}\u{FFFD}\u{FFFD}");
    }

    #[test]
    fn overlong_descriptor_is_middle_capped_keeping_basename() {
        let long_dir = "d".repeat(300);
        let p = format!("/srv/{long_dir}/keep-this-name.txt");
        let s = run_summary("write_file", &json!({ "path": p }));
        assert!(s.contains("keep-this-name.txt"), "tail must survive: {s}");
        assert!(s.contains('…'), "{s}");
        let descriptor = s.strip_prefix("run write_file: ").unwrap();
        assert!(
            descriptor.chars().count() <= MAX_DESCRIPTOR_CHARS,
            "cap: {} chars",
            descriptor.chars().count()
        );
    }

    #[test]
    fn cap_middle_is_noop_under_budget_and_never_underflows() {
        assert_eq!(cap_middle("short", 96), "short");
        // Degenerate budgets must not panic (saturating tail).
        assert_eq!(cap_middle("abc", 1), "…");
        assert_eq!(cap_middle("abc", 0), "…");
    }
}
