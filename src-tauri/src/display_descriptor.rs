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
//!   username) is never echoed; matching is lexical and component-wise, and
//!   doubled separators (`//home/…`) collapse on both platforms. Separators
//!   are platform-aware — on Windows `/` and `\` both split (mixed
//!   `C:/Users/…` spellings still relativize), while on Unix `\` is a regular
//!   filename character and never splits (folding it would describe a
//!   different file than the one the filesystem touches).
//!   Outside the home dir only the last two components (`…/dir/file`). A `..`
//!   component additionally appends an un-elidable `(parent traversal)` marker
//!   — the tail/middle reductions could otherwise hide it, and a traversal
//!   attempt is exactly what the human gate must see (the policy/IO layers
//!   fail closed independently). A Windows UNC target outside the home
//!   likewise appends `(network: \\server)` — the tail reduction would
//!   otherwise hide that the operation leaves the machine.
//! - `run_command`: the first whitespace token only (the executable the
//!   allow-list will judge), never the full argv (argv may carry secrets). A
//!   first token bearing a separator under the platform's semantics (never
//!   run — the allow-list rejects it) is displayed through the path rules
//!   above so an absolute token cannot leak the username either.
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

/// Char budget for the UNC host inside the `(network: \\host)` marker. The
/// host length is attacker-controlled — uncapped it could push the marker
/// labels (or the body tail) out of the display budget (R-C finding).
const UNC_HOST_MAX_CHARS: usize = 24;

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
        // open_url's key is parity-locked against the policy below.
        // external_upload is not implemented yet — this mapping IS the UX
        // contract: when the tool lands (koe-p1a) its destination MUST be the
        // "url" key and join policy_target + the parity test, otherwise the
        // modal could show a decoy host for the one DANGER op that has no
        // independent policy/IO backstop (red-team, PR #57).
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
    path_descriptor_for(raw, cfg!(windows))
}

/// Platform-explicit core of [`path_descriptor`] (both separator branches
/// unit-testable from Linux CI; NOT pure — the home prefix still comes from
/// the real environment via [`home_relative`]). Separator semantics MUST
/// follow the platform the tool will run on: on
/// Unix `\` is a valid filename character, and folding it as a separator would
/// let `/home/alice\x` display as `~/x` while the filesystem touches a
/// root-level `/home` entry named `alice\x` — the modal would describe a
/// different target than the one being approved (Codex Cloud P2, PR #57).
fn path_descriptor_for(raw: &str, windows: bool) -> Option<String> {
    // ONLY the `\\?\` "root local device" form bypasses Win32 path
    // canonicalization (trailing dots/spaces passed through verbatim); `\\.\`
    // "local device" paths ARE canonicalized, so they must still get the
    // trailing-trim display below (MS docs + Project Zero Win32→NT analysis,
    // R-C round 3). Both prefixes are still stripped for display.
    let verbatim = windows && skips_path_normalization(raw);
    let raw = if windows {
        // `\\?\` / `\\.\` are Windows-only namespaces; on Unix the same bytes
        // are just a (weird) filename and must display as-is.
        strip_verbatim(raw)
    } else {
        std::borrow::Cow::Borrowed(raw)
    };
    let raw = raw.as_ref();
    // Win32 strips trailing dots/spaces from every non-verbatim component, so
    // `payroll.xlsx. ` is really `payroll.xlsx`; mirror that for display (and
    // for the home match) so the modal names the file the OS actually touches.
    let trim = windows && !verbatim;
    let home_form = home_relative(raw, windows, trim);
    let in_home = home_form.is_some();
    let body = home_form.or_else(|| tail_components(raw, windows, trim))?;
    let mut markers = String::new();
    if components(raw, windows).iter().any(|c| is_traversal_component(c, windows)) {
        markers.push_str(" (parent traversal)");
    }
    // The tail reduction would elide a UNC remote host — the single most
    // decision-relevant fact for a network target — so it is re-surfaced as
    // an un-elidable marker. A UNC home (roaming profile) relativizes to `~`
    // above: that IS the user's home, no marker noise (red-team, PR #57).
    if windows && !in_home {
        if let Some(host) = unc_host(raw) {
            markers.push_str(&format!(" (network: \\\\{})", cap_middle(host, UNC_HOST_MAX_CHARS)));
        }
    }
    // Budget the body AROUND the risk markers: with a single whole-string cap
    // an attacker could pad the path/host until the truncation swallowed the
    // markers — exactly the part the human must see (R-C finding). Marker
    // lengths are bounded (19 + 37 chars), so the body keeps ≥ 40 chars and
    // the total never exceeds MAX_DESCRIPTOR_CHARS.
    let budget = MAX_DESCRIPTOR_CHARS.saturating_sub(markers.chars().count());
    Some(format!("{}{markers}", cap_middle(&body, budget)))
}

/// Whole-component parent traversal under the platform's semantics. Win32
/// strips trailing dots and spaces from component names, so `".. "` (and kin)
/// resolve like `..`; the windows check is deliberately over-approximate in
/// the fail-safe direction (display-only over-warning — e.g. `...` errors on
/// NT rather than traversing, but still marks).
fn is_traversal_component(c: &str, windows: bool) -> bool {
    if c == ".." {
        return true;
    }
    windows
        && c.strip_prefix("..")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|ch| ch == ' ' || ch == '.'))
}

/// Remote host of a UNC-rooted (`\\server\…`) path — Windows semantics only
/// (the caller gates on the platform flag; on Unix `\\x` is a filename).
fn unc_host(raw: &str) -> Option<&str> {
    let mut chars = raw.chars();
    let two_seps =
        matches!(chars.next(), Some('/' | '\\')) && matches!(chars.next(), Some('/' | '\\'));
    if !two_seps {
        return None;
    }
    components(raw, true).first().copied()
}

fn home_relative(raw: &str, windows: bool, trim: bool) -> Option<String> {
    let home = dirs_next::home_dir()?;
    home_relative_to_trim(raw, &home.to_string_lossy(), windows, trim)
}

/// Pure core of [`home_relative`] (unit-testable with fixed fixtures,
/// including the Windows branch from Linux CI). Component-wise lexical prefix
/// match under the `windows` flag's platform semantics: on Windows separators
/// may be `/` or `\` and components compare ASCII case-insensitively; on Unix
/// only `/` separates and the comparison is exact. Duplicate separators
/// collapse on both. Both sides must be lexically absolute — otherwise a
/// RELATIVE arg like `home/user/x` (which resolves against the CWD, not
/// `/home/user`) could masquerade as `~/x`.
/// Test-only convenience wrapper with the trim defaulted to the platform
/// (non-verbatim); production goes straight to [`home_relative_to_trim`] via
/// [`home_relative`], and the verbatim "no trim" case is exercised by calling
/// [`home_relative_to_trim`] directly.
#[cfg(test)]
fn home_relative_to(raw: &str, home: &str, windows: bool) -> Option<String> {
    home_relative_to_trim(raw, home, windows, windows)
}

fn home_relative_to_trim(raw: &str, home: &str, windows: bool, trim: bool) -> Option<String> {
    if !lexically_absolute(raw, windows) || !lexically_absolute(home, windows) {
        return None;
    }
    let eq = |a: &str, b: &str| {
        if windows {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    let home_comps = display_components(home, windows, trim);
    let raw_comps = display_components(raw, windows, trim);
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

/// Path separators under the given platform semantics: `\` separates on
/// Windows but is a regular filename character on Unix.
fn separators(windows: bool) -> &'static [char] {
    if windows {
        &['/', '\\']
    } else {
        &['/']
    }
}

fn components(p: &str, windows: bool) -> Vec<&str> {
    p.split(separators(windows)).filter(|c| !c.is_empty()).collect()
}

/// Components for DISPLAY / home-matching: like [`components`] but, when
/// `trim`, each component's trailing dots/spaces are dropped to mirror Win32's
/// path normalization (`payroll.xlsx. ` → `payroll.xlsx`). A component that is
/// ALL dots/spaces (`.`, `..`, `.. `) is left intact — those are navigation
/// segments, handled by [`is_traversal_component`], not filenames.
fn display_components(p: &str, windows: bool, trim: bool) -> Vec<&str> {
    p.split(separators(windows))
        .filter(|c| !c.is_empty())
        .map(|c| trim_win32_trailing(c, trim))
        .collect()
}

fn trim_win32_trailing(c: &str, trim: bool) -> &str {
    if !trim {
        return c;
    }
    let trimmed = c.trim_end_matches([' ', '.']);
    if trimmed.is_empty() {
        c
    } else {
        trimmed
    }
}

/// A `\\?\` / `\\.\` (or `//?/`, `//./`) device prefix — stripped for display
/// because the prefix itself is not a meaningful path component. Covers both
/// the root-local-device (`?`) and local-device (`.`) forms.
fn is_device_path(raw: &str) -> bool {
    let b = raw.as_bytes();
    let is_sep = |c: u8| c == b'\\' || c == b'/';
    b.len() >= 4 && is_sep(b[0]) && is_sep(b[1]) && (b[2] == b'?' || b[2] == b'.') && is_sep(b[3])
}

/// The `\\?\` "root local device" prefix (or `//?/`) — the ONLY Win32 form
/// that disables path canonicalization, so trailing dots/spaces and `.`/`..`
/// reach the filesystem verbatim. `\\.\` "local device" paths ARE canonicalized
/// (trailing dot/space removed), so they are deliberately excluded here.
fn skips_path_normalization(raw: &str) -> bool {
    let b = raw.as_bytes();
    let is_sep = |c: u8| c == b'\\' || c == b'/';
    b.len() >= 4 && is_sep(b[0]) && is_sep(b[1]) && b[2] == b'?' && is_sep(b[3])
}

/// Strips the Windows verbatim / device namespace prefix (`\\?\`, `\\.\`).
/// Left in place it would defeat the home match (first component `?` / `.`)
/// and leak the username through the tail fallback for a file in the home root.
/// The verbatim UNC form (`\\?\UNC\server\share\…`) is normalized back to
/// `\\server\share\…` so a UNC home (roaming profile) can still relativize.
/// Matching is deliberately loose in the fail-safe direction: the prefix
/// separators may be `/` or `\` (`//?/` normalizes to the device namespace
/// too) and the `UNC` segment compares ASCII case-insensitively — the NT
/// object manager resolves `\\?\unc\…` to the same network provider, so a
/// lowercase spelling must not suppress the network marker (R-C finding).
fn strip_verbatim(raw: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    let is_sep = |c: u8| c == b'\\' || c == b'/';
    if !is_device_path(raw) {
        return Cow::Borrowed(raw);
    }
    // The first 4 bytes are ASCII (checked above), so slicing is char-safe.
    let rest = &raw[4..];
    let rb = rest.as_bytes();
    if rb.len() >= 4 && rb[..3].eq_ignore_ascii_case(b"UNC") && is_sep(rb[3]) {
        return Cow::Owned(format!(r"\\{}", &rest[4..]));
    }
    Cow::Borrowed(rest)
}

/// Lexically absolute: `/`-rooted on every platform; on Windows also a `\`
/// root or a drive letter (`C:`) — neither of which roots a Unix path.
/// Display classification only — never used for IO.
fn lexically_absolute(p: &str, windows: bool) -> bool {
    if p.starts_with('/') {
        return true;
    }
    if !windows {
        return false;
    }
    let b = p.as_bytes();
    p.starts_with('\\') || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
}

/// Last two path components prefixed with `…/` (or the bare name for a
/// separator-free relative arg). Enough for the human to recognise WHAT is
/// touched without echoing the full absolute path.
fn tail_components(raw: &str, windows: bool, trim: bool) -> Option<String> {
    let comps = display_components(raw, windows, trim);
    let had_separator = raw.contains(separators(windows));
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
/// leak the home prefix / username into the modal. An `=`-bearing token has
/// its value side masked (`KEY=…`): a `NAME=value` env assignment or
/// `--opt=value` first token can carry a secret the model is trying to surface
/// (R-C finding).
fn command_descriptor(raw: &str) -> Option<String> {
    command_descriptor_for(raw, cfg!(windows))
}

/// Platform-explicit core of [`command_descriptor`]: the separator probe
/// (path-shaped vs plain token) follows the same platform semantics as the
/// path rules.
fn command_descriptor_for(raw: &str, windows: bool) -> Option<String> {
    let tok = raw.split_whitespace().next()?;
    let eq = tok.find('=');
    let sep = tok.find(separators(windows));
    match (eq, sep) {
        // The first '=' precedes any separator (NAME=value, --opt=value,
        // --path=/home/…): everything after '=' is a VALUE — it may carry a
        // secret or a username-bearing path — so it is always masked, never
        // path-displayed ('=' is ASCII, so the byte slice is char-safe).
        (Some(e), Some(s)) if e < s => Some(format!("{}=…", &tok[..e])),
        (Some(e), None) => Some(format!("{}=…", &tok[..e])),
        // Separator first (/tmp/a=b/cmd): a path-shaped token → path rules.
        (_, Some(_)) => path_descriptor_for(tok, windows),
        (None, None) => Some(tok.to_string()),
    }
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
            // combining grapheme joiner | soft hyphen | Arabic letter mark
            '\u{034F}' | '\u{00AD}' | '\u{061C}'
            // Hangul fillers (conjoining + compat + halfwidth)
            | '\u{115F}' | '\u{1160}' | '\u{3164}' | '\u{FFA0}'
            // Mongolian variation selectors + vowel separator
            | '\u{180B}'..='\u{180E}'
            // zero-width + bidi marks | bidi embedding/override | line/para sep
            | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2028}' | '\u{2029}'
            // word joiner + invisible operators | bidi isolates | deprecated format
            | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{206A}'..='\u{206F}'
            // variation selectors (BMP + supplement) | BOM/ZWNBSP | interlinear
            | '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}' | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            // tag block (invisible "ASCII smuggling")
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
        // Windows semantics via the explicit flag (runnable from Linux CI).
        assert_eq!(
            path_descriptor_for(r"D:\work\proj\out.txt", true),
            Some("…/proj/out.txt".to_string())
        );
    }

    #[test]
    fn unix_backslash_is_a_filename_char_not_a_separator() {
        // On Unix `\` does not separate: /home/alice\Documents\report.txt is
        // a ROOT-LEVEL /home entry named "alice\Documents\report.txt" — shown
        // as ~/Documents/report.txt it would describe a different file than
        // the one the filesystem touches (Codex Cloud P2, PR #57).
        assert_eq!(
            home_relative_to(r"/home/alice\Documents\report.txt", "/home/alice", false),
            None
        );
        assert_eq!(
            path_descriptor_for(r"/home/alice\Documents\report.txt", false),
            Some(r"…/home/alice\Documents\report.txt".to_string())
        );
        // `\..\` is NOT a traversal under Unix semantics (the `..` sits inside
        // a filename) — no false traversal flag…
        assert_eq!(path_descriptor_for(r"/tmp/a\..\x", false), Some(r"…/tmp/a\..\x".to_string()));
        // …while the same spelling under Windows semantics IS one.
        assert!(path_descriptor_for(r"C:\tmp\a\..\x", true)
            .expect("descriptor")
            .contains("(parent traversal)"));
        // A drive-lettered token is not absolute on Unix either — it is a
        // relative filename and must not enter the home/tail path rules.
        assert_eq!(home_relative_to(r"C:\Users\alice\x.txt", "/home/alice", false), None);
        // The Windows verbatim namespace is NOT stripped under Unix semantics
        // — the bytes are a (weird) filename and display as-is.
        assert_eq!(path_descriptor_for(r"\\?\C:\x", false), Some(r"\\?\C:\x".to_string()));
    }

    #[test]
    fn unc_remote_host_is_surfaced_not_elided() {
        // The tail reduction would hide the remote host — the single most
        // decision-relevant fact for a network target (red-team, PR #57).
        let d = path_descriptor_for(r"\\evil-srv\share\Users\Alice\f.txt", true).expect("d");
        assert!(d.contains("…/Alice/f.txt"), "{d}");
        assert!(d.contains(r"(network: \\evil-srv)"), "{d}");
        // Forward-slash UNC spelling counts on Windows too.
        let d2 = path_descriptor_for("//evil-srv/share/x", true).expect("d");
        assert!(d2.contains(r"(network: \\evil-srv)"), "{d2}");
        // Verbatim UNC normalizes first, then still surfaces the host.
        let d3 = path_descriptor_for(r"\\?\UNC\evil-srv\share\x", true).expect("d");
        assert!(d3.contains(r"(network: \\evil-srv)"), "{d3}");
        // Unix: `\\evil-srv…` is a local filename, not a network root.
        let d4 = path_descriptor_for(r"\\evil-srv\share\x", false).expect("d");
        assert!(!d4.contains("(network:"), "{d4}");
        // A drive-rooted local path never gets the marker.
        let d5 = path_descriptor_for(r"C:\Users\Alice\f.txt", true).expect("d");
        assert!(!d5.contains("(network:"), "{d5}");
        // Lowercase / mixed-case device-namespace UNC reaches the same network
        // provider (NT object-manager lookups are case-insensitive) — the
        // spelling must not suppress the marker (R-C finding).
        let d6 = path_descriptor_for(r"\\?\unc\evil-srv\share\Users\Alice\f.txt", true)
            .expect("d");
        assert!(d6.contains(r"(network: \\evil-srv)"), "{d6}");
        assert_eq!(strip_verbatim(r"\\.\uNc\srv\share\x"), r"\\srv\share\x");
        // Forward-slash device-prefix spelling normalizes to the device
        // namespace too.
        assert_eq!(strip_verbatim("//?/C:/x"), "C:/x");
        assert_eq!(strip_verbatim(r"//?/unc/srv/share/x"), r"\\srv/share/x");
    }

    #[test]
    fn risk_markers_survive_attacker_length_padding() {
        // A single whole-string cap would let a padded path/host truncate the
        // markers away — they are budgeted separately so both always render
        // in full (R-C finding).
        let host = format!("{}.example.internal", "a".repeat(80));
        let p = format!(r"\\{host}\share\safe\.. \{}\{}", "p".repeat(60), "b".repeat(60));
        let d = path_descriptor_for(&p, true).expect("d");
        assert!(d.contains("(parent traversal)"), "{d}");
        assert!(d.contains(r"(network: \\"), "{d}");
        assert!(d.ends_with(')'), "markers must keep their closing paren: {d}");
        assert!(
            d.chars().count() <= MAX_DESCRIPTOR_CHARS,
            "cap: {} chars",
            d.chars().count()
        );
    }

    #[test]
    fn windows_trailing_dot_space_traversal_is_flagged() {
        // Win32 strips trailing dots/spaces from components, so ".. " escapes
        // to the parent like ".."; the marker must not be dodged by the
        // trailing-junk spelling (red-team, PR #57).
        let d = path_descriptor_for(r"C:\data\.. \x", true).expect("d");
        assert!(d.contains("(parent traversal)"), "{d}");
        let d2 = path_descriptor_for(r"C:\data\...\x", true).expect("d");
        assert!(d2.contains("(parent traversal)"), "{d2}");
        // Unix: ".. " is a literal directory name — no false flag.
        let d3 = path_descriptor_for("/tmp/.. /x", false).expect("d");
        assert!(!d3.contains("(parent traversal)"), "{d3}");
        // "..x" is an ordinary name on both platforms.
        let d4 = path_descriptor_for(r"C:\data\..x\y", true).expect("d");
        assert!(!d4.contains("(parent traversal)"), "{d4}");
    }

    #[test]
    fn windows_trailing_dot_space_is_normalized_for_display() {
        // Win32 strips trailing dots/spaces, so the modal must name the file
        // the OS actually touches, not the padded spelling (R-C finding).
        assert_eq!(
            home_relative_to(r"C:\Users\Alice\payroll.xlsx. ", r"C:\Users\Alice", true),
            Some("~/payroll.xlsx".to_string())
        );
        // A trailing-junk spelling of an intermediate (home) component still
        // resolves inside home — must not look out-of-home.
        assert_eq!(
            home_relative_to(r"C:\Users\Alice. \secret.txt", r"C:\Users\Alice", true),
            Some("~/secret.txt".to_string())
        );
        // Out-of-home: tail components are normalized too.
        assert_eq!(
            path_descriptor_for(r"D:\work\proj. \out.txt. ", true),
            Some("…/proj/out.txt".to_string())
        );
        // Unix does NOT normalize — the trailing space is part of the name.
        assert_eq!(
            home_relative_to_trim("/home/u/file. ", "/home/u", false, false),
            Some("~/file. ".to_string())
        );
        // Verbatim Windows paths bypass Win32 normalization — display verbatim.
        assert_eq!(
            home_relative_to_trim(r"C:\Users\Alice\file. ", r"C:\Users\Alice", true, false),
            Some("~/file. ".to_string())
        );
        assert_eq!(
            path_descriptor_for(r"\\?\C:\Users\Alice\payroll.xlsx. ", true)
                .expect("d")
                .ends_with("payroll.xlsx. "),
            true
        );
        // …but \\.\ (local device) IS canonicalized by Win32, so its trailing
        // junk must be trimmed for display (R-C round 3, verified vs MS docs +
        // Project Zero Win32→NT path analysis).
        let dev = path_descriptor_for(r"\\.\C:\Users\Alice\payroll.xlsx. ", true).expect("d");
        assert!(dev.contains("payroll.xlsx"), "{dev}");
        assert!(!dev.contains("payroll.xlsx. "), "{dev}");
        assert!(!skips_path_normalization(r"\\.\C:\x"));
        assert!(skips_path_normalization(r"\\?\C:\x"));
        assert!(skips_path_normalization("//?/C:/x"));
    }

    #[test]
    fn command_separator_probe_is_platform_aware() {
        // Windows: the backslash makes the token path-shaped — the separator
        // precedes the '=', so the path rules win over value masking.
        assert_eq!(command_descriptor_for(r"C:\dir\x=y", true), Some("…/dir/x=y".to_string()));
        // Unix: no '/' present, so the '='-mask branch applies.
        assert_eq!(
            command_descriptor_for(r"C:\dir\x=y", false),
            Some(r"C:\dir\x=…".to_string())
        );
        // Unix: a backslash-only '='-free token is the literal filename the
        // allow-list judges — shown verbatim (model-supplied; the allow-list
        // rejects separator-bearing tokens on both platforms, so it never runs).
        assert_eq!(
            command_descriptor_for(r"C:\Users\alice\tool.exe", false),
            Some(r"C:\Users\alice\tool.exe".to_string())
        );
        // Windows: the same token goes through the path rules instead.
        assert_eq!(
            command_descriptor_for(r"C:\Users\alice\tool.exe", true),
            Some("…/alice/tool.exe".to_string())
        );
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
        // Verbatim UNC normalizes back to plain UNC (R-C finding).
        assert_eq!(strip_verbatim(r"\\?\UNC\srv\share\x"), r"\\srv\share\x");
        // The stripped form home-relativizes (the un-stripped one would not).
        assert_eq!(
            home_relative_to(
                strip_verbatim(r"\\?\C:\Users\Alice\f.txt").as_ref(),
                r"C:\Users\Alice",
                true
            ),
            Some("~/f.txt".to_string())
        );
        // A UNC home (roaming profile) relativizes through the normalization.
        assert_eq!(
            home_relative_to(
                strip_verbatim(r"\\?\UNC\srv\share\Users\Alice\f.txt").as_ref(),
                r"\\srv\share\Users\Alice",
                true
            ),
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
    fn command_env_assignment_value_is_masked() {
        // `NAME=value` first token: the value side may be a secret (R-C HIGH).
        let s = run_summary(
            "run_command",
            &json!({ "command": "OPENAI_API_KEY=sk-secret-123 ls -la" }),
        );
        assert_eq!(s, "run run_command: OPENAI_API_KEY=…");
        // option=value form is masked too.
        let s2 = run_summary("run_command", &json!({ "command": "--password=hunter2" }));
        assert_eq!(s2, "run run_command: --password=…");
        // env assignment whose value is a path must not fall into path display.
        let s3 = run_summary("run_command", &json!({ "command": "FOO=/usr/bin/x ls" }));
        assert_eq!(s3, "run run_command: FOO=…");
        // any '='-before-separator token is masked, even non-env forms whose
        // value embeds a path (R-C round 2: --path=/home/user/secret).
        let s4 = run_summary("run_command", &json!({ "command": "--path=/home/user/secret ls" }));
        assert_eq!(s4, "run run_command: --path=…");
        let s5 = run_summary("run_command", &json!({ "command": "--token=abc/def" }));
        assert_eq!(s5, "run run_command: --token=…");
        // …but a path merely containing '=' (separator first) gets the path rules.
        let s6 = run_summary("run_command", &json!({ "command": "/tmp/a=b/cmd x" }));
        assert_eq!(s6, "run run_command: …/a=b/cmd");
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
                    let base = *components(p, cfg!(windows)).last().unwrap();
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
        // R-C round 2 additions: CGJ / Mongolian VS / deprecated format /
        // VS supplement / halfwidth Hangul filler.
        let s2 = run_summary(
            "open_app",
            &json!({ "name": "v\u{034F}w\u{180B}x\u{206A}y\u{E0100}z\u{FFA0}" }),
        );
        assert_eq!(s2, "run open_app: v\u{FFFD}w\u{FFFD}x\u{FFFD}y\u{FFFD}z\u{FFFD}");
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
