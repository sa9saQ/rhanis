# koe セッション決定記録 — 2026-06-09 / 06-10（設計・機能・課金の全面確定）

このセッションは **競合研究（Codex App / Hermes Desktop）を起点に、koe の機能・デザイン・課金の方向を
全面的に確定**した。**コードは未着手（設計フェーズ）**。次セッションで実装フェーズへ。
真実の源は **bd**（`bd prime` / `bd ready`）+ bd memory `koe-2026-06-10-session-decisions`。本書はその人間可読版。

---

## 0. このセッションで作った/更新した記録（インデックス）

| 種別 | 物 |
|---|---|
| 研究レポート | `docs/reviews/2026-06-09-competitor-design-research.md`（competitor 研究 + §9 ビジョン拡張） |
| 研究ドシエ | `docs/research/competitor-2026-06-09/01-11`（Codex/Hermes teardown・craft・taxonomy・voice-ux・fact-check・curation） |
| 設計ブリーフ（現行の正） | `docs/design/2026-06-10-glassbox-console-design-brief.md` |
| 設計ブリーフ（旧・superseded） | `docs/design/2026-06-09-immersive-orb-design-brief.md` |
| 本書（引き継ぎマスター） | `docs/reviews/2026-06-10-session-decisions.md` |
| bd memory | `koe-2026-06-09-vision-expansion` / `koe-2026-06-10-session-decisions` |
| SoT 更新 | `~/.claude/plans/virtual-riding-hearth.md`（2026-06-09/06-10 節）/ `koe/CLAUDE.md`（Branches/Milestones） |

---

## 1. デザイン（方向 pivot）

- **没入 orb 一本を撤回 → 「見える glass-box コンソール + 音声主役」**。現行の正 = `docs/design/2026-06-10-glassbox-console-design-brief.md`（旧 orb brief は superseded ヘッダ付与）。`koe-ios`（タイトルも更新済）。
- 理由 = 透明性は orb に隠すより**見せて整理する方が glass-box 思想に合致**＋非エンジニアに学習可能（Codex/Hermes が実証）。ただし**音声主役**を維持し text-IDE クローンにしない。**活動パネル（何をしている＋ソース）を主役**に。
- レイアウト: 左サイドバー（開閉: 新規会話/検索/プロジェクト=文脈の束/メモ・履歴/オートメ/手足tool/タスクボード、下部に**残高+⚙設定**）+ 右（案内「今日は何を?」+会話+**ライブ活動パネル=主役**+音声ボタン=縮小 orb=状態表示）。
- **確信度 = 既定で非表示**。低確信 × 重大操作の時だけ、抽象ラベルでなく**具体的で行動につながる注意**（例「この送金は取り消せません。確認しますか?」）。生%・常時「たぶん」禁止。`koe-sua.2`。研究 E2（生confidence < 作業ログ）と user 直感が一致。
- OS追従配色 / アクセント色は今後拡張可（4色固定でない）/ 多言語 UI / anti-ai-smell・WCAG AA 維持。orb craft（モーション・配色）は縮小した音声状態インジケータに流用。

## 2. ビジョン拡張

- **声のコクピット**: 全機能を抱えず **tool/MCP/エージェント委譲で「声で何でも」**（CLI操作=Codex/Claude Code も含む）。ただし**非エンジニアの既定画面には出さない = シンプル既定 + 全パワーは"詳細/上級"の奥に opt-in（削除でなく）**。`koe-0yq`。MCP = 任意tool追加機構（`koe-eal`/`dcj`/`och`）。
- **グローバル多言語**（JP-first 撤回、英語圏含め販売、言語非依存）。
- **視覚グラウンディング（指して話す）**: epic `koe-jhk`（`.1` 注釈オーバーレイ=主役 / `.2` ライブ画面共有 / `.3` 視覚指示→computer_use を DANGER承認で）。裏取り: OpenAI Realtime=画像入力対応 / Gemini Live=画面共有1FPS対応。
- **外出先チャネル常駐**: `koe-pj1`（OpenClaw 方式 = Discord/Telegram/LINE bot + Discord VC 音声繋ぎっぱなし、`koe-9uk` 具体化、コストは VAD ゲート `koe-6ul` を channel にも適用）。電話(SIP)・スマホアプリは M4+。
- **翻訳手足**: `koe-d9t`（リアルタイム音声/動画/生放送 + 文書/論文、**アプリ内テキスト字幕でも出力**、多方向 日/英/中）。専用安価モデル = GPT-Realtime-Translate（$0.034/min, GA）/ Gemini 3.5 Live Translate（preview）。

## 3. モデル / プロバイダ

- **モデルギャラリー** `koe-45n`: カード型で名前+説明+評価軸（声の自然さ/反応速度/tool安定性/作業安定性/機能/言語の質）+言語ラベル(日/英/両)+コスト+「非エンジニア向けおすすめ」+「おまかせ(自動)」。**M4「マス層はモデル名を隠す」決定を撤回**。
- **全プロバイダ対応** `koe-7yy`（trait `koe-zv3` 段階導入）: API=OpenAI Realtime/Gemini Live(`koe-y1j`)/Amazon Nova Sonic/xAI Grok Voice/InWorld、OSS=Qwen3.5-Omni/Moshi/J-Moshi(日本語全二重,名古屋大)/PersonaPlex/Nemotron、翻訳特化=GPT-Realtime-Translate/Gemini 3.5 Live Translate。
- **「300+モデル」は追わない**（Hermes はテキストだから可能、koe はリアルタイム音声=希少。koe の売りは厳選音声品質）。

## 4. 機能 / ツール（Hermes 全機能の取捨）

研究 = `docs/research/competitor-2026-06-09/07-hermes-koe-curation.md`（+08-11）。**訂正: ユーザーが見ていたのは公式 Hermes desktop（非公式 fathah でない）。公式は6テーマ。私の詳細インベントリは fathah を掘った誤り。** zh-Hant=繁体字中国語≠日本語、日本語UIは実在。

- **消費者手足パック + OAuth「接続」ボタン** `koe-v5i`（epic）: **APIキー入力は非エンジニア不可 → 各サービスは「接続」ボタン=ブラウザOAuth**。第一級の声の動詞 = 天気/カレンダー/メール/音楽(Spotify)/スマート家電(Home Assistant)/地図。**接続は多数を志向（拡張可能）**。
- **dev ツール**（CLI/GitHub/terminal/code-exec/browser自動化/MCP設定/kanban orchestration）は**既定非表示=Advanced の奥（削除でない）**。
- **設定パネル統合** `koe-0yq`（epic）: **SIMPLE DEFAULT = 非エンジニアが見るのは 6 グループ**（①残高+上限 ②声(標準/高品質) ③アシスタント(名前/話し方/always-never) ④外観(テーマ/アクセント/言語) ⑤接続(OAuth) ⑥詳細表示On/Off）+ 接続リスト。難設定（コンテキストエンジン/圧縮/補助モデル/timezone/メモリ予算）は**自動管理(UI無し)**、BYOK/メモリprovider/reasoning effort 等は **Advanced**。
- **ガイド付きペルソナ** `koe-owz`（3項目フォーム、raw SOUL.md でない）。**ルーティン** `koe-l0p`（cron 改名・平易化、timezone はOS自動）。**clarify = 中心思想**（先に「AとBどっち?」と聞く）`koe-sua`。**軽量タスク/活動ボード** `koe-3og`（看板研究、koe は1agentなので軽量版）。

## 5. 課金（全面設計）

- **統一クレジットメーター**: 前払い残高1本が **koe が運営する全有料tool（声/画像/動画/検索/翻訳）の唯一のメーター**。①ユーザー自身のアカウント(Spotify/Gmail/カレンダー/家電)=OAuth で無料 / ②koe運営の有料API=残高から原価+マージンで差し引き / ③BYOK=自分原価。`koe-3x6`/`koe-v5i`。
- **プラン構造（Hermes 下敷き）**: Free / Plus / Pro / Ultra + **「支払い額×1.10（10%ボーナス）」クレジット** + 繰越上限 + **いつでもクレジット追加購入**。10%ボーナスは retail建てなので原価安、マージンは従量メーター単価に内包。`koe-3x6`。
- **経済性（裏取り）**: 実会話1時間の koe 原価 ≈ $2（Gemini Live+VAD, $0.03-0.04/min）。API価格では**中庸**（Plus ~5-6時間/月）= バースト/タスク用途には十分・終日おしゃべりには不足。**潤沢化の本命 = 自前OSS音声ホスト（`koe-aja`）で原価10-30x減 → 同価格で桁違い**（ただし**今は前倒しせず今後のアップデート**）。
- **サブスク懸念の解消**: Hermes の $20サブスク = $22クレジット付与（前払いクレジットの月額版、青天井でない）= koe-1mf「青天井サブスク不採用」と矛盾しない。
- **オンボーディング** `koe-30t`: **ログイン壁を最初に出さない（離脱する）→ 最初っ端からデスクトップ「今日は何を?」**。SMS認証、時間制トライアルは任意。正確な¥は M4。
- **⚠️ 未確定 / 要検証**: **赤字/採算**。プラン額・クレジット量の最終確定の前に、**実価格 × 想定使用 × マージンで P/L を検証**してから（user 明言「赤字になるか調べてから」）。`koe-3x6` の M4 実装前ゲート。

## 6. koe-ds6（P0 起動高速化、着手前に戻した）

- **真因検証完了**: secret_store は IOTA Stronghold（`tauri-plugin-stronghold` 2.3.1 / `iota_stronghold` 2.1.0）。snapshot 暗号化は内部で **age の scrypt**（`stronghold_engine` 2.0.1、既定 work_factor ≈20）= 保存毎~1s の主犯。**修正 = `iota_stronghold::engine::snapshot::try_set_encrypt_work_factor(0)` を起動時に1回**（lib.rs setup、snapshot 操作前）。
- **安全**: koe鍵は 32byte CSPRNG 強鍵（`KeychainPassword`）。ライブラリ自身が強鍵は work_factor 0 を推奨（`STRONG_KEY_WORK_FACTOR=0` 存在）。**後方互換**: 復号は RECOMMENDED_MAXIMUM factor なので旧WF20も読め、次保存でWF0に書き直る。
- **状態 = 着手前(open)に戻した。実装は明示指示まで保留**（下記の教訓）。Hybrid（Claude write → Codex adversarial review）+ worktree + R-B/R-C が必要。

## 7. 教訓（このセッション）

- **「お願いします」≠ 実装着手の指示**。設計議論中の同意を実装着手と取り違え、コードを書いて止められた。**コードを書くのは「実装して/コード書いて」等の明示語がある時だけ**。曖昧な「お願いします」は直前話題への同意と解釈し、着手前に確認する。

## 8. このセッションの新規 bd issue

- **label `competitor-2026-06-09`（7）**: `koe-es8`(録音三状態+同意) / `koe-6ul`(Realtime VAD前段ゲート) / `koe-0bc`(文字起こし後音声破棄) / `koe-9jp`(発話/沈黙ルーティング) / `koe-i9a`(発話字幕+SR譲渡) / `koe-b9x`(koe doctor) / `koe-6hu`(グローバルPTT+DND)
- **label `vision-2026-06-09`**: `koe-jhk`(epic 視覚グラウンディング)+`.1`/`.2`/`.3` / `koe-45n`(モデルギャラリー) / `koe-5ed`(OSS提供+課金 decision) / `koe-7yy`(プロバイダ拡張) / `koe-pj1`(チャネル常駐) / `koe-d9t`(翻訳手足) / `koe-0yq`(epic 設定統合) / `koe-v5i`(epic 消費者手足+OAuth) / `koe-3og`(タスクボード)

## 9. 次セッション（ハンドオフ）

- **状態**: 設計フェーズ完了。コード変更ゼロ（lib.rs は HEAD 同一）。frontend typecheck 緑。ブランチ `chore/koe-2026-06-09-ux-rootcause-records`（PR #51 open）に docs+bd を積載、main 未マージ。
- **真実の源**: `bd ready` / `bd prime` / bd memory `koe-2026-06-10-session-decisions`。
- **実装に入るなら明示で**（「実装して」）。候補順: `koe-ds6`(P0 起動高速化, 検証済) → 新コンソールUIモック生成（brief から）→ ...。
- **課金は赤字検証が前提**（プラン額確定の前に P/L）。
- **未マージの注意**: このセッションの docs/bd は PR #51（docs ブランチ）に乗っている。コード実装は別 feature ブランチ + worktree + R-B/R-C で。
