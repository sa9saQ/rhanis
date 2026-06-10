# koe UX / 動線 根本原因レビュー (2026-06-09)

E2E 実機テスト(ネイティブ Windows)でユーザーが観測した UX 不良の根本原因調査。
Dynamic Workflow で 4 観点(接続 / stronghold / 起動・設定 / UI-UX)を並列調査 →
各仮説を別エージェントが敵対的検証(計 27 エージェント)。**19 件確定 / 4 件却下。**

調査時の生データ全文: workflow run `wf_9f54b9cd-847`(セッション内 /tmp、揮発)。本書が永続版。

## 観測症状(ユーザー証言)
1. APIキー「保存」で数秒スピナーが回る。
2. 「完了」押下後も処理が重い。
3. 起動ロード(「読み込み中…」)が長い。
4. 「話す」→「準備中…」が永久に回り会話が始められない(何もできない)。
5. 全体的に UI/UX の完成度が低い → **ユーザー追加方針: デザイン全面見直し**。

---

## ⚠️ 重要な訂正 — モデル名 `gpt-realtime-2` は正しい(調査中の誤断を撤回)

調査の一時点で「`gpt-realtime-2`(realtime_provider.rs:36)は存在しないモデル名」と誤断した。
**中立検証(prompt にモデル名を入れない fetch + 検索 Links 精査)の結果、誤りと確定し撤回。**

- `gpt-realtime-2` は **2026-05-07 に OpenAI がリリースした実在モデル**(realtime 音声の reasoning モデル、
  context 32K→128K、Big Bench Audio 96.6%)。Azure Foundry も "GPT Realtime 2.0" として提供。
- 誤断の原因: ①アシスタント知識カットオフ(2026-01)が 5 月リリースより前、
  ②最初に見た `models/gpt-realtime` は旧 GA 版ページで `gpt-realtime-2` は別ページだった、
  ③WebFetch の小型モデルが prompt 内のモデル名文字列に同調(幻覚)した。
- **教訓**: koe のモデル名/ヘッダ設定(`OpenAI-Beta` 廃止含む)は現行 GA 仕様と一致。正しい。
  将来 `gpt-realtime` 等へ「格下げ修正」してはいけない。ワークフローの検証エージェントの
  「コードは正しい」が正解だった。
- 出典: developers.openai.com/api/docs/models / thenextweb.com/news/openai-gpt-realtime-2-voice-models /
  learn.microsoft.com (Azure Foundry realtime-2)

---

## 症状別 真因サマリ

### 症状 1 / 2 / 3 (保存遅い・完了後重い・起動長い) = 同一真犯人: Stronghold scrypt
**`secret_store.rs` が全 open/save のたびに age 形式の scrypt(work_factor=19, N=2^19 ≒ 512MB, ~1s/回)を回す。**
- koe は snapshot ごとにインスタンスを開き直す(open→op→save→drop、キャッシュなし; secret_store.rs:216-234)。
- 保存1回 = open復号(~1s) + save暗号化(~1s) + 直後の has確認復号(~1s) ≒ 最大3連発で数秒(ApiKeyInput.tsx:62-97)。
- 起動: OnboardingGate が mount 時 `hasOpenaiApiKey()` を発火(StrictMode dev で二重)。
- 完了: `complete_onboarding` 内で `has_api_key` を1回(settings_store.rs:383-389)。
- secret コマンドは `spawn_blocking` で逃がしておらず async ワーカー上で同期ブロック(他は get_cost_snapshot 等で逃がしている)。

**修正(最優先・最もコスパ高):**
- **起動時に `try_set_encrypt_work_factor(0)` を1回呼ぶ**(lib.rs setup、SecretStore 構築前)。
  koe 鍵は KeychainPassword の 32byte CSPRNG **強鍵**(secret_store.rs:203-208)で、age 公式が
  「強鍵は work_factor 0 でよい」と明記。これで save/decrypt の scrypt が ~1s→即時。
  旧 snapshot(WF19)は復号上限23内で読め、次 save で WF0 に書き直る(**後方互換維持**)。
  CSPRNG 強鍵前提をコメントで不変条件として固定。
- 付加: ①保存後 `hasProviderApiKey` 確認を廃止し `onKeyStatusChange?.(true)` を楽観呼び
  (save の Ok が格納の権威証明)。②secret コマンド + complete_onboarding の has を `spawn_blocking` 化(UI 非ブロック)。
- 注意: work_factor=0 は CSPRNG 強鍵前提でのみ安全。fail-closed(Locked/Backend 伝播)・鍵非露出・
  コスト保護は不変。インスタンスキャッシュ化(復号鍵常駐)は別オプション、トレードオフ明示の上で。

### 症状 4 (準備中…が永久) = タイムアウトと脱出口の二重欠如
- **backend**: `establish_connection`(session_manager.rs:844-941)の WS handshake
  `connect_async_with_config`(:870)+ initial_frames 送信(:879-883)に **per-attempt timeout が無い**
  (:1091 コメントが自認「no per-attempt timeout; OS TCP timeout bounds the worst case」)。
  TLS/proxy/firewall で handshake が hang すると connecting=loading=「準備中」のまま固着。
  ※ reconnect 無限ループ・モデル名・4xx 握りつぶしは**否定済**(4xx は Fatal→即 error、reconnect は
  max_attempts=6 / max_total=20 で必ず fail-closed)。真因は handshake timeout 欠如。
- **frontend**: sessionStore.ts:138-165 で loading に入ると、抜けるのは session-status イベントのみ。
  VoiceButton は loading 中 `disabled` + stopSession も loading で no-op(:188) = 経過も見えず停止もできず**完全無反応**。

**修正:**
- backend: `establish_connection` 全体を `tokio::time::timeout(CONNECT_TIMEOUT 10-15s)` で包み、
  Elapsed を `ConnectError::Recoverable("connection timeout")` に。既存 supervisor backoff→
  max_attempts→finalize error emit の fail-closed 経路に乗る(無限化しない、Fatal にしない)。
- frontend: startSession で `startedAt` 記録→watchdog(12-15s)で stoppable な状態に昇格→
  その状態でのみ stopSession 許可(:188 の厳密 loading ガードは維持)。経過秒表示。
  timer は useRef + cleanup(async-react)。stop は ipcStopSession 必達(orphan billable 防止)。
  両方入れる(backend timeout が listener 不達まで救えないため)。

### 症状 5 (UI/UX 完成度) = リデザインで吸収
ユーザー方針で**全面リデザイン(没入型 orb + OS追従)**へ。以下の現状の崩れは個別修正せず epic に吸収:
- `.koe-app-header` 未定義 → 上部ヘッダが縦積みに崩れる(App.tsx:32、CSS 0 件)。
- 起動ローディング画面が無装飾テキストのみ(スピナー/進捗/推定なし; OnboardingGate.tsx:82-93)。
- 設定がインライン展開で主役「話す」を下へ押し下げる(App.tsx:43、ApprovalModal は正しくモーダル化済)。
- `.koe-voice-btn-label` 未定義(VoiceButton.tsx:116、影響軽微だが未配線の証跡)。
- 準備中スピナーに経過表示なし(症状4 と連動)。
- **堅牢点(維持すべき)**: コントラスト比 AA 以上・anti-ai-smell(Inter/gradient 不使用、radii ばらつき)・
  input 16px は現状良好。リデザインで壊さないこと。

---

## 確定 19 件(severity 順)

| # | area | 内容 | sev | 症状 |
|---|---|---|---|---|
| 1 | stronghold | scrypt work_factor=19 が全 open/save で ~1s(主犯) | critical | 1/2/3 |
| 2 | ui-ux | 準備中スピナーに timeout/cancel/経過なし=永久回転 | critical | 4/5 |
| 3 | connection | WS handshake/connect に timeout 無し→hang で固着 | high | 4 |
| 4 | stronghold | 保存1回で scrypt 2〜3連発(save→直後 has 冗長) | high | 1 |
| 5 | stronghold | 初回 snapshot 新規生成+keychain 鍵生成の一括コスト | high | 1 |
| 6 | startup | secret 経路が spawn_blocking 無しで同期ブロック | high | 1/2/3 |
| 7 | startup | 保存=重い書込+冗長な重い再オープンの2連発 | high | 1 |
| 8 | startup | 起動 hasOpenaiApiKey が mount 直後発火+StrictMode 二重 | high | 3 |
| 9 | connection | establish 中の中間 status 無し(進捗が返らない) | medium | 4 |
| 10 | startup | complete_onboarding 内 has_api_key の重い open | medium | 2 |
| 11 | ui-ux | `.koe-app-header` 未定義でヘッダ縦積み崩れ | medium | 5 |
| 12 | ui-ux | 起動ロード画面が無装飾(スピナー/進捗なし) | medium | 3/5 |
| 13 | ui-ux | 保存系が楽観UIでなく完全同期待ち | medium | 1/2/5 |
| 14 | ui-ux | 設定がモーダルでなくインライン展開で主役を押し下げ | medium | 5 |
| 15 | stronghold | open 毎に OS keychain 往復(scrypt より小だが毎回) | low | 1/3 |
| 16 | startup | lib.rs setup は軽量(症状3 主因ではない=切り分け) | low | 3 |
| 17 | ui-ux | `.koe-voice-btn-label` 未定義(未配線の証跡) | low | 5 |
| 18 | ui-ux | コントラスト/ボタン体系は AA 準拠で良好(維持事項) | low | 5 |
| 19 | connection | (否定的所見) reconnect 無限・モデル名・4xx は症状4の原因でない | low | 4切り分け |

## 却下 4 件(敵対的検証で棄却 = 切り分け価値)
- **connection**: cpal device open が connected を gate / prev.join() ブロック → 否定。
  device open は spawn したスレッド内、初回は prev=None、失敗時は "mic device lost" emit で error 化(固着しない)。
- **stronghold**: 「同期 scrypt が async ランタイムを starve」→ 否定。tokio は rt-multi-thread 有効
  (tauri 経由)。実害は各コマンドの latency であり全体凍結ではない。KDF を argon2 と誤帰属していた点も誤り。
- **startup**: 症状2 を ActivityConsole 初期 pull / CostHeader プレースホルダに帰属 → 否定。
  プレースホルダは静的テキスト(回らない)、get_cost_snapshot は lock 競合なし。回るスピナーは症状4(loading)。
- **ui-ux**: 確信度ラベル未実装が症状の根本原因 → 否定。backend が confidence を出さない(M1 仕様通りの正常)、
  症状1-4 と無関係。enhancement を root cause と取り違え。校正レイヤ(koe-sua.2)前の先行 scaffolding は E2 教訓と逆行。

---

## 次アクション(bd issue 起票済、label `review-2026-06-09`)
先行グループ(デザイン非依存・即着手可):
1. **[P0] Stronghold work_factor=0**(症状1/2/3 の主犯、最もコスパ高) — Hybrid review
2. **[P1] 保存 follow**(has 冗長排除 + spawn_blocking) — work_factor の派生
3. **[P1] 接続 timeout (backend)**(症状4) — Hybrid review
4. **[P1] 準備中 watchdog (frontend)**(症状4) — Claude 直
統合グループ(デザイン確定後):
5. **[P1][epic] UI/UX 全面リデザイン**(没入型 orb + OS追従) — デザインブリーフ参照

着手順: 1 → (2 と並行) → 3+4(症状4ペア) → 5(デザイン生成後)。
最新 ID は `bd list --label review-2026-06-09` で確認。
