# Rhanis - Project Rules

グローバル `~/.claude/CLAUDE.md` を継承する。本ファイルは **Rhanis 固有** のみ記述する（重複ルールは書かない）。

## Project Overview
- **製品名（2026-06-13 確定、SoT=`docs/reviews/2026-06-13-product-name-rhanis.md` / `rhanis-0xy`）**: フル製品名 = **Rhanis Agent**（ラニスエージェント、別表記 Rhanis AI）。短縮呼称・音声ウェイクワード = **Rhanis**（"Hey Rhanis"）。由来=ギリシャ神話の水のニンフ Rhanis（水のしずく）、中心思想 glass-box と整合。**旧開発コードネーム「koe」は全廃**（2026-06-13 `rhanis-zs8` 決定で旧「コードネーム継続」案を撤回 → bd prefix・リポジトリ名・内部識別子もすべて Rhanis/rhanis に統一）。実コード反映（productName=`Rhanis Agent` / `mainBinaryName`=`rhanis` / identifier=`com.zsaku.rhanis` / package・crate 名=`rhanis`・`rhanis_lib` / DB=`rhanis.db`）は本リネームで完了。署名 `rhanis-44h`・配布 `rhanis-8h0` は M1.5 でこの識別子を踏襲
- **What**: 起動しっぱなしのリアルタイム音声 AI 秘書（デスクトップアプリ）。GPT-Realtime-2 で人と話すように連続会話しながら、裏で PC 操作 / Web 検索 / 記録を実行し、AI が今何をしているかを画面で可視化する
- **中心思想（2026-06-04 研究で確定、SoT=plan §中心思想 + bd epic rhanis-sua）**: 「校正された透明性（calibrated glass-box）」。考えていること（検証可能な行為＝実行tool/参照source）と校正済みの確信度（実正解率に合わせた3-4段ラベル）を声と画面で開示し、人間が「いつ介入するか」を判断できる。即答ブラックボックスへのアンチテーゼ。既存19製品+学術で「思考の透明化」だけが真空席（製品0・論文0 ※**2026-06-10 スコープ修正**: 無条件「製品0」は FALSE〔Maven AGI が企業CX・内部向けで校正確信度を実装〕→ 真の主張は「**消費者×音声×PC秘書で校正確信度を end user にリアルタイム開示する製品=0**」。`rhanis-20f` / `docs/reviews/2026-06-10-competitive-landscape.md`）。詳細 = `~/research/koe-voice-agent-novelty-2026/report.md`
- **Stack**: Tauri 2 + React 19 + TypeScript + Rust + OpenAI Realtime-2 (WebSocket BYOK)
- **Languages**: TypeScript (frontend) + Rust (backend)
- **収益モデル**: M1 = BYOK 単独。M4 = 運営キー主役 + BYOK 退避（2026-06-03 転換、SoT=プラン + bd rhanis-1mf）:
  - **既定（マス層）**: 運営キー + 手足 tool API も運営持ち = 完全ターンキー。**声＝モデルギャラリー（モデル名+説明+言語ラベル 日/英/両+非エンジニア向けおすすめ+おまかせ自動）で選択**（2026-06-09 更新: 旧「標準/高品質ラベルでモデル名を隠す」を撤回 = `rhanis-45n`。プロバイダは全 API+OSS 対応 `rhanis-7yy`。製品は言語非依存グローバル）。**プリペイド残高（前払いチャージ）を基盤**、UI は「残高 ¥◯◯（目安: 約◯分）」と時間併記（時間売り切りは原価 2.5 倍ブレで赤字化のため不可、内部精算はドル/トークン建て）。**月の上限金額キャップ + 自動チャージ**（上限到達で停止 → その場で上限引上げ可、表示通貨は国別）
  - **無料お試し（フリーミアム入口）**: **任意 — 実施は確定事項ではない**（2026-06-10 確認: session-decisions §5「時間制トライアルは任意」/ rhanis-3x6 note「無くてもよい、user」が正。M4 初期はトライアル無し開始を推奨、その場合 SMS 認証 + Sybil 防御の実装も後回し可）。実施する場合 = 初回 15〜30 分相当（実会話時間ベース）+ **電話番号（SMS）認証主軸**で捨てアカ対策（クレカは求めない）。「24 時間付与」等は原価 + Sybil で破産リスクのため不可
  - **上級設定（技術層）**: BYOK + アプリ本体有料化（買い切り or ソフト月額）。廃止せず奥に退避
  - 接続層を `RealtimeAuth` enum (`ManagedCredit` 主役 / `Byok` 退避) で抽象化
  - **従量課金（後払い meter）は採用しない**（使った後の高額請求を避ける、前払い消費型）。**月額固定（青天井）サブスクは採用しない**（音声 API コスト構造で赤字確定のため）。ただし **月額"クレジット"プラン（従量・繰越・10%ボーナス、Hermes 下敷き）は採用**し、課金は **統一クレジットメーター（前払い残高1本で全有料tool=声/画像/動画/検索/翻訳を計量、自分アカウント OAuth は無料）** に集約。プラン額確定の前に **赤字/採算（P/L）検証が前提**。詳細 = `rhanis-3x6` / `docs/reviews/2026-06-10-session-decisions.md` §5
- **対象 OS**: M1 = Windows のみ、M3 で Mac 追加 → **将来 Windows/Linux/Mac の3OS（user 方針 2026-06-04、`rhanis-cgw`。Tauri は3対応、Linux は cpal/AppImage/署名が追加作業）**
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
- **可視化 → 透明化**: `app.emit("tool-event", payload)` → frontend `listen()`（Enitar の export-progress と同一構造）。中心思想に伴い `thinking-event`（今考えていること＋検証可能な行為＋校正済み確信度ラベル）を追加（bd rhanis-sua.1, M1）。ActivityLog は「作業ログ」から「思考の透明性」窓へ昇華（生CoTは出さず検証可能な行為を主軸、レイテンシ300-700msの「思考の窓」で開示）
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
  session_manager.rs  WebSocket / Realtime-2 接続、セッション開始/停止、自動再接続（指数バックオフ+jitter / reconnecting emit / コスト・予算は再接続跨ぎ保持 / fail-closed、rhanis-byf #44）
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
- 注: 旧記述は「`connect-src` に wss://api.openai.com 追加必須」だったが、これは WebView 直接接続前提の誤り。Rhanis は Rust cpal マイク + Rust WS（上記アーキテクチャ参照）なので訂正（2026-05-31, Codex R-C + CodeRabbit 指摘で確定）

## Reusable Patterns from Enitar
Enitar (`/home/zsaku/projects/Enitar/`) は同ユーザーの確立済 Tauri+React プロジェクト。以下を直接流用:

| Pattern | Enitar source | Rhanis 使い先 |
|---|---|---|
| emit/listen progress | `src-tauri/src/export.rs` + `src/features/export/services/export.ts` | activity の tool-event ライブ表示 |
| path traversal 防止 | `src-tauri/src/validation.rs` | approval_gate / file_ops / computer_use |
| Tauri builder + SQL migration | `src-tauri/src/lib.rs` | 同パターン + stronghold プラグイン追加 |
| Sentry 3 レイヤー + PII redaction | `src/lib/observability/sentry.ts` 他 | M2 で導入 |

**Enitar との方針差**: API キー管理。Enitar はサーバー集約（Supabase Edge Function）で BYOK 無し。Rhanis は BYOK 必須なので tauri-plugin-stronghold を新規採用（Enitar には無い）。

## Testing
- Rust: `cargo test --manifest-path src-tauri/Cargo.toml`（WSL 可、純粋ロジックの単体テスト）
- Frontend: Vitest 導入済（`pnpm install` 後 `pnpm test`）。typecheck は `./node_modules/.bin/tsc --noEmit`（`npx tsc` は supply-chain-gate hook が `trpc` への typo と誤検知するので直叩き推奨）
- E2E: ネイティブ Windows で `pnpm tauri dev`（音声・WebSocket 込み）
- TDD: 実装ファイル新規作成時は `#[cfg(test)] mod tests` を同ファイルに同梱
- **WSL の ALSA ビルド回避（cpal が libasound に link、Rust ビルド毎に必須）**: `/tmp` はセッション間でクリアされるので毎回再展開 — `dpkg-deb -x ~/projects/rhanis/libasound2-dev_*.deb /tmp/claude-1000/alsa-dev` + `ln -sf /usr/lib/x86_64-linux-gnu/libasound.so.2.0.0 /tmp/claude-1000/alsa-dev/usr/lib/x86_64-linux-gnu/libasound.so`（dev .deb は `.so` symlink のみ同梱、実体は system runtime にある）。`PKG_CONFIG_PATH=<extract>/pkgconfig` + `RUSTFLAGS="-L<extract>"` を export して `cargo test`。worktree の fresh target は `CARGO_TARGET_DIR` で既存 worktree の target を指すと cold build（数分）を回避できる

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
- `main`: M1 backend 完成（PR #1–#49 merged）。cost_tracker / secret_store（stronghold BYOK + multi-provider 対応 rhanis-31u）/ activity 可視化 / recorder（SQLite）/ settings + 予算オンボーディング + 声 provider / 手足 tool キー UI / approval_gate（SAFE・CAUTION・DANGER の 3 段 + 同時 pending DANGER 承認 cap = modal-flood guard rhanis-rxh #23）/ tool_dispatcher（in-flight 上限 DoS guard rhanis-wj2）/ session_manager（WebSocket + **`RealtimeProvider` trait 抽象化 rhanis-zv3 #25** + RealtimeAuth + コスト gate）/ audio_bridge（cpal マイク + rodio 再生 + clippy never_loop/absurd 整理 rhanis-a4h #22）/ M1 tools（登録 3 本 = write_note/read_file/take_screenshot。web_search は実装済だが provider 確定 `rhanis-8fw` まで意図的 fail-closed 非登録、設定 UI の dead-end 解消 = `rhanis-25y`）/ **permission_policy（許可ポリシー層 rhanis-351 #29 — 禁止>許可>既定 fail-closed、組み込み baseline 非上書き、承認だけ緩める多層防御、フォルダ/URL CRUD UI）** が実装・マージ済。さらに会話ログ配線 rhanis-emd #34 / session slot 世代 ID rhanis-ego #35 / additive ledger の cross-process atomic 化（BEGIN IMMEDIATE）rhanis-ixt #36 / コスト残高ライブ表示 rhanis-9xi #37 / ApprovalModal a11y rhanis-471 #38 / オンボーディング無スタイル修正 + ダークテーマ統一 rhanis-iyr #39 / input_audio_transcription 有効化 + ASR usage 計上 rhanis-pbe #40 / **CI live 化（GitHub Actions = cargo test + vitest、rhanis-0my）+ pnpm-lock.yaml 実体 commit（rhanis-eco）#41** も merged（CI 詳細は memory `ci-infra-issues-openai-key-claude-action`）。さらに **thinking-event ライブ表示**（考えていること＋検証可能な行為を ActivityLog に開示、中心思想 glass-box M1 の最小実装）rhanis-sua.1 #43 / **WebSocket 自動再接続**（指数バックオフ+jitter+最大試行、reconnecting 表示、コスト・予算を再接続跨ぎ保持、fail-closed）rhanis-byf #44 も merged（#43/#44 は Windows 実機 E2E `rhanis-ef8` 未検のため in_progress 据え置き、下記「M1 残」参照）。**2026-06-09 追加マージ**: #45 docs-mem sync / 予算cap parity 修正（極小正USD<5e-10 が round で 0 nanodollars→enabled+limit=0 を builder/validator parity で fail-closed 拒否、rhanis-he8 #46）/ **Windows E2E smoke CI**（tauri-driver+wdio、`rhanis-ef8` インフラ。CI に 'E2E smoke (Windows)' job 追加、#47）/ settings_store.save の電源断 durability（content fsync + save_lock 直列化、rhanis-6ee #48。Windows write-through durable rename は follow-up `rhanis-z2f`=depends `rhanis-ef8` に分離）/ 会話ログ overflow drop fail-soft 回帰テスト（rhanis-a4f #49）も merged
- **M1 残（2026-06-04 再定義 → 2026-06-07 更新）**: backend コア + CI は完成。**M1 完成の gating item = E2E `rhanis-ef8`（ネイティブ Windows 実機。WSL ではマイク cpal 不可）**。**2026-06-10 更新: ef8 は実質 unblocked（着手順 = `rhanis-5fs` 簡約版 1 行 → ef8〔実 wire frame 採取で `rhanis-bd7`/`rhanis-nal`/`rhanis-2br` を fixture 化〕→ 解禁後 `rhanis-ds6` ウェーブ）+ barge-in 最小 `rhanis-bx7` を M1 必須扱いに追加（user 承認。詳細 = 下記 2026-06-10 徹底レビュー bullet）**。会話ログ配線 `rhanis-emd`（#34）/ ASR `rhanis-pbe`（#40）/ thinking-event `rhanis-sua.1`（#43）/ WebSocket 自動再接続 `rhanis-byf`（#44）は **コードは merged だが Windows 実機 E2E 未検証のため in_progress 据え置き**（「test PASS を完了と詐称しない」= `rhanis-ef8` で実フロー verify 後に一括 close。bd in_progress の 4 件はこれら）。残り（open）: `rhanis-5sc`（IPC parity test）/ `rhanis-8t2`（マイク権限 UX）/ `rhanis-30t`（初回チュートリアル）。Windows 実機 hardening（`rhanis-ef8` 系）: `rhanis-pr3`（audio race）/ `rhanis-8kw`（read_file handle walk）/ `rhanis-2br`（usage payload 確定→fail-closed 化）。**完了済（参考）**: `rhanis-9xi` #37 / `rhanis-iyr` #39 / `rhanis-471` #38 / `rhanis-0my`+`rhanis-eco` #41 / `rhanis-he8` #46（予算cap parity）/ `rhanis-6ee` #48（settings fsync）/ `rhanis-a4f` #49（journal overflow test）。**新規 follow-up**: `rhanis-z2f`（Windows MoveFileExW WRITE_THROUGH durable rename、depends `rhanis-ef8`）。**#47 で Windows E2E smoke CI 着地**（rhanis-ef8 のインフラ前進、ただし rhanis-ef8 本体 = P0 OPEN は未完）。残タスクの最新は必ず `bd ready` 参照（本節は節目要約）
- **2026-06-04 徹底レビュー（全視点監査）**: 新規 22 issue 起票（label `review-2026-06-04`、`bd list --label review-2026-06-04`）+ 依存衛生（`rhanis-zv3` close=PR1 merged・PR2 は `rhanis-y1j`）。所見=安全コアは業界水準超だが製品層（常駐 `rhanis-944`/通知 `rhanis-hah`/配布署名 `rhanis-8h0`/規約・録音同意 `rhanis-n6s`/observability `rhanis-3ai`/履歴 UI `rhanis-sh6`/データ削除 `rhanis-0k1`）が空。中心思想は土台のみ実装ゼロ、最大リスク=校正の信号源未定義（`rhanis-1r1` を `rhanis-sua.2/.3` 前提に起票）。**更新 2026-06-08: 再接続 `rhanis-byf`（#44）と中心思想 thinking-event `rhanis-sua.1`（#43）は実装 merged 済（共に E2E `rhanis-ef8` 待ちで in_progress）→ 上記『製品層が空』『実装ゼロ』はレビュー時点の記述、現状は両者着手済**。手足tool 実装前の risk tier 再設計 `rhanis-p1a`（open_app/write_file）。**監査レポート全文 = `docs/reviews/2026-06-04-comprehensive-review.md`**
- **中心思想 epic（2026-06-04 研究 `~/research/koe-voice-agent-novelty-2026/` で確定）**: 「校正された透明性（calibrated glass-box）」。bd epic `rhanis-sua` + 子 `rhanis-sua.1`〜`.6`（`.1` thinking-event M1 ✅ merged #43〔`4afaa65`、実機 E2E `rhanis-ef8` 待ちで in_progress〕 → `.2` 校正ラベル / `.3` Calibration Memory L4（rhanis-9ds を3層→4層化）/ `.4` ACIエンジン M2 → `.5` Adaptive Transparency M3 → `.6` idle curator M4・rhanis-bu1統合）。opt-in flag で段階導入。実験裏付け: E1（開示で熟考の窓+575ms/品質+29pt）/ E2（生confidence直出しは作業ログ以下6.5%<7.1%）/ E5（確信度記憶で約47h使用してAUROC0.59→0.82）/ E6（状態適応透明性がCCC0.64で固定方針に勝つ）。詳細 = plan §中心思想 / report.md
- **製品方向（2026-06）**: multi-provider キー設定基盤 `rhanis-31u` ✅ merged（#20。声 = OpenAI / Google 選択 + 手足 tool キーの暗号保管 + 設定 UI）→ 接続層 trait 化 `rhanis-zv3` **PR1 ✅ merged**（#25 `81576bf`。`RealtimeProvider` trait 抽象化 + OpenAI 切り出しの挙動不変リファクタ。realtime_provider.rs 新設 = trait + `ProviderEvent` enum + `RealtimeAuth` 移動 + `OpenAiRealtime` + `select_provider`。333 tests green）→ **次**: `rhanis-y1j`（PR2 = `GeminiLive` impl + `google/*` 実配線 + audio 16kHz 入力 + fallback chain。Plan の PR2 セクション参照）+ 手足 tool `rhanis-eal`（x_search 等を tool_dispatcher に追加。`tool_providers` フラグと各プロバイダキーを consume）。許可ポリシー層 `rhanis-351` ✅ merged（#29 `c1740f9`。フォルダ/URL allowlist + 禁止 denylist + ビジュアル編集。禁止>許可>既定 fail-closed、組み込み baseline 非上書き、IDNA host マッチ、UI→settings_store→`SettingsPolicyProvider`→dispatcher 端まで配線。383 cargo + 187 vitest green）
- **follow-up（review 派生）**: `rhanis-rxh` ✅ merged（#23, 承認待ち cap = modal-flood + approval-map guard）/ `rhanis-a4h` ✅ merged（#22, clippy never_loop/absurd）→ `rhanis-e2b`（P2, rhanis-rxh の R-C で Codex 捕捉。approval cap は `register()` で効くが dispatch slot 消費は task `spawn` 時点なので、64 DANGER 連射の spawn-burst で一時的に starvation が残る。根本解決 = spawn 前の risk-aware admission（DispatcherSeam に `try_admit`）。**rhanis-zv3 PR1 完了で着手しやすく**: handle_text が `ProviderEvent::FunctionCall(PendingCall)` 分岐 + typed value なので try_admit を cap check 周辺に挿せる、approval cap は backstop）/ `rhanis-2br`（P2 bug, rhanis-zv3 #25 で CodeRabbit Major 捕捉。`response.done` usage 解析失敗時の fail-open を fail-closed 化。既存挙動で「usage なし正常 vs malformed」区別 + rhanis-ef8 で実 payload 確認後）
- **rhanis-351 派生 follow-up**: `rhanis-6as`（P2, 許可フォルダを `validation.rs` の `allowed_bases` に連動 = 承認だけでなく IO 書込境界も拡張、段階導入）/ `rhanis-gap`（P2, open_url query/userinfo・web_search 経由の exfil 深掘り、open_url 実装時に「外部送信」カテゴリ化）/ `rhanis-1zw`（P3, policy 再読込の dispatch 毎 load をキャッシュ化）/ `rhanis-eh4`（P3, model 制御の tool 名が event/approval payload に出る既存挙動の hardening）/ `rhanis-vxg`（P3, macOS baseline `/private/*`・`~/Library` + Windows case-fold テスト）
- **2026-06-04 技術スカウト（最新技術を Rhanis に）**: 研究 `~/research/koe-integration-tech-2026-06/report.md`（7次元+定量実験）。**自前 realtime 音声の答え = Qwen3.5-Omni（2026-03-30, Apache 2.0, semantic interruption=barge-in / tool / voice clone / realtime / 音声生成36言語に日本語）一択**（Gemma4+Miso=英語不可 / cascaded=847ms>700ms で却下）。経済性=自前 H100 で OpenAI Realtime の **10-36x 安**、損益分岐 月42-100h。中心思想シナジー=自前なら隠れ状態で SEP（校正信号）が取れる（BYOK API 不可）。**ただし全て M2以降 or 将来（`rhanis-aja` post-M1、学習不要・そのまま動く、日本語視聴は着手時の最初の spike）で M1 は変更なし**。STT/TTS=Deepgram Nova-3+Qwen3-TTS/ASR（`rhanis-eru`）/ 手足=MCP client 化（`rhanis-eal`+`rhanis-dcj`）/ メモリ=Zep bi-temporal+Letta（`rhanis-9ds`）/ 校正=conformal/ACI（`rhanis-sua.2/.4`/`rhanis-1r1`）。session memory=`rhanis-2026-06-04-tech-scout-session` / `rhanis-selfhosted-voice-vision`
- タスクの最新状態は markdown ではなく **bd**（`bd ready` / `bd show <id>`）が真実の源。本節は節目の要約のみに留める

- **2026-06-09 UX/動線 根本原因レビュー + デザイン全面リデザイン方針**: E2E 実機(Windows)で観測した UX 不良(保存/起動が遅い・「準備中」固着・完成度)を Dynamic Workflow(27 エージェント、19 確定/4 却下、敵対検証済)で根因特定。**遅さ(症状1/2/3)の主犯 = stronghold が全 open/save で age scrypt(work_factor=19, ~1s/回)を回す → 起動時 `try_set_encrypt_work_factor(0)`(Rhanis 鍵は 32byte CSPRNG 強鍵で安全、後方互換)= `rhanis-ds6`(P0)**。派生 = `rhanis-nt2`(has 冗長排除 + spawn_blocking)。**「準備中」固着(症状4)** = establish_connection に connect timeout 欠如(`rhanis-9wb` backend)+ sessionStore loading に脱出口欠如(`rhanis-5fs` frontend)。`gpt-realtime-2` は**実在の正しいモデル名**(2026-05-07 リリース、格下げ禁止 — bd memory `rhanis-gpt-realtime-2-is-real`)。**ユーザー方針でデザイン全面リデザイン = 没入型 orb + OS追従配色(`rhanis-ios` epic)**(※ **2026-06-10 に「見える glass-box コンソール + 音声主役」へ pivot 済**、下記 2026-06-10 bullet / `docs/design/2026-06-10-glassbox-console-design-brief.md` 参照。orb は縮小した音声状態インジケータに格下げ)、プロンプト全文(旧・superseded) = `docs/design/2026-06-09-immersive-orb-design-brief.md`。bd label `review-2026-06-09`、着手順 = rhanis-ds6 → rhanis-nt2 → (rhanis-9wb + rhanis-5fs) → rhanis-ios(デザイン生成後)。レポート全文 = `docs/reviews/2026-06-09-ux-rootcause-review.md`

- **2026-06-09 ビジョン拡張（ユーザー確定、競合研究 Codex App/Hermes Desktop 起点）**: ①声のコクピット＝全機能を抱えず tool/MCP/エージェント委譲で「声で何でも」(コードも Codex 劣化版にせず `rhanis-eal` 上に code tool)。②**グローバル多言語**(JP-first 撤回、英語圏含め販売、言語非依存)。③**モデル選択を見せる** `rhanis-45n`(名前+説明+言語 日/英/両+非エンジニア向けおすすめ+おまかせ自動)＝**M4「モデル名を隠す」決定を撤回**。④**全プロバイダ対応** `rhanis-7yy`(trait `rhanis-zv3` 段階導入: API=OpenAI/Gemini/Nova Sonic/Grok Voice/InWorld、OSS=Qwen3.5-Omni/Moshi/J-Moshi/PersonaPlex/Nemotron)。⑤OSS提供+課金 `rhanis-5ed`(前払い残高1本=`rhanis-1mf`一致、rhanis-hosted=規模後/on-device Moshi級=無料完全ローカル)。⑥**視覚グラウンディング** epic `rhanis-jhk`(注釈オーバーレイ『指して話す』`rhanis-jhk.1`/ライブ画面共有`rhanis-jhk.2`/視覚指示→computer_use`rhanis-jhk.3`)＝裏取り: OpenAI Realtime 画像入力+Gemini Live 画面共有対応。⑦外出先 `rhanis-pj1`(OpenClaw 方式 Discord/LINE bot+VC 音声, 電話 SIP/スマホアプリ M4+)。検証: computer_use は OSWorld 2026 で 38–79%=不安定→透明性+DANGER 承認が人間保険。bd label `vision-2026-06-09`、bd memory `rhanis-2026-06-09-vision-expansion`、レポート = `docs/reviews/2026-06-09-competitor-design-research.md`

- **2026-06-10 セッション: デザイン pivot + 課金全面設計 + Hermes 取捨（記録 = `docs/reviews/2026-06-10-session-decisions.md`、bd memory `rhanis-2026-06-10-session-decisions`）**: ①**デザイン pivot**: 没入 orb 撤回 → 「見える glass-box コンソール + 音声主役」(`docs/design/2026-06-10-glassbox-console-design-brief.md` が現行の正、旧 orb brief superseded、`rhanis-ios` タイトル更新済)。確信度=既定非表示・低確信×重大時のみ具体的注意(`rhanis-sua.2`)。②**課金**: 統一クレジットメーター(前払い残高1本で全有料tool=声/画像/動画/検索/翻訳、自分アカウント OAuth 無料/BYOK 自分原価) + Hermes 下敷きプラン(Hermes 実名は Free/Plus/**Super**/Ultra — 旧記録の「Pro」は誤記、2026-06-10 徹底レビューで公式 X 発表により確認。+10%ボーナス+繰越+追加購入) `rhanis-3x6`。経済性=API中庸/潤沢化は自前ホスト後日(`rhanis-aja`)。**要検証=赤字/採算(P/L)をプラン額確定前に**。③**機能/Hermes取捨**: 消費者手足パック+OAuth『接続』ボタン(キー入力なし) `rhanis-v5i`、設定 SIMPLE DEFAULT(非エンジニアは6グループ、dev/難設定は Advanced/自動管理) `rhanis-0yq`。④**翻訳手足** `rhanis-d9t`(音声/動画/生放送+文書、アプリ内字幕も)、**CLI/MCP も使える(既定非表示=削除でなく Advanced)**。⑤オンボ=ログイン壁なし最初からデスクトップ `rhanis-30t`。⑥**rhanis-ds6**(P0 起動高速化): 真因検証済(IOTA Stronghold=age scrypt WF~20、修正=`iota_stronghold::engine::snapshot::try_set_encrypt_work_factor(0)`、32byte CSPRNG 強鍵で安全/後方互換)、**実装は明示指示まで保留**。**教訓: 『お願いします』≠実装着手、明示『実装して』まで code 触らない**。

- **2026-06-10 競合地図 + 勝ち筋（記録 = `docs/reviews/2026-06-10-competitive-landscape.md` / research 12-16、bd memory `rhanis-2026-06-10-competitive-landscape`、戦略 `rhanis-20f`）**: ①**音声 + 常駐 + PC操作は table-stakes 化**（Copilot/Gemini/Siri/Alexa+/Perplexity/ChatGPT/Hermes 全社）= 「話せる」「3段承認ゲート」を差別化に使わない。②**最大脅威 = Microsoft Copilot（Windows = M1 surface に OS ネイティブ）**、Hermes/OpenAI でない（OpenAI の Mac voice 撤退は Windows 継続で M1 を救わない）。直接競合 = Simular Sai（最接近 startup・最重要 watch）/ Gemini Spark / Perplexity Personal Computer / Claude Cowork。③4軸採点で **end-user 向け校正 glass-box（axis3）= 全社0 = Rhanis 唯一の耐久ウェッジ**（大手は魔法UXを自壊させるため構造的に出しにくい ※精緻化 2026-06-10 徹底レビュー: 正確には「出しても使われなかったので再投資しない」〔Google double-check 前例〕、単独ウェッジでなく複合体へ — 下記 徹底レビュー bullet ③参照）。④**勝ち筋の思想 = 能力（レンタル・コモディティ）でなく信頼（積み上がる・奪えない）で戦う**。Calibration Memory（使うほど校正が正確化＝奪えない蓄積）+ first-mover window で最速 + 作り手=プロダクトの透明性一致（build-in-public）+ provider中立/ローカル。⑤watch = Simular Sai / Confidence UI pattern catalog。

- **2026-06-10 徹底レビュー（全文 = `docs/reviews/2026-06-10-exhaustive-review.md`、新規 19 issue = `bd list --label review-2026-06-10`、bd memory `rhanis-2026-06-10-exhaustive-review`。検証 = 自照合 + Codex 別 provider、refuted 0）+ user 決定 2 件（2026-06-10 承認）**: ①**M1 動線の結論と着手順（user 記載指示）**: 動線は entry→core→output まで正しく配線済み（不一致 0、skeleton は意図的 2 箇所のみ）。M1 の残りは実装でなく**検証 1 本** — `rhanis-ef8` は実質 unblocked（blocker の emd/pbe は merged 済で close 条件が ef8 自身 = 検証の循環参照、bd の blocked 表示は形式）。**着手順 = `rhanis-5fs` 簡約版（loading 中も stopSession を通す 1 行。backend は connect を select! で stop と競争済 = `session_manager.rs:1090-1100`、frontend `sessionStore.ts:188` が唯一のブロッカー）→ `rhanis-ef8` 完走（実 wire frame 採取で `rhanis-bd7` GA 音声名無音 / `rhanis-nal` server error 黙殺 / `rhanis-2br` usage fail-open を確定 → fixture 化）→（解禁後）`rhanis-ds6` ウェーブ（E2E 1 周目 = before 計測）**。②**table-stakes + 校正体験の採用（user 承認「入れといて」）**: barge-in 最小実装 `rhanis-bx7`（speech_started→再生即停止+response.cancel、対 = `rhanis-z8j`）を **M1 必須扱い**、常駐 `rhanis-944` を M1.5（配布可能な製品 = 署名 `rhanis-44h` + 法務 n6s + オンボ 30t）へ前倒し、校正体験 3 点 = ワンタップ訂正 `rhanis-1l4` / 元に戻す `rhanis-nak` / 正直レポート `rhanis-84w` を**採用確定**。収益は骨格（前払いクレジット 1 本 + 月額クレジットプラン、青天井/後払い不採用）を維持し、プラン額確定時に P/L ゲート `rhanis-krv`（メーター = 実コスト×1.8-2.0 パススルー型 — rhanis-1mf 旧記述 +15-25% は赤字設計 / 安価既定ロック / 全クレジット 180 日失効 = 資金決済法適用除外）を通す。**決済 = Stripe 確定（user 2026-06-10、記録済み既定どおり。レビューの Polar 提案は撤回 — Stripe JP は固定手数料なしで ¥500 チャージも手数料上成立。EU の VAT 義務のみ課題 = 当面 EU 販売制限 or Stripe Managed Payments〔Stripe 自身の MoR、2026-02 preview〕を M4 設計時に確認）。無料お試し = 「任意」の既決定が正（session-decisions §5 / rhanis-3x6 note「時間制トライアルは無くてもよい、user」）— M4 初期はトライアル無し開始を推奨（SMS 認証 + Sybil 防御の実装が後回しになり M4 が軽くなる）**。③**戦略 stress-test（レビュー結論、適用は `rhanis-20f` note）**: 校正 glass-box 単独ウェッジは機能の堀としては弱い（見せかけ確信度 UI = confidence theater の氾濫が真の脅威）→ 防御は複合体（provider 中立+ローカル主権〔MS が定義上模倣不能〕× 校正品質の実行 × 作り手の真正性）=「信頼の主権」。収益本体の非エンジニア層への見出しは体感語「見える・止められる・成績表がある」（glass-box 見出しは技術層/プレス向け）。分離装置 = 正直レポート `rhanis-84w`、`rhanis-1r1` の計測開始前倒しが戦略最優先。初収益 = Founder's License 橋（ef8 後 5-7 週、要 `rhanis-bup` 決定）→ M4 本線（GA 120-150 日）、M1.5「配布可能な製品」= 署名 `rhanis-44h`（カレンダー律速・日本個人は Azure Public Trust 不可）+ n6s + 944 + 30t を新設。

- **2026-06-11: モデル別タスク分割 + 次セッション E2E 実務ライン確定（bd memory `rhanis-next-session-e2e-line` / `rhanis-2026-06-10-fable5-opus-task-split`）**: 〜2026-06-22 の Claude Fable5 期間枠を活用するため、open+in_progress を 3 ラベルに分類（`model-fable5`:51 / `model-opus`:50 / `model-onhw`:11。Dynamic Workflow 4 レンズの敵対検証で誤分類 5 件補正済み）。ループ = Opus 枠 `~/.claude/loops/rhanis-loop.md`（`bd ready`）/ Fable5 枠 `~/.claude/loops/rhanis-fable5-loop.md`（`bd ready --label model-fable5`、`/model claude-fable-5` で起動）。**`rhanis-1r1` 分割**: 設計（確信度入力源 / outcome 観測経路 / カテゴリ定義の文書化 = 実機不要 fable5、ef8 依存を外し ready 化）と実測 validation（`rhanis-508` = onhw、depends `rhanis-1r1`+`rhanis-ef8`）。**`rhanis-ios`→opus**（ブリーフ `docs/design/2026-06-10-glassbox-console-design-brief.md` 承認済み = 実装フェーズ）+ 骨格子 `rhanis-ios.1`（glass-box コンソール最小骨格、E2E 用、model-opus）。**次セッション = E2E 実務ライン（Opus 4.8、ループで回す）。着手順 = `rhanis-5fs`（準備中 loading 固着の脱出口 = `sessionStore.ts:188` から `||status==="loading"` を外し loading 中も stopSession 許可 + コメント更新 + テスト。backend は connect を select! で stop と競争済 = `session_manager.rs:1090-1100`）→ `rhanis-ios.1`（骨格、ブリーフ忠実）→ `rhanis-ef8`（実機 E2E、ユーザー Windows）**。

- **2026-06-11 ループ周回成果（/loop × Fable5、周回 1-3）**: ① **PR #54 merged** = `rhanis-ios.1` glass-box コンソール骨格（`ConsoleLayout` 新設 = 折りたたみサイドバー〔新しい会話 = startSession 実配線 + 近日追加リスト + CostHeader/設定〕+ 状態連動 greeting + ActivityLog ヒーロー + VoiceButton 96px orb。OS 追従ライト/ダーク + ink トークン `--on-accent/--on-warn/--on-danger`。over-budget 可視性は `sidebarOpen || overBudget` 派生値で fail-closed。`rhanis-zea` unblock、follow-up 3 件 = DevMockEmitter dynamic import / orb アニメ compositor 化 / ipc mock 共有 factory）② **PR #55 merged** = `rhanis-bx7` barge-in 最小実装（`ProviderEvent::SpeechStarted` + trait `cancel_frame()` / 非 terminal `AudioControl::ClearPlayback` / **lock-free `PlaybackHandle` 3 状態 gate** = 発話中の response.created では開かない〔tool 完走の talk-over 防止〕/ cancel 2 段配送 = try_send → Full 時 1s freshness bound 付き parked task。cargo 468 green。**実機挙動は ef8 待ちで in_progress 据え置き**。follow-up: `rhanis-460` targeted cancel / `rhanis-2e7` truncate）③ **PR #56** = `rhanis-1r1` 校正信号源の設計文書 `docs/design/2026-06-11-calibration-signal-sources.md`（D1 実行層 Beta / **D2 意味層 = 打ち切り付き訂正レート**〔negative-only Beta の退化を閉鎖〕/ D3 束ね + no-semantic-evidence gate / D4 提示は sua.2 確定に従属 / S1-S6 台帳 + S1 汚染防止 / カテゴリ列分解 + 予算分離 / C5 farming cap / C8 schema 強制。R-B 2round + Codex R-C 2round で LGTM。**sua.2/.3 の前提 — user 承認で rhanis-1r1 close**）。**運用学び**: R-B は `Skill("review-loop")` 経由必須（Agent 直起動は merge gate 証跡にならない）/ `cr review` は `--base origin/main` 必須（local main が stale）/ **bd jsonl を PR に同梱すると post-checkout clobber が止まる（恒久回避成立）**

- **2026-06-12 ループ周回（/loop × Fable5、周回 4）**: **PR #57 open（bot review 待ち）** = `rhanis-whf` 安全な対象記述子（`display_descriptor.rs` 新設 — DANGER 承認モーダル + ActivityLog に home 相対 path / コマンド先頭トークン / URL host+非default port を開示。敵対文字は U+FFFD 置換 = 改竄可視化、`(parent traversal)` 非省略マーカー、`policy_target` parity test で表示/policy 判定の乖離を lock、`=`-先行トークンは `NAME=…` で値 mask、`\\?\`/`\\?\UNC\` 正規化、ApprovalModal は `<code>` data 表示。**gate 判定経路は不変 = 表示専用**）。R-B 3視点 C0/H0 全修正 + 収束 CONVERGED / cr review No findings / Codex R-C 3 round LGTM。cargo 497 green。**セキュリティ近接 = 重要 PR → bot 解消後に次セッションでマージ**（gate 証跡 TTL 切れのため収束確認パス再実行が必要、手順 = bd `rhanis-whf` note）。follow-up: ActivityLog の data 表示 mirror（新 issue）/ `rhanis-eh4`・`rhanis-p1a` に note。**運用学び: 引数なし `bd export` は stdout に吐くだけでファイル未更新 — jsonl 同梱は「commit（hook が main へ export）→ cp → amend」の順が正**

- **2026-06-12 ループ周回（/loop、周回 1）**: **PR #59 merged** = `rhanis-9wb` connect timeout（`ReconnectConfig.connect_timeout`=live 15s で supervisor の (re)connect を `tokio::time::timeout` 包み → `Elapsed`→`Recoverable("connection timeout")`→既存 backoff/max_attempts(6)/max_total(20)→fail-closed。`master_shutdown` レースは別 select arm で維持。**症状4「準備中」固着の backend 根治**、frontend 脱出口 `rhanis-5fs` #53 の対）。新テスト `supervisor_fails_closed_on_connect_timeout`。cargo 505 green。R-B review-loop PASS(C0/H0) / cr No findings / R-C Codex LGTM / CodeRabbit No actionable / CI 4/4 緑で自律マージ。**`rhanis-9wb` は `rhanis-byf` 先例どおり merged 後も in_progress 据え置き**＝real-hang（実 TLS/proxy blackhole で「準備中」を脱出するか）の体感確認は `rhanis-ef8` Windows 実機 E2E wave で一括 close（in_progress は計 7 件＝1r1/byf/bx7/sua.1/emd/pbe/9wb、全て code+全レビュー済で ef8 待ち据え置きが正）。`rhanis-bd7`（GA 音声名 `response.output_audio.delta` 両対応）は **PR #60 で実装済み**（2026-06-12 UTC、`8dc014e`。`is_audio_delta_type` 述語で decode / barge-in gate / parse_frame の 3 箇所を統一 + テスト 3 本、cargo 508 green。R-B PASS C0/H0 / cr No findings / R-C Codex LGTM。merge 後も先例どおり実 wire 名確定 = `rhanis-ef8` まで in_progress 据え置き予定、bd memory `rhanis-bd7-impl-plan`）。**運用学び: 新モデル枠分け（model-fable5/opus）は廃止済で `bd ready` 全件から難度判定して拾う。新スキル `windows-e2e-bridge`（WSL→Windows 実機）が ef8 系を前進させ得るので評価対象**

- **2026-06-12 UTC ループ周回（/loop、周回 1-6）**: **PR #60 merged** = `rhanis-bd7` GA 音声名両対応（`is_audio_delta_type` 述語で decode / barge-in gate / parse_frame の 3 箇所統一 + テスト 3 本。GA wire での assistant 無音リスク解消）。**PR #61 merged** = `rhanis-nal` server error frame の表面化（`ProviderEvent::ServerError` 正規化 = key マスク + U+FFFD 衛生 + cap の 3 段防御 / barge-in cancel noise の narrow benign 分類 = once-per-connection latch で stderr のみ / 新 channel `provider-error` → activityStore（cap 20・dedup・staleness guard 2 重 = backend generation-gate + frontend clearSequence）→ ActivityLog エラー strip。**session は殺さない**（session-status error = terminal 契約、コード別 fail-stop は ef8 後）。レビューで key-mask を 3 round 強化: bidi 割り込み透過（hygiene→mask→cap 順序 + U+FFFD 透過マッチ）/ provider-redacted 断片（sk-/AIza 閾値 4 + redaction 句読点）/ Bearer case-insensitive。CodeRabbit Major 1 + Codex P2 1 + MCP L2 全修正、収束確認パスで merge-gate TTL 失効を復旧）。**両 issue とも先例どおり merged 後 in_progress 据え置き**（ef8 待ちは計 9 件 = 1r1/byf/bx7/sua.1/emd/pbe/9wb/bd7/nal）。follow-up: `rhanis-7xw`（thinking/cost emitter の generation-gate、P3）。**運用学び**: merge-gate 証跡 TTL 失効は「収束確認パス」（review-loop skill 経由の統合検証 1 agent）で復旧可 / verification-gate が wakeup 境界で前周の検証を見失う（targeted cargo test で満たす）/ `~/projects/package.json` 0 バイト stray が vitest 全死を引き起こす既知問題が 2 回再発（都度削除、根本原因未特定）

- **2026-06-12 UTC ループ周回（/loop、周回 7-8 = 上限で停止）**: **PR #62 merged** = `rhanis-r2o` 未登録 tool の stub Ok（phase=done）を **gate 前の phase=error 化**（承認した DANGER が no-op なのに成功表示される虚偽の解消。step 4.5 = deny-list の後・承認 gate の前に registry 存在チェック → operator は実行不能 tool の承認で中断されず、hallucinated 名スパムが approval cap〔rhanis-rxh〕slot を占有する DoS 経路も閉鎖。`LogRow` が `event.detail` を描画 = エラーの「なぜ」を可視化〔R-C 指摘〕。テスト新規 3 + 11 本 `registry_with` 適応 + deny-list 順序 lock。**pure logic で実機非依存のため rhanis-r2o は close**）。Round 1 / C0 / H0 / bot 指摘 0 / CI 4/4 緑で自律マージ。**ループ計 8 周で PR 3 本（#60/#61/#62）を実装→3段レビュー→マージ完遂、上限到達で停止**。残: clippy doc nit は session-end sync で修正済み。**運用学び: merge-gate の R-C 証跡が skill 実行直後でも未認識になるケースあり → 収束確認パス（skill 再 invoke + Codex 確認 1 往復）で確実に復旧**

- **2026-06-14 リネーム後始末セッション**: koe→Rhanis 全統一リネーム（PR #64 本体）の取りこぼし後始末を完遂。①ローカル main→origin 同期（`6c619c1`）②`.beads/config.yaml` sync.remote koe→rhanis.git（**PR #65 merged**）③`rhanis-52p` close（製品名実コード反映を検証 = productName='Rhanis Agent' / identifier=com.zsaku.rhanis / package・crate=rhanis、依存 rhanis-44h/8h0 は署名整合確認の別フェーズのため `--force`）+ `rhanis-zs8` close（リネーム親タスク、6ステップ全検証、**PR #66 merged**）④`core.hooksPath` を旧 `~/projects/koe/.beads/hooks`→rhanis（user shell 実行、`.git/config` は WSL bind-mount で Claude 書込不可、bd git 連携 hook 復活）⑤孤児 `-home-zsaku-projects-koe` memory dir 削除（user shell、`rm` は削除保護 hook で Claude 不可）⑥`.claude/loop.md`=直す箇所なし（唯一の koe は研究パス参照で実フォルダと一致=正、起動コマンドは `~/projects/rhanis/.claude/loop.md` に修正案内）⑦研究フォルダ `~/research/koe-*` は**保護維持**（migration-plan L46『対象外・保護(不変)』、user 確認）。**残課題=`rhanis-3fd`**（migration-plan F: グローバル `~/.claude` の koe→rhanis content 統一 = bd issue title `feat(koe)`/`koe.db` 言及・bd memories キー名 koe-*/現状 content、surface 別で保護対象〔外部 koe.*/研究パス/過去履歴〕あり）。ハンドオフ memory=`2026-06-14-rename-followup-done`。

詳細マイルストーンは `~/.claude/plans/virtual-riding-hearth.md` 参照。

## Task Routing for Rhanis
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
