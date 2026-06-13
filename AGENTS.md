# Rhanis - Codex Review Instructions

Codex (gpt-5.5) によるレビュー時の **Rhanis 固有** の焦点を記す。グローバル AGENTS.md / `~/.claude/CLAUDE.md` のルールは繰り返さない。

## Project Context
- **What**: 起動しっぱなしのリアルタイム音声 AI 秘書（Tauri デスクトップ）。GPT-Realtime-2 で連続会話しながら、裏で PC 操作 / 検索 / 記録を並行実行し、AI が今何をしているかを画面で可視化する
- **Stack**: Tauri 2 + React 19 + TypeScript + Rust + OpenAI Realtime-2 (WebSocket)
- **収益モデル**: M1 = BYOK、M4 で「BYOK + アプリ有料」と「運営キー + 従量課金プリペイドクレジット」の 2 モードを提供。`RealtimeAuth` enum で接続抽象化
- **Architecture**: WebView (React) ↔ Tauri IPC ↔ Rust backend ↔ OpenAI WSS。**API キーは Rust 側 stronghold のみ**

## Key Risk Areas (このプロジェクトで最も注意すべき領域)
1. **コスト保護の正しさ** — BYOK で高額課金事故 = 信用崩壊。`cost_tracker` の不変条件が崩れると致命
2. **API キー漏洩** — WebView / ログ / panic / Tauri event payload / WebSocket close reason の各経路
3. **PC 操作の暴走** — `computer_use` で誤削除 / 外部送信 / シェルコマンド injection
4. **WebSocket 切断・状態整合** — 会話途中切断時の usage 課金確定 / セッション再開 / 重複カウント
5. **オーディオストリーミング** — PCM フレームの取りこぼし / 重複再生 / 順序逆転
6. **（M4 以降）ManagedCredit モードの運営キー漏洩** — Cloudflare Worker proxy 経路、Stripe meter 残高 race

## Review Focus Areas

### Priority 1（必須チェック）

#### コスト保護 (`src-tauri/src/cost_tracker.rs` + その配線)

| # | チェック項目 |
|---|---|
| 1 | 金額が **u64 nanodollars** で統一されているか（f64 計算混入禁止、表示用 `cost_usd()` 以外） |
| 2 | `add_usage` の月リセットが **「前進かつ妥当な YYYYMM」のみ** か（過去月 / 0 / 13 月 / 1999 年でリセットしないか） |
| 3 | `saturating_*` でオーバーフロー時 fail-closed か（panic 経路ゼロ） |
| 4 | `is_over_budget` が `>=`（上限ちょうど含む）か |
| 5 | `usd_to_nanodollars` が NaN / Inf / 負 / **u64 overflow（1e30 等）** を `None` で弾くか |
| 6 | **配線 P2-b**: session_manager が usage 受信ごとに `is_over_budget()` を呼び、超過したら進行中セッションを即停止しているか（cost_tracker 単体には実装されない他層責務） |
| 7 | **配線 P2-c**: settings UI で「上限設定 / 明示的に無制限」のオンボーディング必須フローが実装されているか（既定 `enabled=false` の保護なし状態を放置しない） |

#### BYOK / API キー保管 (`secret_store.rs` / `session_manager.rs`)

| # | チェック項目 |
|---|---|
| 1 | API キーが **WebView / Tauri command 戻り値 / event payload / ログ / panic message** に**一度も**現れないか |
| 2 | stronghold 読み取りキーがメモリ内で必要最小限の寿命か（`SecretString` + `Drop` で zeroize 推奨） |
| 3 | WebSocket `Authorization: Bearer <key>` の組み立てが Rust 内で完結し、frontend に往復していないか |
| 4 | 設定 UI のキー入力欄が typing 中・保存後にプレーンテキストでアプリ外に漏れないか（show/hide トグル、clipboard 履歴注意） |
| 5 | エラー時の error response に key 値が含まれていないか（OpenAI 側 4xx の `param` フィールド等） |

#### PC 操作の安全ゲート (`approval_gate.rs` / `tools/computer_use.rs` / `validation.rs`)

| # | チェック項目 |
|---|---|
| 1 | **DANGER** 操作（run_command / delete_file / external_upload）が必ず承認モーダル経由か。バイパス経路ゼロ |
| 2 | 承認 oneshot timeout（30 秒）が fail-closed（拒否扱い）か。タイムアウト後に実行されないか |
| 3 | パストラバーサル / シンボリックリンク経由のジェイルブレイクが防げているか（`validation.rs` 流用先全て） |
| 4 | シェルコマンドの DENY_LIST チェック（rm/del/format/curl/wget/powershell -enc）が ALLOW_LIST より**先**に評価されるか |
| 5 | 書き込み可ディレクトリ（Documents/Desktop）の許可範囲が realpath 評価後もスコープを越えていないか |

#### Realtime-2 接続 (`session_manager.rs`)

| # | チェック項目 |
|---|---|
| 1 | WebSocket 切断時の usage 取りこぼし対策（`response.done` 待ちのハンドリング、reconnect 時の重複カウント防止） |
| 2 | function_call 受信から tool 実行 → 結果返却までが **async tokio task で並行**で、会話ストリームをブロックしないか |
| 3 | `conversation.item.create` の `call_id` が対応する function_call の id と正確に一致しているか |
| 4 | セッション開始時の `can_start_session()` チェック（cost_tracker と協調、二重呼び出し race なし） |
| 5 | セッション中の `is_over_budget()` 監視が usage イベントごとに走り、超過で即停止しているか |

### Priority 2

#### Tauri IPC / Rust

| # | チェック項目 |
|---|---|
| 1 | Tauri commands が `Result<T, String>` 統一か（生 panic で UI が落ちないか） |
| 2 | `app.emit` の payload に secret / PII が混入していないか（`ToolEvent` の `description` 等） |
| 3 | `validation.rs` を経由しない `Path::new(user_input).join(...)` の直書きがないか |

#### React (frontend)

| # | チェック項目 |
|---|---|
| 1 | `listen()` で取得した `UnlistenFn` が cleanup されているか（unmount で漏れない） |
| 2 | Zustand store の `subscribe` / `setState` が race condition なし（`async-react.md` 準拠） |
| 3 | `ApprovalModal` 表示中に新規 DANGER tool が来た時の queue / 排他処理が破綻していないか |
| 4 | `ActivityLog` の event 順序保証（timestamp / sequence、out-of-order に対する handling） |
| 5 | エラー / disconnect / over-budget の状態 UI が常に表示されるか（無音失敗にならない） |

### Priority 3（M4 以降、ManagedCredit モード）

| # | チェック項目 |
|---|---|
| 1 | Cloudflare Worker が運営キーを環境変数のみで保持、ログ / エラーレスポンス / WS close reason に絶対露出しないか |
| 2 | Stripe webhook 署名検証が `stripe.webhooks.constructEvent` で実装されているか（`/stripe:webhook-setup` 推奨） |
| 3 | プリペイド残高更新が DB トランザクション + `FOR UPDATE` でレース防止されているか |
| 4 | 残高ゼロ判定が fail-closed（不明・エラー時もクローズ、無料化 bypass なし） |
| 5 | Worker と app 間の `credit_token` が user 紐付け済みで他ユーザー残高を消費できないか（IDOR） |

## Design Decisions（レビュー時の参照）

| Decision | Rationale | Check |
|---|---|---|
| WebSocket 直接接続（ephemeral key 不採用） | BYOK でローカルにキーあり、開発者サーバー不要 | サーバー経由フォールバックが secret 漏洩経路にならないか |
| マイク = Rust cpal（getUserMedia 不採用） | CSP / 権限の複雑性回避、PCM を直接 WS へ | frontend に音声バイト列が漏れていないか |
| 金額 u64 nanodollars | f64 NaN/Inf/丸め誤差で fail-open する事故防止 | 計算経路に f64 混入なし、表示専用 `cost_usd()` のみ |
| 月リセット = 前進かつ妥当 YYYYMM のみ | 過去月入力で累計消去 → 無料化を防ぐ | `past_month_does_not_reset` / `invalid_month_does_not_reset` テストあり |
| `BudgetConfig` 既定 `enabled=false` | ユーザー判断委ねる方針、オンボーディングで明示選択 | settings UI 層の必須フロー実装あり |
| `RecorderAdapter` trait | 記録先（SQLite/Obsidian/Notion）を差し替え可能 | 各実装が trait 契約（async/エラー型）を満たすか |
| `RealtimeAuth` enum (Byok/ManagedCredit) | 2 収益モードを接続層で抽象化 | enum 分岐が完結（漏れ）なく、追加モードで戻り値整合性が壊れていないか |
| 月額固定サブスク不採用 | 音声 API コストで 1 ヘビーユーザー赤字構造 | 設定 UI / 課金 UI に "unlimited monthly" 等の文言が紛れ込んでいないか |

## Known Patterns

| Pattern | Location | Note |
|---|---|---|
| emit/listen tool-event | Rust `app.emit("tool-event", ...)` ↔ TS `listen()` | Enitar `export-progress` 流用、payload は `ToolEvent` 統一型 |
| Approval oneshot | `approval_gate.rs` | `tokio::sync::oneshot` + timeout 30s + fail-closed |
| u64 nanodollars 整数算術 | `cost_tracker.rs` | 比較・累計・上限判定すべて整数、`saturating_*` |
| BYOK / Managed 切替 | `RealtimeAuth` enum | 接続層が 2 モードを完全に隠蔽（呼び出し側は意識しない） |

## Anti-Patterns to Watch

| Anti-Pattern | Risk | What to Check |
|---|---|---|
| f64 を金額判定に再混入 | NaN/Inf fail-open 復活 | cost_tracker 配線箇所で `cost_usd()` 戻り値を比較に使っていないか |
| API キーを Tauri event payload に乗せる | WebView 経由で漏洩 | `app.emit` の payload に key 値ゼロか、`SecretString` の `Display` 経路注意 |
| DANGER 操作の承認をスキップする条件分岐 | computer_use 暴走 | `approval_gate` を経由しない直接実行経路がないか |
| Path 結合に `validation.rs` を通さない | path traversal 復活 | `Path::new(user_input).join(...)` 直書き禁止 |
| WebSocket 再接続で usage を二重カウント | 過大課金 | `event_id` / `response.id` で dedup しているか |
| ManagedCredit で残高未確認のまま OpenAI 転送 | 無料化 bypass | Worker 先頭で残高確認、ゼロなら接続拒否 |

## Spec Reference
- 全体設計 / マイルストーン: `~/.claude/plans/virtual-riding-hearth.md`（収益モデル節含む）
- グローバル AGENTS.md: `~/.claude/AGENTS.md`（あれば）

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
