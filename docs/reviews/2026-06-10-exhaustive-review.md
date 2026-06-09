# koe 徹底レビュー 2026-06-10 — 動線 / 収益性 / 競合戦略 / 製品強化

## 0. 実施方法と検証の来歴

- **体制**: Dynamic Workflow 13 エージェント完走（コード監査 3 = 動線/セキュリティ/テスト実態、状態整理 2 = docs/bd 地図、web 研究 4 = 音声原価/競合価格/課金運用法務/GTM、戦略分析 4 = P/L・競合 stress-test・ロードマップ・製品ギャップ）。研究は recon 規律（公式 pricing ページ直接 fetch・as_of 日付・entity 実在検証）。
- **検証**: workflow の敵対検証フェーズ（29 エージェント）が Claude 週次上限で失敗したため、代替 3 系統で実施 — ①主執筆者による実コード直接照合（high 所見 5 件 + コード関連戦略主張 4 件、全件 file:line で再現確認）②**Codex GPT-5.5（別 provider）の read-only 独立レビュー**（8 件全て CONFIRMED + 監査が見逃していた新規 high 3 件を捕捉）③P/L 数式の主執筆者による追算（§4 に補正注記あり）。
- **棄却（refuted）された所見: 0 件**。検証で修正が入ったのは P/L のマージン下限解釈 1 点のみ（§4.2）。
- bd 反映: 新規 19 issue 起票（label `review-2026-06-10`）+ 依存リンク 4 件 + CLAUDE.md 事実修正 2 件（Hermes tier 名 / M1 tools 本数）。

---

## 1. エグゼクティブサマリ

**総合判定: コア（動線・安全・テスト規律）は業界水準を超えて健全。M1 の残りは実装ではなく「検証 1 本（koe-ef8、実質今すぐ実行可能）」。一方で、製品を商売にする層（wire 形状の実機確定・課金マージン・配布・table-stakes 2 件）に、自覚されていなかった穴が 5 群あった。**

1. **動線は「正しく作られている」**。主要 5 動線（キー→セッション→音声→UI / function_call→3 段ゲート→tool→記録→表示 / usage→ledger→予算停止→表示 / thinking-event / 設定→反映）は entry→core→output まで実配線を確認。コマンド 19 本・イベント 5 チャネルの Rust↔TS 不一致 0。skeleton 残置は意図的 2 箇所のみ。
2. **「準備中」固着の唯一のブロッカーは frontend の 1 行**（`sessionStore.ts:188`）。backend は接続ハング中の stop を既にサポート済（`tokio::select!`）— koe-5fs は watchdog を待たず 1 行修正で大半が解消し、これを **koe-ef8 着手前に入れないと E2E 自体がハングで止まり得る**。
3. **mock green が隠す wire 形状リスク 3 件**が未起票だった: GA 音声イベント名の非対称（実機で無音、`koe-bd7`）/ server error の黙殺（tools・ASR・会話ログ・thinking-event が 4 連鎖 silent dead、`koe-nal`）/ usage 解析失敗の fail-open（予算 cap の核心保証が未成立、既知 koe-2br）。さらに Codex が独立に 3 件捕捉（cancelled response の tool 実行 `koe-z8j` / 応答単位 cap 不在 `koe-95z` / 時計ジャンプ ledger reset `koe-9qd`）。**ef8 実機で wire frame を採取し fixture 化（recorded-fixture regression）することが恒久対策**。
4. **利益は出る。ただし数字 1 箇所を直す前提** — koe-1mf の「実コスト+15-25% マージン」は 10% ボーナス+決済手数料込みで赤字〜ゼロマージン設計（検算 §4.2）。メーターは**実コスト×1.8-2.0 のパススルー型**へ。既定モデルを安価クラス（Gemini Live 級 $1.1/h）にロックすることが採算の第一決定要因（OpenAI-full 素は $29/h まで膨らむ）。損益分岐は**有料 5-30 人**、全クレジット 180 日失効で資金決済法を構造回避。
5. **勝ち筋「校正 glass-box」は単独ウェッジでは通らない** — 見せかけ確信度 UI（confidence theater）は数週間で模倣可能で、校正品質の差は外から見えない。生き残る形は複合体 =（i）provider 中立+ローカル主権（Microsoft が定義上模倣不能）（ii）校正品質の実行（iii）作り手=プロダクトの真正性。これを体験に変える装置が**正直レポート**（週次自己採点、`koe-84w`）— 校正なしの競合は実数を開示できないため、唯一の可視な分離装置になる。
6. **table-stakes の 2 件が自分に無い**: barge-in（割り込みで AI が黙る、`koe-bx7`）と常駐 UX（トレイ/自動起動、koe-944）。差別化を語る前に土俵に乗る条件で、barge-in は介入 UI として差別化と同一実装線上にある。
7. **M1 と M4 の間に「配布可能な製品」マイルストーンが欠落**。コード署名は日本在住個人だと Azure の Public Trust 不可で、IV 証明書 vs Microsoft Store の経路決定 + 調達がカレンダー律速（`koe-44h`、即時着手推奨）。初収益の最短経路は **Founder's License（BYOK+応援ライセンス、ef8 後 5-7 週）を橋にして M4（120-150 日）を本線にするハイブリッド**。

---

## 2. 動線監査 — 「動線はちゃんとできるようになっているか」への回答

**回答: はい。設計通りに配線されており、進行中の未配線（web_search 非登録 / ManagedCredit stub）も意図的・追跡済み。** 以下が検証済み所見（全件、主執筆者のコード照合 + Codex CONFIRMED）。

| # | 所見 | 深刻度 | 証拠 | 対応 |
|---|---|---|---|---|
| W1 | 「準備中」固着の唯一のブロッカー = frontend 1 行ガード。backend `run_session_supervised` は connect を `master_shutdown` と select! で競争済みで、接続ハング中の stop に対応済 | **high** | `sessionStore.ts:188` vs `session_manager.rs:1090-1100` | **koe-5fs の第一手を「loading 中も stopSession を通す」1 行に変更し ef8 前に実施**。watchdog/経過表示はその上。connect timeout（koe-9wb）は別途必要 |
| W2 | 手足 tool キー設定 UI が dead-end（xai/x/search キーを保存・有効化しても consumer 0 件） | medium | `tools/mod.rs:300-306`（search provider 無条件 None）、SettingsPanel.tsx:24-29 | `koe-25y` 起票（準備中ラベル or 非表示）。CLAUDE.md「M1 tools 4 本」は修正済 |
| W3 | 未登録 tool が stub Ok + phase=done を emit — DANGER 承認後に「実行していない作業」が完了表示される | medium | `tool_dispatcher.rs:326-335` | `koe-r2o` 起票（phase=error 化 + ゲート前 registry チェック）。透明性を掲げる製品で虚偽に近い表示 |
| W4 | recorder の読み出し面が production 未配線（`list_recent_notes/events` は test のみ）— 製品 3 本柱「記録」が user に見えない | medium | `storage/sqlite.rs`（cfg(test) のみ）、lib.rs handler に履歴系 0 | 既知 koe-sh6 の範囲。**ef8 の検証手順に SQLite ファイル直接確認を明記**（提案、§9） |
| W5 | M4 経路: `RealtimeAuth::ManagedCredit` は enum 枠のみで構築箇所 0（意図的 stub）。現 `bearer_header` 形式のままだと「運営キーをクライアントに渡す」形になる | low | `realtime_provider.rs:47-63`、`session_manager.rs:850`（常に Byok） | koe-3x6 設計リサーチで ephemeral token mint vs WS proxy を最初に決定（§4.7） |
| W6 | ActivityLog の displaySummary 英語固定（"run {tool}"）と thinking-event 日本語の言語混在 — glass-box 主表示面 | low | `tool_dispatcher.rs:375-377` vs `session_manager.rs:524-537` | koe-ios リデザイン時に開示文言を単一ソース化 + i18n 前提に |

**M1 完成までに残る配線**: (a) koe-ef8（最大 gating、§7）、(b) W1 の 1 行、(c) 上記 medium 3 件は M1 出荷をブロックしない（ただし W3 は DANGER tool 実配線前に必須）。

---

## 3. mock-green と実機の乖離 + セキュリティ

### 3.1 テスト実態の総括

cargo 400+ 本 / vitest 187 本は「注入された入力に対する状態機械の検証」として非常に質が高い（世代ガード・予算 fail-closed・再接続 supervisor を網羅、未検証境界をコメントで自認する誠実さも例外的）。**ただし実 IO 境界 5 つ — 実 WSS handshake / 実 OpenAI payload 形状 / cpal・rodio デバイス / Windows Credential Manager / 実 Tauri IPC — は全 suite + CI を通じて構造的にゼロカバレッジ**。「383+ tests green」の正確な意味は「実機で会話できる」ではない。koe-ef8 を M1 gating に置く運用は正しい。

### 3.2 wire 形状リスク（全て検証済・起票済）

| # | 所見 | 深刻度 | 起票 |
|---|---|---|---|
| T1 | **音声再生が beta イベント名のみマッチ**（`response.audio.delta`）。transcript は GA/beta 両対応なのに audio だけ非対称で、build_request は GA インターフェースを選択済み。実サーバが GA 名（`response.output_audio.delta`）で送ると**全テスト green のまま assistant 無音** | **high** | `koe-bd7`（新規発見、どこにも未追跡だった） |
| T2 | **parse_frame に `error` arm が無く server error を黙殺**。session.update が実 API に拒否されると tools 広告/ASR/会話ログ/thinking-event（= in_progress 4 件中 3 件の実機価値）が 4 連鎖で silent dead | **high** | `koe-nal` |
| T3 | **usage 解析失敗 = fail-open**。「予算 cap で必ず止まる」という koe の核心保証が、実 GA payload 形状という未検証前提の上に建っている | **high** | 既知 `koe-2br`（本レビューで深刻度の含意を明確化: 実 payload 確認まで「予算 cap は実機未検証」を完了報告に明記し続ける） |
| T4 | connect timeout 欠如 + loading 脱出口なし — 「hang する接続」は現テスト構造で表現不能（全テストが注入式） | **high** | 既知 `koe-9wb` / `koe-5fs`（W1 で前進） |
| T5 | 再接続毎にマイク再 open し、一時的デバイス競合を `Fatal` 誤分類して retry せず死ぬ（supervisor テスト 10 本は audio open を通らない） | medium | 既知 koe-pr3 / koe-byf 系。ef8 必須シナリオに「会話中に Wi-Fi 切断→復帰」を追加 |
| T6 | Windows E2E smoke の実カバレッジは「boot + IPC 1 往復」のみ。stronghold 書込/Windows keychain は全 suite で実行 0、60s timeout は koe-ds6 級の起動遅延を完全マスク | medium | `koe-ysu` 起票（Step B + 起動時間 assert） |
| T7 | vitest は IPC 境界 100% mock、payload casing 3 様式混在（camel/snake/json! 直書き）の整合はコメント頼み | medium | 既知 koe-5sc（parity test） |

### 3.3 Codex（別 provider）独立スイープの新規捕捉 3 件

| # | 所見 | 深刻度 | 起票 |
|---|---|---|---|
| X1 | **cancelled/incomplete response の function_call がそのまま実行され得る** — `function_call_arguments.done` 受信即 dispatch で `response.done` の status と相関しない。ユーザーが遮った action の SAFE/CAUTION tool が走る | high（barge-in 実装で必須の対） | `koe-z8j` |
| X2 | **応答単位の出力上限なし** — 予算 gate は usage 受信後のみ発火するため、1 つの長大応答で cap を単発超過し得る（有界だが BYOK 実費）。`max_response_output_tokens` 未設定 | high → 実害は応答 1 個分で有界のため運用上 medium 寄り | `koe-95z` |
| X3 | **時計ジャンプで月 ledger reset** — 会計月がローカル時計由来のため未来月ジャンプで cap 回避可。M1（自己予算）は実害小、**M4 サーバーメーター移植時の設計要件**として記録 | high → M1 文脈では low-medium | `koe-9qd` |

### 3.4 セキュリティ総括

**BYOK コアは攻撃者視点でも堅牢**: 鍵は stronghold に閉じ、SecretString/Debug redaction/非 Serialize/zeroize の多層、`get_*_api_key` コマンドの構造的排除、ログ・panic・event payload への漏れ経路 0 を確認。予算 ledger（additive/saturating/fail-closed）、validation.rs（openat2/O_NOFOLLOW）、permission_policy（DENY>ALLOW>DEFAULT）も水準超。

新規所見（起票済）:
- **承認モーダルと開示が操作対象（パス/コマンド/URL）を一切出さない**（`koe-whf`、P1）— 人間は「run delete_file」だけ見て承認することになり、prompt injection への最後の防壁と glass-box の双方を弱める。**中心思想が redaction によって自壊している**本質的所見。安全な粒度（ホーム相対 dir+basename / コマンド先頭トークン / host のみ）で開示する。実 tool 配線（koe-eal/p1a）前に承認 UX 契約を確定すべき。
- 会話ログ/ノートが**平文 SQLite**（`koe-2ms`）— 発話逐語は API キーより機微なのに鍵だけ暗号化されている非対称。
- WebView 侵害→DANGER 自己承認のネイティブ確認面が**コード内 acknowledged だが bd 未追跡だった**（`koe-38m`、style-src 'unsafe-inline' 撤去込み）。
- model 制御 call_id の長さ無制限コピー（`koe-ijd`）。

---

## 4. 収益性 P/L — 「利益が出るようになるためには」への回答

**回答: 現在の設計骨格（前払いクレジット 1 本 + Hermes 下敷きプラン + 青天井/後払い不採用）は健全で、利益は出る。ただし以下の 6 点を直す/確定することが前提。** 出典 facts は公式 pricing ページ直接確認（§10 参照）。

### 4.1 原価の実数（実会話 1 時間、VAD ゲート通過後）

| シナリオ | 声 API | ASR | tool | 決済/インフラ按分 | **合計/h** |
|---|---|---|---|---|---|
| Low（Gemini Live 級既定） | $0.84 | $0.09 | $0.01 | $0.14 | **$1.08 ≈ ¥168** |
| Base（混合: Gemini 6 割 + mini/Nova 2.5 割 + OpenAI cached 1.5 割） | $1.80 | $0.18 | $0.05 | $0.30 | **$2.33 ≈ ¥361** |
| High（gpt-realtime-2、cache 中位） | $6.00 | $0.51 | $0.25 | $0.93 | **$7.69 ≈ ¥1,192** |
| 災害尾部（OpenAI-full、cache 不全） | $27.6 | — | — | — | **≈$29/h = ¥4,500** |

単価根拠: gpt-realtime-2 = audio in $32/1M tok（cached $0.40）/ out $64/1M、入力 600 tok/分・出力 1,200 tok/分 → 理論下限 $2.88/h、実測 cached $3-6/h・uncached $6.9-27.6/h（支配項 = 毎ターン再課金されるシステムプロンプト+履歴）。Gemini Live（3.1-flash-live）= in $0.005/分 + out $0.018/分 ≈ $0.7-0.9/h。Nova 2 Sonic ≈ $1/h。Grok Voice $0.05/分 flat = $3/h。

**含意**: 原価は既定モデル選定で 7 倍、運用ミスで 27 倍ブレる。**`koe-7yy`（安価プロバイダ）と `koe-y1j`（GeminiLive 配線）と `koe-6ul`（VAD ゲート）は機能要望ではなく収益 critical path 上の実装 issue**。OpenAI-full は「残高消費が速いプレミアム」の明示 opt-in に。原価ブレの主因は会話時間でなく「プロンプト長 × cache ヒット率」— メーターは分単価固定でなく**実トークン消費×M のパススルー型**にする（cost_tracker の nanodollars 設計がそのままサーバー側メーターの原型になる）。

### 4.2 マージン倍率 M の検算 — koe-1mf の現記述は赤字設計【本レビュー最重要の数値修正】

収入 R、付与クレジット 1.1R（10% ボーナス、retail 建て）、MoR 手数料 6.5%+$0.50、消費率 f、原価 = 消費 retail / M とすると:

> 月次利益 = R(0.935 − 1.1f/M) − $0.50

- **M=1.15（koe-1mf の「+15%」）**: f=1.0 で Plus $20 は **−$0.93/月（赤字）**
- **M=1.25（「+25%」）**: f=1.0 で +$0.60 — 手数料誤差で消えるゼロマージン
- **定常状態の損益分岐は M ≥ 1.21**（Plus: $18.20 ≥ $22/M）
- 検証注記（主執筆者の追算による補正）: P/L 分析が出した「M≥1.76（繰越上限まで使うワースト）」は**単月キャッシュの境界**であり、繰越分は前月支払い済みクレジットなので定常では二重計上になる。定常下限は上記 1.21。ただし**推奨は M=1.8-2.0 で変わらない** — 原価追従の誤差（cache ヒット率の想定外れ・プロバイダ値動き）、サポート/不正/為替のバッファとして必要。M=2.0 でブレンド粗利 Plus 58% / 中位 tier 49% / 上位 42%。
- 「使われすぎて赤字」は構造的に起きない（消費に比例して retail で減算されるため）。**赤字経路は (a) メーター単価の原価誤設定 (b) 無料配布/ボーナスの漏れ (c) 盗難カード→チャージバック、の 3 つだけ**。リスクはここに 100% 集中する。

### 4.3 プラン・チャージ設計

- **価格帯**: 設計中心 **$19.99/¥2,980**（市場重心: Hermes Plus $20 / Simular Sai $20 / ChatGPT Plus $20 / M365 Premium $19.99 / Highlight $20）、エントリー $8/¥1,200（ChatGPT Go 対抗）、上位は $49 まで。**$100+ tier は自前ホスト（koe-aja）による原価 10-30 倍減の前には出さない**（API 原価のままだと「時間が少ない」体感不満が正直ブランドを毀損する）。
- **分（会話時間）の潤沢さでは競争しない**。売り物は「会話分数」でなく「片付いた秘書仕事」。
- **「料金も glass-box」をブランド拡張に**: VAD 課金（開きっぱなし無料・実会話のみ課金）+「残高 ¥820 ≈ 約 14 分」併記 + 上限 cap は、サブスク業界の dark pattern への明確なカウンターで透明性ブランドと同型。価格ページ自体を差別化面として書く。
- **¥500 チャージパック廃止**: MoR 固定手数料 $0.50 で実効手数料 20.6%。最低チャージ ¥1,500（実効 10%）〜¥3,000（7.6%）。card testing（Visa VAMP の excessive 閾値が 2026-04 から 1.5% へ強化）対策としても有効。
- 10% ボーナスは**プラン更新限定**（Hermes と同じ）。都度チャージに付けるとプランの存在意義と Sybil 耐性が崩れる。

### 4.4 無料お試しと不正

- 1 人あたり原価 **¥51〜109**（15-30 分 × 安価既定 $0.27-0.54 + SMS）。**トライアル中は安価既定モデルに強制ロック**（誤って OpenAI-full なら ¥580/人 = 8-11 倍）。
- SMS は **Firebase Phone Auth**（月 1 万認証無料、超過 $0.06）が Twilio Verify（日本 $0.14-0.17）の 1/2〜1/3。Stripe 公式ガイドの多層防御（使い捨てメール遮断 + デバイス/IP velocity + VoIP 番号拒否）を併用。
- Sybil 上限損失は 1 偽アカウント ¥51-109 で頭打ち（一回きり付与）。**月次トライアル予算 cap（例 ¥50k ≈ 700 人/月）で破産経路を物理的に塞ぐ**。
- 転換率 3-5%（業界標準。ハードペイウォールなら中央値 12.1% = RevenueCat 7.5 万アプリ）なら有料 1 人あたりトライアル原価 ¥1,400-2,300、Plus 月次粗利 ¥1,793 で**回収 0.8-1.3 ヶ月**。

### 4.5 決済層と法務

- **決済は Polar 一本を推奨**（MoR 5%+$0.50、日本セラー対応明記、無料プラン、dispute $15）。Paddle は $10 未満要相談、Lemon Squeezy は Stripe 買収後の移行期で新規非推奨。Stripe 直販（3.6%）は最安だが EU VAT が非 EU 事業者に閾値ゼロで 1 件目から課されるため個人のグローバル販売には非現実的。
- **資金決済法: 全クレジットを「付与日から 180 日失効」に設計すれば 6 ヶ月適用除外**（資金決済法 4 条 2 号）で届出・未使用残高 1/2 供託・払戻規制が全て外れる。繰越・ボーナス・都度チャージ全部に統一、自動延長禁止、有償先消費 FIFO。規約確定時に専門家確認推奨。→ `koe-krv` に集約。

### 4.6 損益分岐と固定費

- 固定費 ≈ **¥13,000-15,000/月**（署名 ¥1,600-3,300 + Sentry $26 + Supabase Pro $25 + CF Workers $5 + 雑費。M1 BYOK 期はさらに小さい）。
- **有料 5 人（ミックス）〜9 人（全員 Plus）で黒字**。トライアル予算 ¥50k/月を載せても 16-28 人。個人開発の固定費構造では黒字化のハードルは実質存在しない。
- **自前ホスト（koe-aja）発動トリガーの数値化**: 「有料 50 人 or 月間総会話 150h」で前倒し検討（H100 $2.54/h、損益分岐 月 40-100 会話 h）。それまで API 仕入れが正しい（固定費ゼロ・fail-soft）。

### 4.7 収益化の順序

- **M4 前倒しは不要**（固定費が小さく runway を圧迫しない）。ただし**価格・メータリング設計は本分析で今確定**できる（`koe-krv`）。
- M4 backend の最初の決定 = **ephemeral token mint vs WS proxy**（現 `bearer_header` のままでは運営キーがクライアントに渡る、§2 W5）。
- **初収益の橋 = Founder's License**（§7）。着手前に `koe-bup`（BYOK 有料/無料の二説解消）が必須。

---

## 5. 競合戦略 — 「競合に勝つには」への回答

### 5.1 stress-test の結論: ウェッジの再定義

「校正 glass-box = 唯一の耐久ウェッジ」を敵対的に検証した結果、**条件付きで生存**:

- **機能の堀としては弱い**: 確信度バッジ UI（校正なし）は Simular Sai なら数週間、Perplexity なら 3-6 ヶ月で模倣可能。校正品質の差は外から見えない（スクショでは本物と見せかけが同一）。Google は「double-check」（検索照合ハイライト）を 2023 年から実際に出荷しており「大手は構造的に出せない」説は部分反証済み — 正確には「**出しても使われなかったので再投資しない**」。
- **真の脅威 = confidence theater の氾濫**: 低品質な見せかけ確信度が 2026 年内に複数出ると、市場認知では「確信度表示はどこにでもある」になり、**first-mover window は実装でなく認知の窓として先に閉じる**。
- **需要の真実**: ユーザーが欲しいのは確信度表示でなく「オオカミ少年にならない少数の的確な警告」（koe 自身の E2 実験: 生 confidence 直出しは作業ログ以下）。確信度既定非表示の決定（koe-sua.2）により、**日常で見える差別化面は活動ログに縮小**しており、それは Copilot Actions（Agent Workspace）等で table-stakes 化進行中。
- **Calibration Memory の「奪えない蓄積」は過大主張**: 47h の成熟は有料ユーザー実利用ペース（月 3-4h）で 1 年超 = **獲得には効かない**。大手は人口レベル校正 prior で cold-start の大半を埋められる。真の含意は「**今すぐ outcome 計測を全セッションに仕込み蓄積を最速開始**」（koe-1r1 が戦略上の最優先タスク）+ koe 自身も工場出荷 prior を同梱して自衛。

**生き残る形 — 防御力は複合体**:
> （i）**provider 中立 + ローカル主権** — Microsoft が定義上模倣できない唯一の属性は「Microsoft でないこと」。実は最も耐久性が高い。（ii）**校正品質の実行** — 見た目の模倣は安いが、実数を開示できる正直さは校正なしには出せない。（iii）**作り手=プロダクトの真正性** — 非エンジニアが AI で透明に作る物語（note/X の BIP 資産）。
> 3 本まとめて「**信頼の主権**（your secretary, honestly yours / 日本語訳: あなたの秘書、誠実にあなたのもの）」。

### 5.2 不可視の校正品質を可視資産に変える装置 = 正直レポート（本レビュー最大の新規提案、`koe-84w`）

週次の自己採点カード:「今週、『確実』と言った操作の実成功率 96%。『自信なし』と警告した 3 件中 2 件はあなたが止めて正解」+ 共有可能な画像出力。

- **デモ不能問題を解く**（校正は稀な重大局面でしか体感されず、スクショ・口コミで伝わらない）
- **confidence theater との分離装置**（校正なしの競合が実数を開示すれば嘘がバレる — 大手は法務上、自己精度の数値公開ができない。**正直さは構造的に大手が追えない発信形式**）
- **BIP 素材が毎週自動生成される**（「自分の AI の成績表を毎週晒す開発者」はそれ自体がコンテンツ）

### 5.3 Copilot 無料同梱シナリオの生存戦略

主権ニッチの王に退避（Proton/Obsidian 型: 「会話・ファイル・校正データは全部あなたの PC の中」。Copilot の強制力が強まるほどこのニッチは拡大する）+ 正直さの挑戦者ブランド + モデル自由のアービトラージ（Copilot は Azure マージンが必要で最安モデルを選べない）+ 週次出荷の機動戦。enterprise と便利さの正面戦には出ない。

### 5.4 ポジショニング（3 セグメント分業）

| セグメント | 役割 | 価格 | 一言ピッチ |
|---|---|---|---|
| A. 主権パワーユーザー（local-first/privacy 層） | **配布エンジン**（口コミ/winget/GitHub） | BYOK 無料 | **EN**: "Your AI secretary that shows its work and stays on your machine — not Microsoft's." / **訳**: 「作業の中身を見せ、あなたの PC の中に留まる AI 秘書 — Microsoft のものではなく、あなたのもの」 |
| B. 安全に任せたい非エンジニアのソロワーカー | **収益本体** | ¥1,200-2,980/月 | **EN**: "An AI secretary you can watch work. It asks before doing anything risky — and shows you its own report card every week." / **訳**: 「働きぶりが見える AI 秘書。危ないことは先に聞いてくる — そして毎週、自分の成績表まで見せてくる」 |
| C. 日本語×build-in-public 読者 | **信頼資産・初期 10-100 ユーザー** | A/B 混合 | **EN**: "Talks naturally in Japanese, works honestly on your PC. Built in public by a non-engineer." / **訳**: 「日本語で人と話すように。あなたの PC で誠実に働く秘書。非エンジニアが過程を公開しながら作っています」 |

**koe-20f「ピッチを glass-box 主役に」の部分修正**: glass-box 見出しは投資家/プレス/セグメント A 向け。**収益本体のセグメント B には「見える・止められる・成績表がある」という体感語に翻訳**して使う（「校正された透明性」では伝わらない）。

### 5.5 競合事実の更新（公式 source 確認済）

- **Hermes Desktop は 2026-06-02 に public preview リリース済**（Win/Mac/Linux、MIT OSS）。Nous Portal 実名は Free / Plus $20 / **Super** $100 / Ultra $200（「Pro」は不存在 — CLAUDE.md 修正済）。
- **MS Copilot**: Copilot Pro は 2025-10 廃止 → M365 Premium $19.99 へ統合。無料 Copilot に音声込み、Copilot Actions は Insider 展開中（追加課金なし）、Build 2026 で Agent Workspace/Orchestrator を 25H2 へ拡大。**最大脅威評価は維持・強化**。
- **Simular Sai**: Plus $20（10,000 credits）/ Pro $500、**Windows 対応が公式 pricing に明記** = M1 surface 直撃。最重要 watch 継続。
- ChatGPT: Free/Go $8/Plus $20/**Pro $100（2026-04 新設）**/Pro $200。Agent Mode は Plus 以上。
- **ウェッジ鮮度: 「消費者×音声×PC 秘書×校正確信度開示」の新製品は 2026-06-10 時点で発見されず — 空席維持を確認**（見つかったのは縦型 B2B のみ）。ただし空席の意味（機会か墓場か）は 90 日の出荷で自ら検証するしかない。

### 5.6 90 日内の禁止リスト（リソース分散の防止）

koe-jhk（視覚 epic）/ koe-pj1（外出先チャネル）/ koe-d9t（翻訳）/ koe-aja（自前ホスト R&D）/ koe-7yy の全プロバイダ実装（OpenAI + Gemini の 2 本で十分）/ Mac・Linux 移植 / M4 backend の実装着手（設計リサーチまで）/ コンソール UI の磨き込み無限ループ / 「話せる」「3 段承認」を見出しに使う発信。

---

## 6. 製品強化 — table-stakes と差別化体験

### 6.1 table-stakes ギャップ（無いと選ばれない順）

| # | 機能 | koe の現状（実コード確認済） | 対応 |
|---|---|---|---|
| 1 | **barge-in（割り込み）** | **未実装**。speech_started/response.cancel 処理 0 件、stop_immediate は未配線 — ユーザーが被せても AI 音声が流れ続ける。ChatGPT AVM/Gemini Live/Copilot 全対応 | `koe-bx7` 起票（P1、koe-dcq から最小分離）。**M1 体験の成立条件** |
| 2 | **常駐 UX（トレイ/自動起動/OS 通知）** | 実装 0 件。閉じる=終了。非フォーカス時 DANGER 承認が沈黙 timeout→自動拒否 | 既知 koe-944/koe-hah。「起動しっぱなし」の約束が現状虚偽 — M1.5 へ前倒し推奨 |
| 3 | **起動・応答の速さ** | koe-ds6（scrypt ~1s×3、真因確定・実装は明示指示待ち）+ 9wb/5fs | 解禁後に最初の実装スロット（§7） |
| 4 | **会話を跨ぐメモリ** | 無し。**session.update に instructions すら送っておらず、人格・記憶注入の経路自体が未配線**（grep 0 件） | 既知 koe-9ds。「秘書」を名乗る最低条件 + Calibration Memory の土台 = 遅延は堀の構築遅延 |
| 5 | **履歴/記録の閲覧 UI** | SQLite に書くが読み出し面 0（§2 W4） | 既知 koe-sh6 |
| 6 | **配布の信頼（署名/SmartScreen）** | 未着手。**日本在住個人は Azure Public Trust 不可** | `koe-44h` 起票（経路決定 + 調達、カレンダー律速） |
| 7-10 | 画面共有 / 声・ペルソナ / wake word / モバイル | epic・issue 起票済（jhk/owz・45n/6ul・6hu/pj1） | M2-M4 で順次。M1 ブロッカーではない |

**核心**: 競合地図の結論通り「音声+常駐+PC 操作は table-stakes」だが、**koe は現時点でその table-stakes の「音声（割り込み）」と「常駐」自体を満たしていない**。差別化の上積みより先に土俵に乗る 2 点が落ちている。幸い barge-in は「介入 UI」として差別化と同一実装線上にある（DANGER 30s カウントダウン中に声で「だめ」と言える、まで伸ばせる）。

### 6.2 校正 glass-box を「体験」にする 4 装置（起票済）

1. **ワンタップ訂正「それ違う」**（`koe-1l4`、P1）— 各ターンに訂正ボタン → 確信度×結果ペアを記録。**校正の信号源問題（koe-1r1 = 堀の最大技術リスク、人間シグナルが疎）を最も安価に解く経路**。訂正を受けたら音声で短く認め「訂正済み」マークを残す誤り告白（trust repair）は、魔法 UX の大手が構造的に出せない体験。
2. **正直レポート**（`koe-84w`、§5.2）— 蓄積の見える化 = retention 装置 + スイッチングコストの体験化 + glass-box 思想の自己適用。
3. **元に戻すバー**（`koe-nak`）— write 系の backup-before-write + 実行後 N 秒 undo。「AI に PC を触らせる怖さ」への最終回答。Gmail 送信取り消しと同じ文法で非エンジニアに説明不要。
4. **DANGER モーダルへの校正統合**（koe-sua.2 の具体化）— 高確信なら根拠 1 行、低確信なら追加確認 + 既定ボタンを[やめる]側に。**確信度がボタンの並びを変える = ラベルを読まなくても体験が変わる**。

### 6.3 非エンジニア・グローバル UX

- north-star 指標: **「インストールから最初の 60 秒で声が出る」**（SmartScreen 警告 → マイク権限 → 準備中固着、の 3 連続が現状の最悪経路 = koe-44h + koe-8t2 + koe-9wb/5fs は同じ 1 本の体験）。
- 課金 UX の欠け: 低残高の事前警告と会話中の graceful 停止（突然切断は信頼を壊す — koe-3x6 要件化）、国別通貨表示。
- **i18n（koe-mfr）が P3 のままなのは「グローバル多言語・英語圏主戦場」確定と矛盾** → P2 引き上げを提案（§9）。
- 市場順序: 英語圏先行が合理（生成 AI 個人利用率 日 26.7% vs 米 68.8%、有機的流通網も英語側）+ **日本語音声品質を品質ウェッジとして同時提供**（日本語圏の競合は薄い、note BIP は信頼資産）。

---

## 7. ロードマップ提言 — 最短 M1 → 最短初収益

### 7.1 順序の核心 3 点

1. **koe-ef8 は実質 unblocked で今日実行できる**。bd 上の blocked は形式上のもの（blocker の emd/pbe は merged 済で close 条件が ef8 自身 = 検証の循環参照）。M1 の残りは実装ではなく**検証 1 本**。Step A（Windows 起動）は 2026-06-09 に済。残り = 実マイク + 実 OpenAI 会話の verify。**唯一の前置き = W1 の 1 行修正**（E2E 中の接続ハングで脱出不能になるのを防ぐ）。
2. **koe-ds6 は ef8 の「直後の最初の実装スロット」**（E2E 1 周目が before 計測、修正後 2 周目で after — 検証が二度活きる。実装解禁はユーザーの明示指示が条件のまま）。
3. **M1.5「配布可能な製品」マイルストーンを新設**: 署名調達（`koe-44h`、即日発注級のカレンダー律速）+ 最小法務（n6s、BYOK 単体版は薄くて済む）+ 常駐（944）+ 最小オンボ（30t）。**koe-1mf の下流 issue が 0 件で、bd 上「収益への道」が途切れている**のが現ロードマップ最大の構造穴（配布できないアプリは案 A でも案 B でも売れない）。

### 7.2 初収益の経路: 「B を橋、A を本線」

| | 案 A: M4 managed credit | 案 B: Founder's License（BYOK + 応援ライセンス、MoR 経由でサーバー不要） |
|---|---|---|
| 所要（ef8 後） | **14-20 週**（backend 新設 + P/L ゲート + 安い既定 y1j/6ul が採算前提） | **5-7 週**（署名 1-2 週がカレンダー律速） |
| 売上見込み | 天井が高いが初売上は 4-5 ヶ月先 | 象徴的（3 ヶ月 ¥3-25 万）だが**捨て作業ゼロ**（署名/法務/磨きは全て M4 でも必要） |
| 戦略価値 | 本線 | **支払い意思の最速検証 + Founder ユーザーが校正データ（koe-1r1）の観測母体になる** |

- 売り方は「完成した堀」でなく「**ビジョンの早期支援者ライセンス**」と正直に（M1 時点の glass-box は thinking-event のみ。誇張は BIP の信頼資産を毀損）。
- **着手前の必須決定 = `koe-bup`**（BYOK 有料/無料の二説解消。推奨: Founder's License = 時限の応援ライセンス + M4 でクレジット特典転換、恒久 BYOK 課金は確約しない）。

### 7.3 30/60/90 日

- **Day 0-30「M1✓ + 配布可能α」**: W1 1 行 → **ef8 完走**（wire frame 採取 → bd7/nal/2br 確定、in_progress 4 件一括 close）→（解禁後）ds6+nt2+9wb ウェーブ → 署名発注（即日）→ 8h0 パイプライン + n6s 最小法務 → koe-20f 消化 + koe-bup 決定。
- **Day 31-60「Founder 初売上」**: yb4 完成（MoR=Polar + 販売ページ + デモ動画）+ 30t 最小オンボ + 944 常駐 → **販売開始（Day 40-50）**。koe-1r1 spike + koe-1l4。koe-ios はプロトタイプのみ（販売素材兼用、React 全面移植は M4 期へ）。3x6 完了 → M4 実装群の分解起票 + 1mf epic 化。
- **Day 61-90「M4 closed beta + 堀の最初の実装」**: y1j + 6ul（採算前提）→ M4 backend skeleton（ephemeral mint + 前払い ledger test mode + SMS spike）→ `koe-krv` を実価格で確定 → 1r1 が通れば **koe-sua.2（校正ラベル）着手 = 会社の本体**。**managed credit GA は Day 120-150 が現実線 — 90 日 GA を約束しない**。

### 7.4 今やる 10 / 凍結 10

**今やる**: ①koe-5fs 簡約版（1 行）②koe-ef8 ③koe-ds6+nt2（解禁後）④koe-9wb ⑤koe-20f ⑥koe-44h（署名、即日）⑦koe-n6s 最小版 ⑧koe-bup → koe-yb4 ⑨koe-3x6 + koe-krv ⑩koe-bd7+koe-nal（ef8 と同時期の wire 修正）。次点: koe-1r1 / koe-1l4 / koe-bx7 / koe-30t / koe-944。

**凍結（90 日窓）**: koe-jhk（+.1/.2/.3）/ koe-pj1 / koe-d9t / koe-v5i / koe-0yq / koe-aja / koe-5ed（3x6 に合流させる）/ koe-9ds の L4 部分（1r1 先）/ koe-sua.6 / koe-cgw（3OS 化）。koe-ios は「プロトタイプのみ 60 日窓」の中間扱い。

---

## 8. 最優先アクション Top 10（理由つき）

| # | アクション | 理由 |
|---|---|---|
| 1 | **koe-5fs 簡約版（loading 中 stop の 1 行）を ef8 前に** | E2E 中の接続ハングで脱出不能を防ぐ。backend 対応済で frontend 1 行が唯一のブロッカー（検証済✓✓） |
| 2 | **koe-ef8 完走 + 実 wire frame の採取・fixture 化** | M1 の栓。bd7/nal/2br/z8j の形状確定が全部ここに合流。in_progress 4 件一括 close |
| 3 | **koe-bd7 + koe-nal（GA 音声名 / error arm）** | 「実機で無音」「4 連鎖 silent dead」の 2 大 wire リスク。修正は各 1-数行 |
| 4 | **koe-44h コード署名の経路決定 + 調達発注（即日）** | カレンダー律速。配布できないアプリは売れない。日本個人は Azure Public Trust 不可 |
| 5 | **koe-ds6 + koe-nt2 + koe-9wb（ユーザー解禁後の最初のスロット）** | 第一印象=転換率。E2E 1 周目を before 計測に使う |
| 6 | **koe-bup 決定 → koe-yb4 を Founder's License 化** | 初収益 5-7 週の橋。支払い意思の最速検証 + 校正データ母体 |
| 7 | **koe-krv P/L ゲート確定（素材は本レポート §4 で揃った）** | M=1.8-2.0 / 安価既定ロック / 180 日失効 / 最低チャージ ¥1,500+ をプラン額として固定 |
| 8 | **koe-bx7 barge-in 最小実装（+ koe-z8j を対で）** | table-stakes #1 = M1 体験の成立条件。介入 UI として差別化と同一線上 |
| 9 | **koe-1r1 spike + koe-1l4 ワンタップ訂正** | 堀（Calibration Memory）の蓄積開始を最速化。出荷 1 ヶ月遅延 = 窓 1 ヶ月 + データ 1 ヶ月の二重損失 |
| 10 | **koe-84w 正直レポート（90 日内）** | デモ不能問題と confidence theater を同時に解く本レビュー最大の提案。BIP 素材が毎週自動生成 |

---

## 9. bd 反映済みと、ユーザー判断待ちの提案

### 反映済み（本セッション）

- **新規 19 issue**（label `review-2026-06-10`）: P1 = koe-bd7 / koe-nal / koe-whf / koe-bx7 / koe-1l4 / koe-44h / koe-bup / koe-krv、P2 = koe-z8j / koe-95z / koe-2ms / koe-r2o / koe-nak / koe-84w / koe-ysu、P3 = koe-9qd / koe-ijd / koe-38m / koe-25y
- **依存リンク 4 件**: koe-1r1←koe-ef8（blocks、READY 誤誘導の解消）/ koe-jhk.3←koe-p1a（blocks）/ koe-84w←koe-sua.3（blocks）/ koe-v5i↔koe-eal（related）
- **CLAUDE.md 事実修正 2 件**: Hermes tier 実名（Pro→Super）/ M1 tools 本数（登録 3 本 + web_search 非登録の明記）

### ユーザー判断待ちの提案（実施していない）

1. **優先度変更**: koe-yb4 P2→P1（Founder 経路採用時）/ koe-mfr P3→P2（グローバル方針との整合）/ koe-30t P2→P1（「予算無制限の明示選択」は cost_tracker 不変条件の未履行責務）
2. **koe-ds6 の保留状態の機械可読化**（label か NOTES 先頭。`bd ready` 先頭に居続け将来セッションの誤着手リスク）
3. **koe-ef8 の AC⑥（SNS 素材）分離** + 検証手順に「SQLite ファイル直接確認」を明記
4. **koe-1mf の epic 化 + M4 実装群の分解起票**（koe-3x6 完了時、issue 自身が宣言済み）
5. soft link 追加: yb4↔1mf / 6ul・y1j・7yy→3x6（収益 critical path の可視化）
6. **plan（virtual-riding-hearth.md）の stale 記述 4 件修正**: §再利用パターンの CSP 記述（CLAUDE.md は訂正済）/ M4 節の旧「2 モード対等」/ 「標準/高品質」ラベル残存（koe-45n で撤回済）/ 無料お試し「必須」（3x6 は「任意」）
7. Hermes 繰越 cap（Super $50/Ultra $100 は二次ソース、Plus 不明）— koe-3x6 設計時に公式再確認

---

## 10. 付録

### 10.1 medium/low 全所見（本文未掲載分の 1 行要約）

- [med/wiring] 手足キー UI dead-end → koe-25y（§2 W2）
- [med/wiring] 未登録 tool stub done → koe-r2o(§2 W3)
- [med/wiring] recorder 読み出し面ゼロ → koe-sh6（§2 W4）
- [med/sec] 承認/開示の対象記述子欠如 → koe-whf（§3.4、P1 扱い）
- [med/sec] 平文 SQLite → koe-2ms
- [med/test] 再接続マイク Fatal 誤分類 → koe-byf/pr3 系、ef8 シナリオ追加
- [med/test] E2E 実カバレッジ boot のみ → koe-ysu
- [med/test] vitest IPC 100% mock + casing 3 様式 → koe-5sc
- [med/test] thinking-event の価値（300-700ms の窓で知覚できるか）は実機未実証 → ef8 受け入れ基準①
- [low/sec] WebView 自己承認ネイティブ確認面 → koe-38m / call_id bound → koe-ijd
- [low/wiring] ManagedCredit stub（意図通り）/ displaySummary 言語混在 → koe-ios
- [low/test] cargo test の実 IO 境界 5 つゼロカバレッジ（自覚済・ef8 で fixture 化）

### 10.2 検証の限界（正直な注記）

- P/L の利用率分布（軽 40%/中 80%/重 100%）と転換率 3-5% は業界一般値ベースの仮定。**ベータ実測で必ず差し替える**（cost_tracker のテレメトリがそのまま使える）。
- gpt-realtime-mini の audio 単価と Grok flat 料金は二次ソース（confidence medium）。Azure/Sentry 価格は要最新確認（結論への影響軽微）。
- Codex 捕捉の X1（cancelled dispatch）は GA docs 由来の構造指摘 — 実 wire 挙動は ef8 で確定。
- 資金決済法の無償ポイント扱いと FIFO 設計は一般的整理。**規約確定時に専門家レビュー推奨**。
- 戦略主張の多重敵対検証（2 レンズ×12 主張）は週次上限で未実施。ただし競合 stress-test 自体が敵対的設計であり、P/L は追算済み、コード関連主張は全件二重検証済み。

### 10.3 主要出典（research facts、全 60+ 件から抜粋）

- OpenAI pricing / realtime-costs: developers.openai.com/api/docs/pricing, /guides/realtime-costs（2026-06-10 確認）
- Gemini Live: ai.google.dev/gemini-api/docs/pricing / Grok Voice: x.ai/news/grok-voice-agent-api（二次照合）
- Hermes/Nous Portal: hermes-agent.nousresearch.com/desktop + 公式 X 2026-04-23 発表 / Simular Sai: simular.ai/pricing
- MS: microsoft.com/microsoft-365-copilot/pricing + Windows Insider Blog 2025-11-17（Copilot Actions）
- Polar fees: polar.sh/docs/merchant-of-record/fees / Stripe JP: stripe.com/jp/pricing / Twilio: twilio.com/verify/pricing / Firebase: firebase.google.com/docs/phone-number-verification/pricing
- 資金決済法: lfb.mof.go.jp（関東財務局）+ s-kessai.jp + 6 ヶ月適用除外の実務解説
- Azure Artifact Signing FAQ: learn.microsoft.com/azure/artifact-signing/faq（個人は米/加のみ、2026-05-14 版）
- RevenueCat State of Subscription Apps 2025 / ChartMogul SaaS Conversion / Granola・Wispr Flow・Screen Studio・Cluely の各成長分析（techcrunch ほか）
- 総務省 令和 7 年版情報通信白書（生成 AI 個人利用率 日 26.7% vs 米 68.8%）

### 10.4 本レビューと過去レビューの関係

2026-06-04 包括レビュー（製品層の空白）→ 2026-06-09 UX 根因（ds6 系）→ 2026-06-10 競合地図（勝ち筋）の積み上げの上に、本レビューは「実機 wire 形状」「課金の数値検証」「ウェッジの stress-test」「収益への bd 経路」の 4 つを新規に埋めた。次の節目レビューは **ef8 完走後**（wire 確定で §3 の前提が実測に置き換わる）か **Founder 販売開始後**（P/L 仮定が実測に置き換わる）が適切。
