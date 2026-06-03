# koe - Project Rules

グローバル `~/.claude/CLAUDE.md` を継承する。本ファイルは **koe 固有** のみ記述する（重複ルールは書かない）。

## Project Overview
- **What**: 起動しっぱなしのリアルタイム音声 AI 秘書（デスクトップアプリ）。GPT-Realtime-2 で人と話すように連続会話しながら、裏で PC 操作 / Web 検索 / 記録を実行し、AI が今何をしているかを画面で可視化する
- **中心思想（2026-06-04 研究で確定、SoT=plan §中心思想 + bd epic koe-sua）**: 「校正された透明性（calibrated glass-box）」。考えていること（検証可能な行為＝実行tool/参照source）と校正済みの確信度（実正解率に合わせた3-4段ラベル）を声と画面で開示し、人間が「いつ介入するか」を判断できる。即答ブラックボックスへのアンチテーゼ。既存19製品+学術で「思考の透明化」だけが真空席（製品0・論文0）。詳細 = `~/research/koe-voice-agent-novelty-2026/report.md`
- **Stack**: Tauri 2 + React 19 + TypeScript + Rust + OpenAI Realtime-2 (WebSocket BYOK)
- **Languages**: TypeScript (frontend) + Rust (backend)
- **収益モデル**: M1 = BYOK 単独。M4 = 運営キー主役 + BYOK 退避（2026-06-03 転換、SoT=プラン + bd koe-1mf）:
  - **既定（マス層）**: 運営キー（声=OpenAI/Google を「標準/高品質」の体験ラベルで選択）+ 手足 tool API も運営持ち = 完全ターンキー。**プリペイド残高（前払いチャージ）を基盤**、UI は「残高 ¥◯◯（目安: 約◯分）」と時間併記（時間売り切りは原価 2.5 倍ブレで赤字化のため不可、内部精算はドル/トークン建て）。**月の上限金額キャップ + 自動チャージ**（上限到達で停止 → その場で上限引上げ可、表示通貨は国別）
  - **無料お試し（フリーミアム入口）**: 初回 15〜30 分相当（実会話時間ベース）+ **電話番号（SMS）認証主軸**で捨てアカ対策（クレカは求めない）。「24 時間付与」等は原価 + Sybil で破産リスクのため不可
  - **上級設定（技術層）**: BYOK + アプリ本体有料化（買い切り or ソフト月額）。廃止せず奥に退避
  - 接続層を `RealtimeAuth` enum (`ManagedCredit` 主役 / `Byok` 退避) で抽象化
  - **従量課金（後払い meter）は採用しない**（使った後の高額請求を避ける、前払い消費型）。**月額固定サブスクも採用しない**（音声 API コスト構造で赤字確定のため）
- **対象 OS**: M1 = Windows のみ、M3 で Mac 追加
- **重要な開発環境制約**: WSL ではマイク（cpal）が動作不可。コード作成・cargo test（純粋ロジック）は WSL で OK、音声/E2E は **ネイティブ Windows 必須**

## Plan Reference (SoT)
- 全体設計・マイルストーン・差別化: `~/.claude/plans/virtual-riding-hearth.md`
- 実装着手前は必ずプランを参照する。プランがこのプロジェクトの真実の源

## Architecture

```
WebView (React)         Tauri IPC          Rust backend                OpenAI
─────────────────  ◄──invoke()──►   ─────────────────────   ──WSS───►   GPT-Realtime-2
VoiceButton             emit/listen       session_manager
ActivityLog ★            tool-event       audio_bridge (cpal)
ApprovalModal           approval-req      tool_dispatcher
SettingsPanel                              cost_tracker ★
                                          approval_gate
                                          secret_store (stronghold)
```

- **会話**: GPT-Realtime-2 へ WebSocket 直接接続（BYOK。ephemeral key 方式は採用しない、ローカルにキーがあるため不要）
- **マイク**: **Rust 側 cpal** で取得（WebView の getUserMedia / CSP / AudioWorklet 複雑性を回避）。音声再生は rodio
- **tool 実行**: async tokio task で並行（会話ストリームを止めずに裏作業）
- **可視化 → 透明化**: `app.emit("tool-event", payload)` → frontend `listen()`（Enitar の export-progress と同一構造）。中心思想に伴い `thinking-event`（今考えていること＋検証可能な行為＋校正済み確信度ラベル）を追加（bd koe-sua.1, M1）。ActivityLog は「作業ログ」から「思考の透明性」窓へ昇華（生CoTは出さず検証可能な行為を主軸、レイテンシ300-700msの「思考の窓」で開示）
- **記録**: `RecorderAdapter` trait で差し替え可能（M1=SQLite, M2=Obsidian, M3=Notion）

## Directory Structure

```
src/
  features/
    session/    VoiceButton, SessionStatus, sessionStore
    activity/   ActivityLog, CurrentAction, ApprovalModal, activityStore  ★最重要差別化
    settings/   ApiKeyInput, AdapterSelector, settingsStore
  lib/tauri/ipc.ts   invoke/listen の type-safe wrapper
src-tauri/src/
  lib.rs              Tauri builder + plugin 登録 + SQL migration
  session_manager.rs  WebSocket / Realtime-2 接続、セッション開始/停止
  audio_bridge.rs     cpal マイク → PCM → WS / 音声受信 → rodio 再生
  tool_dispatcher.rs  function_call ルーティング、async tokio task
  approval_gate.rs    DANGER 操作の人間承認 (oneshot 30s timeout, fail-closed)
  cost_tracker.rs     月次予算ハードキャップ ★ 実装済 (14 tests, R-C round 2 通過)
  secret_store.rs     tauri-plugin-stronghold ラッパー（BYOK 用、Enitar 未採用の新規）
  validation.rs       Path traversal 防止（Enitar 流用）+ computer_use 拡張
  observability.rs    Sentry 3 レイヤー（M2）
  tools/    web_search / file_ops / computer_use / recorder
  storage/  adapter.rs (trait) + sqlite.rs (デフォルト)
```

## Project-Specific Rules

### Tauri / Rust 規約
- Tauri commands は `Result<T, String>` 統一（Rust エラーを String 化、frontend で扱いやすく）
- 進捗・イベント push は `app.emit("event-name", payload)` + frontend `listen()`（**Enitar `export.rs` と同パターン**、可視化の背骨）
- Path 操作は必ず `validation.rs` を通す（path traversal 防止、Enitar 流用）
- セキュリティ機能は **fail-closed**: 不明 / エラー / オーバーフロー / タイムアウト時は安全側（= 制限する側）に倒す

### BYOK / シークレット管理（最重要）
- **OpenAI API キーは Rust 側（`tauri-plugin-stronghold` 暗号化保管庫）のみ保持。WebView には絶対に露出させない**
- WebSocket 接続は Rust backend が `Authorization: Bearer <key>` で OpenAI に直接張る
- frontend からは「セッション開始/停止」「使用額取得」等の高レベル invoke だけ。生キーは絶対に往復しない
- ログ / panic message / Tauri event payload にキー値が出ないか PR ごとに確認

### コスト保護の不変条件 (`cost_tracker.rs`、変更時はテスト必須)
- **金額は u64 nanodollars（1 USD = 1e9）** で扱う。f64 は表示のみ
- 累計は `saturating_add` / `saturating_mul`（オーバーフロー時は上限張り付き = fail-closed）
- 月リセットは `month > current_month` かつ妥当な YYYYMM の時のみ（過去月 / 0 / 13 月でリセットしない）
- `BudgetConfig::enabled = false` は「ユーザーが明示的に無制限を選んだ」状態。**初回オンボーディングで「上限設定」 or 「明示的に無制限」の必須選択を設けること（settings UI 層の責務）**
- session_manager は **usage 受信ごとに `is_over_budget()` を確認**し、超過したら進行中セッションを即停止（cost_tracker の R-C round 2 で確認済の他層責務）

### PC 操作の安全ゲート（3 段、fail-closed）
| 危険度 | 操作 | フロー |
|---|---|---|
| SAFE | web_search / read_file (allowlist) / take_screenshot / write_note | 即実行 |
| CAUTION | write_file / open_url / open_app | 実行前通知 |
| DANGER | run_command / delete_file / external_upload | `app.emit("tool-approval-required")` → ApprovalModal → 人間承認（oneshot 30s timeout）→ 拒否なら `"user declined"` を Realtime-2 に返す |

- 書き込み許可ディレクトリ: Documents / Desktop 配下のみ
- シェル: DENY_LIST (rm/del/format/curl/wget/powershell -enc) を先に判定、その後 ALLOW_LIST ホワイトリスト

### Realtime-2 接続
- エンドポイント: `wss://api.openai.com/v1/realtime`（WebSocket 永続接続）
- function calling は side channel イベント (`response.function_call_arguments.done`)
- tool 実行完了後 `conversation.item.create` で結果を返し `response.create` で次応答を促す
- セッションタイムアウト既定 30 分（コスト保護の補助）

### CSP (tauri.conf.json)
- OpenAI Realtime / Bing 等の外部 API は **Rust 側**（`tokio-tungstenite` WebSocket / Rust HTTP client）で接続するため **WebView CSP の対象外**。CSP は WebView 内の fetch/WS にのみ効くので、`connect-src` に OpenAI/Bing を足しても無効（むしろ XSS 時の外向き通信面を広げるだけ）→ **足さない**
- `connect-src` は Tauri IPC のみ（`ipc: http://ipc.localhost`）。`csp` は `null` でなく最小値（`default-src 'self'` 系）にして XSS を防ぐ
- 将来 WebView から直接外部 API を叩く経路を足す場合のみ、そのホストを `connect-src` に追加
- 注: 旧記述は「`connect-src` に wss://api.openai.com 追加必須」だったが、これは WebView 直接接続前提の誤り。koe は Rust cpal マイク + Rust WS（上記アーキテクチャ参照）なので訂正（2026-05-31, Codex R-C + CodeRabbit 指摘で確定）

## Reusable Patterns from Enitar
Enitar (`/home/zsaku/projects/Enitar/`) は同ユーザーの確立済 Tauri+React プロジェクト。以下を直接流用:

| Pattern | Enitar source | koe 使い先 |
|---|---|---|
| emit/listen progress | `src-tauri/src/export.rs` + `src/features/export/services/export.ts` | activity の tool-event ライブ表示 |
| path traversal 防止 | `src-tauri/src/validation.rs` | approval_gate / file_ops / computer_use |
| Tauri builder + SQL migration | `src-tauri/src/lib.rs` | 同パターン + stronghold プラグイン追加 |
| Sentry 3 レイヤー + PII redaction | `src/lib/observability/sentry.ts` 他 | M2 で導入 |

**Enitar との方針差**: API キー管理。Enitar はサーバー集約（Supabase Edge Function）で BYOK 無し。koe は BYOK 必須なので tauri-plugin-stronghold を新規採用（Enitar には無い）。

## Testing
- Rust: `cargo test --manifest-path src-tauri/Cargo.toml`（WSL 可、純粋ロジックの単体テスト）
- Frontend: Vitest 導入済（`pnpm install` 後 `pnpm test`）。typecheck は `./node_modules/.bin/tsc --noEmit`（`npx tsc` は supply-chain-gate hook が `trpc` への typo と誤検知するので直叩き推奨）
- E2E: ネイティブ Windows で `pnpm tauri dev`（音声・WebSocket 込み）
- TDD: 実装ファイル新規作成時は `#[cfg(test)] mod tests` を同ファイルに同梱
- **WSL の ALSA ビルド回避（cpal が libasound に link、Rust ビルド毎に必須）**: `/tmp` はセッション間でクリアされるので毎回再展開 — `dpkg-deb -x ~/projects/koe/libasound2-dev_*.deb /tmp/claude-1000/alsa-dev` + `ln -sf /usr/lib/x86_64-linux-gnu/libasound.so.2.0.0 /tmp/claude-1000/alsa-dev/usr/lib/x86_64-linux-gnu/libasound.so`（dev .deb は `.so` symlink のみ同梱、実体は system runtime にある）。`PKG_CONFIG_PATH=<extract>/pkgconfig` + `RUSTFLAGS="-L<extract>"` を export して `cargo test`。worktree の fresh target は `CARGO_TARGET_DIR` で既存 worktree の target を指すと cold build（数分）を回避できる

## Build & Deploy
- Dev: `pnpm tauri dev`（ネイティブ Windows）
- Build: `pnpm tauri build`（Windows / Mac）
- 配布: M4 で `tauri-plugin-updater` + GitHub Releases + pubkey 署名

## Environment Variables
M1 では `.env` 不使用。API キーはユーザーがアプリ UI で入力 → stronghold へ保管（BYOK 製品方針）。

| Variable | Purpose | Required |
|---|---|---|
| （該当なし、M1） | — | — |

## Branches / Milestones
- `main`: M1 backend 完成（PR #1–#25 merged）。cost_tracker / secret_store（stronghold BYOK + multi-provider 対応 koe-31u）/ activity 可視化 / recorder（SQLite）/ settings + 予算オンボーディング + 声 provider / 手足 tool キー UI / approval_gate（SAFE・CAUTION・DANGER の 3 段 + 同時 pending DANGER 承認 cap = modal-flood guard koe-rxh #23）/ tool_dispatcher（in-flight 上限 DoS guard koe-wj2）/ session_manager（WebSocket + **`RealtimeProvider` trait 抽象化 koe-zv3 #25** + RealtimeAuth + コスト gate）/ audio_bridge（cpal マイク + rodio 再生 + clippy never_loop/absurd 整理 koe-a4h #22）/ M1 tools 4 本 / **permission_policy（許可ポリシー層 koe-351 #29 — 禁止>許可>既定 fail-closed、組み込み baseline 非上書き、承認だけ緩める多層防御、フォルダ/URL CRUD UI）** が実装・マージ済
- **M1 残**: ネイティブ Windows 実機での E2E（`koe-ef8`、依存は全て ✓ で着手可）と hardening（`koe-pr3` audio race / `koe-8kw` read_file の Windows handle walk 等）。WSL ではマイク（cpal）が動かないため E2E は Windows 必須
- **中心思想 epic（2026-06-04 研究 `~/research/koe-voice-agent-novelty-2026/` で確定）**: 「校正された透明性（calibrated glass-box）」。bd epic `koe-sua` + 子 `koe-sua.1`〜`.6`（`.1` thinking-event M1 → `.2` 校正ラベル / `.3` Calibration Memory L4（koe-9ds を3層→4層化）/ `.4` ACIエンジン M2 → `.5` Adaptive Transparency M3 → `.6` idle curator M4・koe-bu1統合）。opt-in flag で段階導入。実験裏付け: E1（開示で熟考の窓+575ms/品質+29pt）/ E2（生confidence直出しは作業ログ以下6.5%<7.1%）/ E5（確信度記憶で約47h使用してAUROC0.59→0.82）/ E6（状態適応透明性がCCC0.64で固定方針に勝つ）。詳細 = plan §中心思想 / report.md
- **製品方向（2026-06）**: multi-provider キー設定基盤 `koe-31u` ✅ merged（#20。声 = OpenAI / Google 選択 + 手足 tool キーの暗号保管 + 設定 UI）→ 接続層 trait 化 `koe-zv3` **PR1 ✅ merged**（#25 `81576bf`。`RealtimeProvider` trait 抽象化 + OpenAI 切り出しの挙動不変リファクタ。realtime_provider.rs 新設 = trait + `ProviderEvent` enum + `RealtimeAuth` 移動 + `OpenAiRealtime` + `select_provider`。333 tests green）→ **次**: `koe-y1j`（PR2 = `GeminiLive` impl + `google/*` 実配線 + audio 16kHz 入力 + fallback chain。Plan の PR2 セクション参照）+ 手足 tool `koe-eal`（x_search 等を tool_dispatcher に追加。`tool_providers` フラグと各プロバイダキーを consume）。許可ポリシー層 `koe-351` ✅ merged（#29 `c1740f9`。フォルダ/URL allowlist + 禁止 denylist + ビジュアル編集。禁止>許可>既定 fail-closed、組み込み baseline 非上書き、IDNA host マッチ、UI→settings_store→`SettingsPolicyProvider`→dispatcher 端まで配線。383 cargo + 187 vitest green）
- **follow-up（review 派生）**: `koe-rxh` ✅ merged（#23, 承認待ち cap = modal-flood + approval-map guard）/ `koe-a4h` ✅ merged（#22, clippy never_loop/absurd）→ `koe-e2b`（P2, koe-rxh の R-C で Codex 捕捉。approval cap は `register()` で効くが dispatch slot 消費は task `spawn` 時点なので、64 DANGER 連射の spawn-burst で一時的に starvation が残る。根本解決 = spawn 前の risk-aware admission（DispatcherSeam に `try_admit`）。**koe-zv3 PR1 完了で着手しやすく**: handle_text が `ProviderEvent::FunctionCall(PendingCall)` 分岐 + typed value なので try_admit を cap check 周辺に挿せる、approval cap は backstop）/ `koe-2br`（P2 bug, koe-zv3 #25 で CodeRabbit Major 捕捉。`response.done` usage 解析失敗時の fail-open を fail-closed 化。既存挙動で「usage なし正常 vs malformed」区別 + koe-ef8 で実 payload 確認後）
- **koe-351 派生 follow-up**: `koe-6as`（P2, 許可フォルダを `validation.rs` の `allowed_bases` に連動 = 承認だけでなく IO 書込境界も拡張、段階導入）/ `koe-gap`（P2, open_url query/userinfo・web_search 経由の exfil 深掘り、open_url 実装時に「外部送信」カテゴリ化）/ `koe-1zw`（P3, policy 再読込の dispatch 毎 load をキャッシュ化）/ `koe-eh4`（P3, model 制御の tool 名が event/approval payload に出る既存挙動の hardening）/ `koe-vxg`（P3, macOS baseline `/private/*`・`~/Library` + Windows case-fold テスト）
- タスクの最新状態は markdown ではなく **bd**（`bd ready` / `bd show <id>`）が真実の源。本節は節目の要約のみに留める

詳細マイルストーンは `~/.claude/plans/virtual-riding-hearth.md` 参照。

## Task Routing for koe
- **Frontend (React features)**: Claude 直
- **コア（session_manager / audio_bridge / tool_dispatcher / approval_gate / secret_store / cost_tracker）**: Hybrid（Claude write → Codex adversarial review、課金・セキュリティ近接のため）
- **autonomous batch（tools 実装・テスト追加）**: Codex MCP 委譲も可


<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:7510c1e2 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->
