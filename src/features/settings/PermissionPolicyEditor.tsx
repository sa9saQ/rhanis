// Permission policy editor (rhanis-351). Lets the user curate which folders/URLs
// the AI may touch automatically (許可) and which it must always confirm (禁止),
// layered on top of the built-in SAFE/CAUTION/DANGER gate.
//
// Every committed change persists immediately through the store (no lingering
// "unsaved draft"): text inputs stage a value, and 追加 / 削除 / toggles write the
// whole policy via `savePermissionPolicy`. The backend validates + the
// fail-closed evaluation (baseline + 禁止 always win) does the rest at run time —
// so the worst a bad UI entry can do is fail to relax a gate, never weaken one.

import { useRef, useState } from "react";

import { pickFolder } from "../../lib/tauri/ipc";
import { useSettingsStore } from "./settingsStore";
import { EMPTY_PERMISSION_POLICY, type AllowedFolder, type PermissionPolicy } from "./types";

const ALLOW_ALL_URLS_HINT_ID = "rhanis-allow-all-urls-hint";

/**
 * Normalizes a typed host entry, or returns `null` if it is not a bare host.
 * Mirrors the Rust `host_entry` rejection (no scheme / path / userinfo / port /
 * whitespace) so the UI gives immediate feedback instead of a backend round-trip.
 */
function normalizeHostInput(raw: string): string | null {
  const h = raw.trim().toLowerCase().replace(/\.+$/, "");
  if (h.length === 0 || h.length > 253) return null;
  if (/[/@:\s]/.test(h) || h.includes("://")) return null;
  return h;
}

export function PermissionPolicyEditor() {
  const { settings, savePermissionPolicy } = useSettingsStore();
  const policy = settings?.permission_policy ?? EMPTY_PERMISSION_POLICY;

  const [allowedFolderText, setAllowedFolderText] = useState("");
  const [deniedFolderText, setDeniedFolderText] = useState("");
  const [allowedHostText, setAllowedHostText] = useState("");
  const [deniedHostText, setDeniedHostText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  // Re-entrancy guard: a committed change must finish before the next begins, so
  // two fast clicks can't race on a stale base policy.
  const inFlight = useRef(false);
  // Separate guard for the async folder picker (its dialog runs while saving is
  // still false), so a double-click can't open two dialogs / double-add.
  const picking = useRef(false);

  /**
   * Persists `next`. Returns `true` only if it actually saved — callers clear
   * their input ONLY on `true`, so a dropped (in-flight) or failed commit never
   * silently loses a typed entry. Dropping a deny entry would be a security-
   * relevant false sense of safety, so this must never fail silently (R-C[P2]).
   */
  async function commit(next: PermissionPolicy): Promise<boolean> {
    if (inFlight.current || saving) return false;
    inFlight.current = true;
    setSaving(true);
    setError(null);
    try {
      await savePermissionPolicy(next);
      return true;
    } catch {
      setError("ポリシーの保存に失敗しました。入力を確認してもう一度お試しください。");
      return false;
    } finally {
      inFlight.current = false;
      setSaving(false);
    }
  }

  // Reads the freshest PERSISTED policy at commit time — NOT the render-time
  // `policy` snapshot. The folder picker awaits a dialog (`handlePickFolder`), and
  // another setting can be saved during that await; building `commit` from the
  // stale render snapshot would silently revert it (stale-closure / Zustand
  // setState race — async-react.md). Every mutation merges from `latestPolicy()`
  // and identifies entries by VALUE (not render index) so it stays correct even
  // if the list changed since render.
  function latestPolicy(): PermissionPolicy {
    return useSettingsStore.getState().settings?.permission_policy ?? EMPTY_PERMISSION_POLICY;
  }

  // ---- folder helpers ------------------------------------------------------

  async function addAllowedFolder(path: string) {
    const p = path.trim();
    if (!p) return;
    const cur = latestPolicy();
    if (cur.allowed_folders.some((f) => f.path === p)) {
      setError("そのフォルダはすでに許可リストにあります。");
      return;
    }
    const ok = await commit({
      ...cur,
      allowed_folders: [...cur.allowed_folders, { path: p, allow_danger: false }],
    });
    if (ok) setAllowedFolderText("");
  }

  async function addDeniedFolder(path: string) {
    const p = path.trim();
    if (!p) return;
    const cur = latestPolicy();
    if (cur.denied_folders.includes(p)) {
      setError("その場所はすでに禁止リストにあります。");
      return;
    }
    const ok = await commit({ ...cur, denied_folders: [...cur.denied_folders, p] });
    if (ok) setDeniedFolderText("");
  }

  function removeAllowedFolder(path: string) {
    const cur = latestPolicy();
    void commit({ ...cur, allowed_folders: cur.allowed_folders.filter((f) => f.path !== path) });
  }

  function removeDeniedFolder(path: string) {
    const cur = latestPolicy();
    void commit({ ...cur, denied_folders: cur.denied_folders.filter((d) => d !== path) });
  }

  function toggleAllowDanger(path: string, allow: boolean) {
    const cur = latestPolicy();
    const allowed_folders: AllowedFolder[] = cur.allowed_folders.map((f) =>
      f.path === path ? { ...f, allow_danger: allow } : f,
    );
    void commit({ ...cur, allowed_folders });
  }

  async function handlePickFolder(target: "allowed" | "denied") {
    if (picking.current) return; // a dialog is already open
    picking.current = true;
    setError(null);
    try {
      const path = await pickFolder();
      if (!path) return; // cancelled
      if (target === "allowed") await addAllowedFolder(path);
      else await addDeniedFolder(path);
    } catch {
      setError("フォルダ選択ダイアログを開けませんでした。パスを直接入力してください。");
    } finally {
      picking.current = false;
    }
  }

  // ---- host helpers --------------------------------------------------------

  async function addAllowedHost(raw: string) {
    const h = normalizeHostInput(raw);
    if (!h) {
      setError("ホスト名の形式が正しくありません（例: openai.com）。");
      return;
    }
    const cur = latestPolicy();
    if (cur.allowed_url_hosts.includes(h)) {
      setError("そのドメインはすでに許可リストにあります。");
      return;
    }
    const ok = await commit({ ...cur, allowed_url_hosts: [...cur.allowed_url_hosts, h] });
    if (ok) setAllowedHostText("");
  }

  async function addDeniedHost(raw: string) {
    const h = normalizeHostInput(raw);
    if (!h) {
      setError("ホスト名の形式が正しくありません（例: evil.com）。");
      return;
    }
    const cur = latestPolicy();
    if (cur.denied_url_hosts.includes(h)) {
      setError("そのドメインはすでに禁止リストにあります。");
      return;
    }
    const ok = await commit({ ...cur, denied_url_hosts: [...cur.denied_url_hosts, h] });
    if (ok) setDeniedHostText("");
  }

  function removeAllowedHost(host: string) {
    const cur = latestPolicy();
    void commit({ ...cur, allowed_url_hosts: cur.allowed_url_hosts.filter((x) => x !== host) });
  }

  function removeDeniedHost(host: string) {
    const cur = latestPolicy();
    void commit({ ...cur, denied_url_hosts: cur.denied_url_hosts.filter((x) => x !== host) });
  }

  function toggleAllowAllUrls(allow: boolean) {
    void commit({ ...latestPolicy(), allow_all_urls: allow });
  }

  return (
    <div className="rhanis-policy-editor">
      {error && (
        <p role="alert" aria-live="polite" className="rhanis-settings-error">
          {error}
        </p>
      )}

      {/* 許可フォルダ */}
      <div className="rhanis-policy-group">
        <h4>許可フォルダ（自動で触ってよい場所）</h4>
        <p className="rhanis-settings-hint">
          ここに入れたフォルダの中は、いちいち確認されずに自動で操作されます。削除やコマンドなどの強い操作（DANGER）は、
          フォルダごとに「強い操作も自動」を ON にしたときだけ自動になります。
        </p>
        <ul className="rhanis-policy-list">
          {policy.allowed_folders.map((f, idx) => (
            <li key={`${f.path}-${idx}`} className="rhanis-policy-item">
              <span className="rhanis-policy-path" title={f.path}>
                {f.path}
              </span>
              <label className="rhanis-policy-danger">
                <input
                  type="checkbox"
                  checked={f.allow_danger}
                  disabled={saving}
                  onChange={(e) => toggleAllowDanger(f.path, e.target.checked)}
                />
                <span>強い操作も自動</span>
              </label>
              <button
                type="button"
                className="rhanis-btn rhanis-btn-danger"
                disabled={saving}
                onClick={() => removeAllowedFolder(f.path)}
                aria-label={`許可フォルダを削除: ${f.path}`}
              >
                削除
              </button>
            </li>
          ))}
          {policy.allowed_folders.length === 0 && (
            <li className="rhanis-policy-empty">まだ登録されていません。</li>
          )}
        </ul>
        <div className="rhanis-policy-add">
          <input
            type="text"
            className="rhanis-input"
            placeholder="フォルダのパス（例: /home/user/work）"
            aria-label="許可フォルダのパス"
            value={allowedFolderText}
            disabled={saving}
            onChange={(e) => setAllowedFolderText(e.target.value)}
          />
          <button
            type="button"
            className="rhanis-btn"
            disabled={saving}
            aria-label="許可フォルダを追加"
            onClick={() => void addAllowedFolder(allowedFolderText)}
          >
            追加
          </button>
          <button
            type="button"
            className="rhanis-btn"
            disabled={saving}
            aria-label="許可フォルダをダイアログで選択"
            onClick={() => void handlePickFolder("allowed")}
          >
            フォルダを選択…
          </button>
        </div>
      </div>

      {/* 禁止の場所 */}
      <div className="rhanis-policy-group">
        <h4>禁止の場所（必ず確認する）</h4>
        <p className="rhanis-settings-hint">
          ここに入れた場所は自動では絶対に触らず、必要なときも毎回あなたの確認を求めます。SSH 鍵や認証情報、システムフォルダは、
          ここに入れなくても最初から保護されています。
        </p>
        <ul className="rhanis-policy-list">
          {policy.denied_folders.map((path, idx) => (
            <li key={`${path}-${idx}`} className="rhanis-policy-item">
              <span className="rhanis-policy-path" title={path}>
                {path}
              </span>
              <button
                type="button"
                className="rhanis-btn rhanis-btn-danger"
                disabled={saving}
                onClick={() => removeDeniedFolder(path)}
                aria-label={`禁止の場所を削除: ${path}`}
              >
                削除
              </button>
            </li>
          ))}
          {policy.denied_folders.length === 0 && (
            <li className="rhanis-policy-empty">まだ登録されていません。</li>
          )}
        </ul>
        <div className="rhanis-policy-add">
          <input
            type="text"
            className="rhanis-input"
            placeholder="フォルダのパス（例: /home/user/secret）"
            aria-label="禁止する場所のパス"
            value={deniedFolderText}
            disabled={saving}
            onChange={(e) => setDeniedFolderText(e.target.value)}
          />
          <button
            type="button"
            className="rhanis-btn"
            disabled={saving}
            aria-label="禁止の場所を追加"
            onClick={() => void addDeniedFolder(deniedFolderText)}
          >
            追加
          </button>
          <button
            type="button"
            className="rhanis-btn"
            disabled={saving}
            aria-label="禁止の場所をダイアログで選択"
            onClick={() => void handlePickFolder("denied")}
          >
            フォルダを選択…
          </button>
        </div>
      </div>

      {/* URL ポリシー */}
      <div className="rhanis-policy-group">
        <h4>URL（AI が開いてよいサイト）</h4>
        <p className="rhanis-settings-hint">
          AI が URL を開くときは、許可リストにあるドメインだけ確認なしで開きます。それ以外は毎回確認します。
        </p>

        <label className="rhanis-budget-option">
          <input
            type="checkbox"
            checked={policy.allow_all_urls}
            disabled={saving}
            onChange={(e) => toggleAllowAllUrls(e.target.checked)}
            aria-describedby={ALLOW_ALL_URLS_HINT_ID}
          />
          <span>すべての URL を許可（確認なしで開く）</span>
        </label>
        <p id={ALLOW_ALL_URLS_HINT_ID} className="rhanis-settings-hint rhanis-policy-warning">
          ⚠ ON にすると AI が確認なしに任意の URL を開けます。機密が URL に含まれて外部に送られる可能性があります。
        </p>

        <h5 className="rhanis-policy-subhead">許可ドメイン</h5>
        <ul className="rhanis-policy-list">
          {policy.allowed_url_hosts.map((host, idx) => (
            <li key={`${host}-${idx}`} className="rhanis-policy-item">
              <span className="rhanis-policy-path">{host}</span>
              <button
                type="button"
                className="rhanis-btn rhanis-btn-danger"
                disabled={saving}
                onClick={() => removeAllowedHost(host)}
                aria-label={`許可ドメインを削除: ${host}`}
              >
                削除
              </button>
            </li>
          ))}
          {policy.allowed_url_hosts.length === 0 && (
            <li className="rhanis-policy-empty">まだ登録されていません。</li>
          )}
        </ul>
        <div className="rhanis-policy-add">
          <input
            type="text"
            className="rhanis-input"
            placeholder="ドメイン（例: openai.com）"
            aria-label="許可ドメイン"
            value={allowedHostText}
            disabled={saving}
            onChange={(e) => setAllowedHostText(e.target.value)}
          />
          <button
            type="button"
            className="rhanis-btn"
            disabled={saving}
            aria-label="許可ドメインを追加"
            onClick={() => void addAllowedHost(allowedHostText)}
          >
            追加
          </button>
        </div>

        <h5 className="rhanis-policy-subhead">禁止ドメイン（常に確認）</h5>
        <ul className="rhanis-policy-list">
          {policy.denied_url_hosts.map((host, idx) => (
            <li key={`${host}-${idx}`} className="rhanis-policy-item">
              <span className="rhanis-policy-path">{host}</span>
              <button
                type="button"
                className="rhanis-btn rhanis-btn-danger"
                disabled={saving}
                onClick={() => removeDeniedHost(host)}
                aria-label={`禁止ドメインを削除: ${host}`}
              >
                削除
              </button>
            </li>
          ))}
          {policy.denied_url_hosts.length === 0 && (
            <li className="rhanis-policy-empty">まだ登録されていません。</li>
          )}
        </ul>
        <div className="rhanis-policy-add">
          <input
            type="text"
            className="rhanis-input"
            placeholder="ドメイン（例: evil.com）"
            aria-label="禁止ドメイン"
            value={deniedHostText}
            disabled={saving}
            onChange={(e) => setDeniedHostText(e.target.value)}
          />
          <button
            type="button"
            className="rhanis-btn"
            disabled={saving}
            aria-label="禁止ドメインを追加"
            onClick={() => void addDeniedHost(deniedHostText)}
          >
            追加
          </button>
        </div>
      </div>
    </div>
  );
}
