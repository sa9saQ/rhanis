# 校正の信号源 — confidence 入力 + outcome ground truth + 状況カテゴリの確定（koe-1r1）

**この文書が koe-sua.2（校正層と提示）/ koe-sua.3（Calibration Memory L4）の前提（SoT）。**
中心思想「校正された透明性（calibrated glass-box）」の最大の技術リスク —
「(a) モデルは自信スコアを出さない、(b) tool 成功 ≠ 答えが正しい、(c) 人間シグナルは疎」—
に対して、**何を信号として採り、何を捨て、どう束ねるか**を決める。実測検証は koe-508（ef8 実機）。

研究根拠: `~/research/koe-voice-agent-novelty-2026/report.md`（E2/E5/E6）。
関連 issue: `koe-sua.2` / `koe-sua.3` / `koe-1l4`（ワンタップ訂正、承認済）/ `koe-84w`（正直レポート）/
`koe-508`（実測 validation）/ `koe-o7z`（ConversationEvent に outcome）/ `koe-460`・`koe-z8j`（response 状態追跡）。

> 改訂履歴: v2（同日）— R-B 指摘を反映。意味層の推定を「打ち切り付き訂正レート」として定義
> （negative-only の Beta が苦情カウンタに退化する穴を閉鎖）、sua.2 の確定 user 決定
> （既定非表示・曖昧ラベル禁止）と整合化、ApprovalOutcome の timeout 混入と S1 経路汚染を分離、
> confidence farming 対策と PII 不変条件を追加。

---

## 0. 制約（設計の外周、ここは動かない）

| # | 制約 | 根拠 |
|---|---|---|
| C1 | **生 confidence（連続% / verbalized）を信号の主役にしない** | E2: verbalized AUROC 0.55 ≈ chance、成功率 6.5% < 作業ログ 7.1%。Xiong (ICLR2024) AUROC 0.522–0.605 |
| C2 | **BYOK API 経路では隠れ状態・logprobs に触れない** | OpenAI Realtime (WebSocket) は内部状態を返さない。SEP（隠れ状態プローブ）は自前ホスト経路（koe-aja、post-M1）でのみ可能 |
| C3 | **絶対校正より単調・一貫**: 確信度バンドは推定値の単調関数（境界のヒステリシス帯のみ履歴依存を許す、§1） | Li & Steyvers。一貫性が信頼の前提 |
| C4 | **cold start は保守側**（データ不足 = 低確信扱い） | safe-by-default。財務 tool の楽観 default 禁止と同じ原則 |
| C5 | **自分の行為の結果のみ記録** + **反復行為の寄与制限**: (i) 同一 tool×同一引数ハッシュの短時間反復は trials 寄与を dedup、(ii) ユーザー発話起点でない自発 tool 連打の寄与に上限 | poisoning 対策。(i)(ii) は confidence farming（自明成功の反復で p̂ を吊り上げ、低確信×重大の警告を黙らせる攻撃）への防御（R-B Phase2） |
| C6 | **会話文脈に校正ログを載せない**（別テーブル、コスト +1〜2% 以内） | koe-sua.3 既定 |
| C7 | 主張のスコープ規律: 「校正確信度 = 業界初」ではなく「**消費者×音声×PC秘書で end-user にリアルタイム開示 = 0**」 | 2026-06-10 競合研究（Maven AGI が企業内部向けに実装済）、koe-20f |
| C8 | **calibration_log に raw のユーザーデータを入れない**: `category`/`signal`/`aux` は **スカラーと列挙のみ**（tool 引数・パス・URL・転写テキスト禁止） | PII 不変条件。`call_id` で会話ジャーナルと join 可能な以上、84w（共有画像）の上流に PII を乗せない（R-B Phase2） |

---

## 1. 決定 (a) — confidence 入力源と束ね方

### D1. 実行層の主信号 = **状況カテゴリ別の自前成否統計（Beta-Bernoulli）**

モデルは使える自信スコアを出さない（C1/C2）。**実行層**（その行為は機械的に完遂したか）は
S1（§2）の成功/失敗が両方観測できるので、素直な Beta-Bernoulli が成立する:

```
p̂_exec(cat) = (α + successes) / (α + β + trials)        … Beta(α, β) 事後平均
prior: α = 1, β = 2  （事前 p̂ = 1/3 = 低確信側、C4）
```

### D2. 意味層の主信号 = **打ち切り付き訂正レート（rate、p̂ ではない）**

**意味層**（人間の意図に合っていたか）の明示シグナルは v1 では負例のみ
（deny / ワンタップ訂正）。negative-only のまま D1 の式に入れると successes ≡ 0 で
**p̂ が単調に沈み続け回復経路のない「苦情カウンタ」に退化**する（R-B Phase3 CRITICAL）。
そこで意味層は確率推定をやめ、**機会あたりの訂正レート**として定義する:

```
r̂_sem(cat) = (α' + negatives) / (α' + β' + opportunities)
  negatives     = denied + corrected                        … 明示負例（§2 S2/S3）
  opportunities = negatives + weak_positive + approved      … 「明示拒否/訂正の機会」があったターン数
                  （ボタン提示チャネル: corrected + weak_positive /
                    gate 提示チャネル: denied + approved — 両チャネルとも分子と分母が対で揃う）
  prior: α' = 1, β' = 9  （事前 r̂ ≈ 0.1 = 訂正は珍しい想定。508 で再推定）
```

- `weak_positive`（= 訂正ボタンが提示されたが押されず、同セッション内に後続訂正も無かったターン、
  §2）と `approved`（= DANGER gate でユーザーが明示承認したターン）は **分母にのみ**入る打ち切り
  観測。明示の不満が無い機会が増えるほど r̂ は経験訂正レート（不満ゼロなら → 0）へ収束する —
  **沈みっぱなしにならない**。`approved` は実行前の明示的な意図一致シグナルだが、v1 は保守側に
  倒して分母のみ（正例としての重み付けは 508 後）
- 既知バイアス: ボタンを押さない人では r̂ が過小（= 楽観側）に出る（§7-R3）。緩和は
  保守 prior + 下記束ねの cap。バイアス方向を 84w の週次集計で常時可視化する

### D3. 束ね規則（sua.2 へ渡す確信度は 1 値）

```
conf(cat) = p̂_exec(cat) × (1 − min(r̂_sem(cat), R_CAP))      R_CAP = 0.5
```

- 両因子に単調 → C3 を満たす。R_CAP は「意味層だけで confidence を 0 に潰さない」上限
  （v1 初期値、508 で調整）
- **内部バンド**（校正層の内部状態。**ユーザーに直接出すラベルではない** — 提示は D4）:
  `high: conf ≥ 0.85` / `mid: [0.5, 0.85)` / `low: < 0.5`（半開区間）
- 境界チラつき: バンド遷移にヒステリシス帯 `±max(0.03, 1/(trials+3))` を設ける
  （`trials` = 当該カテゴリの実行層 trials と意味層 opportunities の小さい方。低データ域では帯を
  広げる。帯内のみ履歴依存 = C3 の明示的例外）
- back-off の不連続防止: 子カテゴリは `trials` による shrinkage（親子推定の重み付き平均
  `w = n/(n+N_MIN)`）で滑らかに独立化する。閾値跨ぎのジャンプを作らない

### D4. 提示は sua.2 の確定 user 決定に従う（本設計はそれを変更しない）

**2026-06-10 確定**: 確信度は**既定非表示**。「たぶん」のような曖昧ラベルは**出さない**。
**低確信（内部 band = low）× 重大操作**の時だけ、具体的で行動につながる注意
（例「この送金は取り消せません。確認しますか?」）として surface する。
- 本設計の内部バンドは **surface 判定の入力**であって表示文言ではない
- 84w（正直レポート）の集計母集団も「内部 band 別の実成功率」
  （例「内部で高確信だった操作の実成功率 96%」）— "言ったラベル" ではなく内部判定で集計する

### D5. v1 で採らない入力（明示的除外）

| 候補 | 除外理由 |
|---|---|
| verbalized confidence（モデルに言わせて特徴量化） | AUROC ≈ chance（C1）。弱特徴として混ぜる案も不採用 — C3 を乱すノイズ源。E2 の轍 |
| self-consistency（複数回実行の一致度） | リアルタイム音声では遅延とコストが窓（300-700ms）を壊す。idle curator（M4）で再評価 |
| logprobs / 隠れ状態 | BYOK で取得不能（C2）。自前ホスト（koe-aja）の将来信号として §6 に予約 |
| 音声 prosody | E6 の領域（開示粒度の適応制御）であって confidence の信号源ではない |

### D6. tool 固有の補助手がかり（記録のみ、推定には混ぜない）

web_search のヒット件数、run_command の exit code 等の**スカラー**は calibration_log の `aux` に
記録するが（C8 の範囲で）、v1 の推定には使わない。寄与不明のまま混ぜると 508 の寄与分析が壊れる。

---

## 2. 決定 (b) — outcome ground truth の観測経路

outcome は 2 層: **実行層**（機械的完遂、密で自動）と **意味層**（意図適合、疎で人間由来 = 本丸）。

### シグナル台帳（v1 の採否込み）

| # | シグナル | 層 | 観測点 | 密度 | 信頼度 | v1 採否 |
|---|---|---|---|---|---|---|
| S1 | tool 実行成否 | 実行 | dispatch 経路の**分岐点を構造化して**観測（下記「S1 の汚染防止」） | 密 | 高（機械的） | **採用**（実行層の教師） |
| S2 | DANGER 承認の**明示 deny** | 意味 | `ApprovalOutcome`（`approval_gate.rs`）。**deny / timeout / チャネル断の区別が必要**（下記） | 疎（DANGER のみ。**M1 登録 tool は全部 SAFE なので M1 では密度 ≈ 0**） | 最高（実行前の明示拒否） | **採用** |
| S3 | ワンタップ訂正「それ違う」 | 意味 | **koe-1l4（承認済・未実装）** | 疎〜中 | 最高（事後の明示否定） | **採用**（意味層の主教師） |
| S4 | barge-in（応答への被せ） | 意味 | `ProviderEvent::SpeechStarted`（koe-bx7、PR #55） | 中 | 低（割り込み ≠ 否定） | **ゲート付き記録のみ**（重み 0。**応答音声の再生中（playback gate が閉じる前に音声を enqueue 済み）の speech_started だけを `interrupted` として記録** — 単なる発話開始と区別。508 で S3 との相関を見てから昇格判定） |
| S5 | 訂正発話の検出（「違う」「やり直して」） | 意味 | 転写は存在（koe-pbe）。判定器は未実装 | 中 | 中（精度未検証） | **v2 候補**（C5 と原理的に緊張する — §6 の予約参照。v1 で入れると label noise が C3 を壊す） |
| S6 | 無反応・継続使用 | 意味 | — | 密 | 最低 | **不採用**（`weak_positive` の定義（下記）に該当しないものは unlabeled） |

### S1 の汚染防止（実行層に入れて良いもの・悪いもの）

`function_call_output` の**文字列**を観測点にすると実行層が汚染される（R-B Phase1 HIGH）:
deny（`"user declined"` は `tool_dispatcher.rs` で encode）、ポリシーブロック
（`"command blocked by security policy"` 等）、未登録 tool の stub（`{"status":"tool not yet
implemented"}` は**成功形**で返る）が混入するため。**recorder seam は dispatch 経路の分岐点で
構造化された種別を受け取る**:

| dispatch 分岐 | calibration outcome | 実行層の学習 |
|---|---|---|
| 実行して成功 | `success_exec` | ○（正例） |
| 実行して失敗 | `error_exec` | ○（負例） |
| 呼び出し形成不良（tool 名長すぎ / arguments サイズ超過の検証失敗） | `error_exec` | ○（負例 — モデル起因の「機械的に完遂しない」） |
| gate **approve**（S2 の対） | `approved` | ×（意味層の分母のみ、D2） |
| gate deny（S2） | `denied` | **×**（実行していない — 意味層のみ） |
| gate timeout / チャネル断 | `approval_timeout` | ×（離席は意図の証拠ではない。重み 0） |
| ポリシーブロック（deny-list / allow-list 等） | `policy_block` | ×（重み 0。farming 検知の材料として記録） |
| 未登録 tool stub | 記録しない | ×（koe-r2o の解消対象。校正の母集団に入れない） |

実コードの分岐順序（v1 時点、`tool_dispatcher.rs`）: classify → deny-list（policy）→
arguments サイズ検証 → DANGER gate → allow-list（policy）→ registry → 実行。

### S2 の精密化: `ApprovalOutcome` は現状 deny と timeout を区別しない

現行コードは Deny・30s timeout・チャネル断をすべて `Declined`（fail-closed）に潰す — ゲートと
しては正しいが、**校正の教師としては timeout（離席）を「明示拒否」と同格にできない**。
sua.3 の実装要件として「**校正記録用に deny / timeout / チャネル断の理由を分離して流す**」を
引き渡す（`ApprovalOutcome` の拡張 or 理由の side-channel。ゲートの fail-closed 挙動は不変）。

### `weak_positive` の定義と書き込みトリガ

- 定義: **S3 の訂正ボタンが提示されたターンで、押されず、かつ同一セッション内に後続の明示訂正が
  無かった**もの（= 訂正の機会があったが明示の不満が出なかった打ち切り観測）
- 書き込み: イベントの不在で定義されるため、**セッション終了時に一括確定（バックフィル）**する
  （セッション中は pending、終了時に未訂正のものを `weak_positive` として append）
- 用途: D2 の分母のみ。508 の分布分析で識別できるよう **専用 enum 値として記録**する
  （unlabeled に潰さない — R-B Phase3 HIGH）

### ラベリング規約（§4 スキーマの enum と 1:1）

```
outcome ∈ { success_exec, error_exec,            // 実行層（S1）
            denied, corrected,                   // 意味層・明示負例（S2/S3）
            weak_positive, approved,             // 意味層・打ち切り観測（分母のみ、D2）
            approval_timeout, policy_block,      // 重み 0（記録のみ）
            interrupted,                         // S4（重み 0、ゲート付き記録）
            unlabeled }
```

- 1 行為 = 1 エピソード。キーは `call_id`（thinking-event / tool-event / approval が既に共有）
- 列の決め打ち（sua.3 実装が迷わないために）: `weak_positive` の `signal` =
  `'session_end_backfill'`、`approved`/`denied`/`approval_timeout` の `signal` = `'approval'`。
  重み 0 行の `layer`: `approval_timeout`/`interrupted` = `'sem'`、`policy_block` = `'exec'`

### 既存コードへの接続（wiring 確認、実装は sua.3）

| 観測点 | 場所 | 備考 |
|---|---|---|
| dispatch 分岐（S1 構造化） | `tool_dispatcher.rs`（classify → gate → policy → registry → 実行 の各分岐。`function_call_output` ヘルパは `realtime_types.rs`） | 文字列でなく分岐種別を流す |
| 承認解決（S2） | `approval_gate.rs` `ApprovalOutcome`（deny encode は `tool_dispatcher.rs` 側） | 理由分離は sua.3 要件 |
| 訂正ボタン（S3） | koe-1l4 の新規 IPC（frontend → Rust） | |
| barge-in（S4） | `session_manager.rs` `handle_event` の `SpeechStarted` arm（PR #55） | 「再生中」ゲートは playback gate 状態 or 直近 response の音声 enqueue 有無で判定。response 紐付けの精密化は koe-460 と共用 |

すべて Rust 側の既存 seam に乗る。`koe-o7z`（ConversationEvent に outcome/phase）と記録先は
分離: 会話ジャーナル = 履歴 UI 用、calibration_log = 校正専用（C6）。

---

## 3. 決定 (c) — 状況カテゴリ定義

E5 の核心は「カテゴリの切り方が AUROC を支配する」。47h ≈ 567 エピソード（12 行為/h）という
**データの希少さが上限**なので、v1 は粗く始めて証拠が貯まった所だけ割る:

### 階層（上から back-off 先、下が学習対象）

```
L0: global                                  … 最後の砦（cold start、C4）
L1: risk_tier ∈ {SAFE, CAUTION, DANGER}     … 3 バケット
L2: tool 名                                  … M1 登録 tool: write_note / read_file /
                                              take_screenshot（+ provider 設定時 web_search、以降増分）
L3: tool × 対象クラス                        … 例: read_file × {存在既知/未知}、
                                              web_search × {ドメイン既知/未知}、
                                              run_command × {既知コマンド allowlist への写像 | other}
```

（C8 遵守: `target_class` は常に**有限の列挙値**。run_command の生の第 1 トークンのような
raw 引数断片は保存しない — allowlist に無いものはすべて `other`）

- **v1 の学習粒度 = L2**（tool 数 ≈ 3-10 → 567 エピソードで各 50+ 件が現実的）
- L3 は **508 の実測で「L2 内の成功率分散が大きい」と確認できた tool のみ**開放（YAGNI）
- **分割と平滑化**: 子は親との shrinkage ブレンド（D3。`N_MIN = 8` は「独立寄与が支配的になる」
  目安であって階段ではない）
- **実行層と意味層は同じカテゴリ木で別々に推定**（D1/D2）
- **カテゴリは列で持つ**（`tool` / `tier` / `target_class` / `layer` — 文字列パスに埋め込まない。
  パスは読み出し時に合成）。`tier` は**行為時点の分類**を記録する — tool の tier 再分類
  （例: CAUTION 方針変更）が起きても旧行が黙って別カテゴリに分裂しない（R-B Phase3）

### 忘却（sua.3 へ引き渡す既定）

- **FIFO/recency**（E5: fifo 0.78 が最良）。**surprise 保持（|conf−outcome| 大を残す）は禁止**
  （E5: 0.47 で最悪 — 校正ヒストグラムを反転させる）
- **予算は「学習行」と「観測専用行」で分離**（R-B Phase3）: 学習行（success_exec / error_exec /
  denied / corrected / weak_positive）はカテゴリ（= tool × tier × layer）毎リング上限 512 行。
  観測専用行（approval_timeout / policy_block / interrupted / unlabeled）は**別予算**
  （全体上限 4096 行の共有リング）— 希少な S3 教師行が重み 0 行に押し出されない
- 全体上限到達時も学習行の eviction は自カテゴリのリングのみ（FIFO）

---

## 4. 記録スキーマ（sua.3 実装への引き渡し案）

```sql
CREATE TABLE calibration_log (
  id            INTEGER PRIMARY KEY,
  ts            INTEGER NOT NULL,          -- unix ms
  call_id       TEXT NOT NULL,             -- thinking/tool/approval と共通キー
  layer         TEXT NOT NULL,             -- 'exec' | 'sem'
  tool          TEXT NOT NULL,             -- L2
  tier          TEXT NOT NULL,             -- 行為時点の SAFE/CAUTION/DANGER
  target_class  TEXT,                      -- L3（開放後のみ。列挙値のみ、C8）
  predicted_p   REAL,                      -- 内部 conf（D3、その時点）。NULL = 推定前
  band          TEXT,                      -- 内部バンド high/mid/low（D3。表示文言ではない）
  outcome       TEXT NOT NULL,             -- §2 のラベリング規約と 1:1
  signal        TEXT NOT NULL,             -- 'dispatch'|'approval'|'one_tap'|'barge_in'|…
  calib_version INTEGER NOT NULL,          -- バンド境界・prior の版（R4。aux に混ぜない）
  aux           TEXT                       -- D6 のスカラー手がかり（JSON）。C8: raw 引数/パス/URL/転写 禁止
);
```

- 行は更新しない（insert のみ）。削除は §3 のリング eviction のみ
- **predicted_p と outcome の対が校正曲線の素材** — koe-84w（正直レポート）はこのテーブルの
  週次集計だけで作れる（D4: 集計は内部 band 別。C8 により共有画像経路に PII は乗らない）

---

## 5. 計測計画（koe-508 = ef8 実機、本設計の検証条件）

**前提: V2/V3 は koe-1l4（S3）の実装後にのみ実行可能**。bd で `koe-508 depends koe-1l4` を追加する。
また M1 登録 tool は全部 SAFE なので S2 の密度は DANGER tool（run_command 等）解禁後まで ≈ 0 —
508 の意味層検証は S3 が主役になる。

| 検証項目 | 合格基準（楽観値の棄却条件） |
|---|---|
| V1 行為密度 | 実機の能動使用で **≥ 8 行為/h**（E5 の 12/h が 1.5 倍以内の楽観か）。下回るなら 47h 到達予測を再計算し sua.2 の cold-start 文言を調整 |
| V2 明示シグナル率 | S2+S3 が**全エピソード（call_id 単位）**の **≥ 5%**。下回るなら S5 の v2 昇格を前倒し |
| V3 S4 の相関 | （ゲート済み）interrupted と S3 訂正の共起率 → 重み 0 から昇格するか判定 |
| V4 カテゴリ分散 | L2 バケット間の成功率分散 → L3 開放の要否 |
| V5 ログのコスト | 記録経路の遅延/容量が C6（+1〜2%）以内 |
| V6 farming 耐性 | 同一引数反復・自発連打を試行し、C5(i)(ii) の dedup/上限で p̂_exec が動かないことを確認 |
| V7 意味層の識別力 | corrected/denied を「不適合」クラス、weak_positive/approved を「推定適合」クラスとした **noisy AUROC**（注意: enum 名 `weak_positive` は AUROC 上は「不適合検出の負例」側 — 命名は校正の文脈〔意図適合 = positive〕に従う。打ち切りラベルのノイズを明記の上で参考値として測る。クリーンな sem AUROC は v1 では測定不能 — 構造的制約として記録） |

---

## 6. 将来（v1 のスコープ外、設計だけ予約）

- **自前ホスト経路（koe-aja）**: Qwen3.5-Omni の隠れ状態に SEP（線形プローブ）→ 言語化されない
  不確実性が信号化。`signal='sep'` / `aux` に載せれば**スキーマ変更なしで合流**
- **ACI 校正エンジン（koe-sua.4、M2+）**: D1/D2 のルールベース推定を、同じテーブルを教師に
  conformal/ACI へ差し替え。インターフェイス（category → conf → band）は不変
- **S5（訂正発話検出）の昇格条件**: S5 は「ユーザー発話を教師にする」ため C5 と原理的に緊張する
  （第三者音声・TV の「違う」が負例注入チャネルになる）。昇格時は **C5 の適用除外条件を別途設計**
  する（自分の直近行為への参照が確認できた発話のみ採用、等）— 本設計では予約のみ
- **応答単位の意味層 outcome**: koe-460/z8j の response 追跡が入ると「どの response が訂正されたか」
  の紐付け精度が上がる（v1 は call_id 単位で開始）

---

## 7. リスクと未決（正直に）

- **R1: 意味層は打ち切り推定（D2）の歪みを持つ**。weak_positive は「ボタンが見えていたのに
  押されなかった」であって「正しかった」ではない。押さない人では r̂ が過小（楽観側）。
  保守 prior（α'=1, β'=9）+ R_CAP + 84w の常時可視化で緩和し、**508 の実測分布（V2/V7）を見て
  prior と cap を再推定**する
- **R2: 567 エピソード ≈ 47h は単一カテゴリ木での合算値**。L2 分割後の各バケット到達はさらに遅い。
  cold start の UX（「まだ学習中」の見せ方）は sua.2 の設計項目として引き渡し
- **R3: S3 の押下率はユーザー性格に依存**（押さない人の koe は校正されない）。84w が「訂正するほど
  賢くなる」動機付けとして機能するか、508 で初期観測
- **R4: バンド境界・prior の変更はドリフトを生む**。変更は `calib_version` を上げて記録し、
  84w の週次集計はバージョン跨ぎを分離する（C3 の通時版）

---

## 8. 受け入れ条件との対応（koe-1r1 ACCEPTANCE）

- confidence 入力源: **§1 で確定**（D1 実行層 Beta / D2 意味層 打ち切りレート / D3 束ね /
  D4 提示は sua.2 確定に従属 / D5 除外 / D6 保留）
- outcome 観測経路: **§2 で確定**（台帳 + S1 汚染防止 + S2 精密化 + weak_positive 定義 +
  規約 + 既存コード接点）
- 状況カテゴリ定義: **§3 で確定**（階層 + shrinkage + 列分解 + 忘却と予算分離）
- 本文書の承認をもって `koe-sua.2` / `koe-sua.3` の前提解除、`koe-508` が実測検証を引き継ぐ
  （`koe-508 depends koe-1l4` の追加を含む）
