# koe 競合地図 — 正直な評価 (2026-06-10)

Dynamic Workflow（5エージェント、3スキャン+敵対検証+戦略統合）で 2026 年の競合を裏取り。
全文 = `docs/research/competitor-2026-06-09/12-16`（消費者音声/PC操作エージェント/niche+大手脅威/factcheck/戦略統合）。
bd memory `koe-2026-06-10-competitive-landscape`、戦略 issue `koe-20f`。

## 結論（率直に・誇張も安心づけもなし）

1. **音声はもう table-stakes（コモディティ）**。Copilot/Gemini/Siri/Alexa+/Perplexity/ChatGPT/Hermes 全社が実装。
   **「話せる」を差別化に使うな**（2026 のデッキで「人のように話せる」は「検索バーがある」と同じに聞こえる）。
   ※ voice-native 技術も koe 独占でない（Gemini Flash Live も gpt-realtime-2 も native speech-to-speech）。残るのは
   「**音声＝製品の背骨**（チャット欄＋マイクでない）」という設計主張だけ。
2. **3段承認ゲートも table-stakes**（Copilot/Gemini/Sai/Claude 皆ある）= 差別化に使うな。安全衛生機能として持つ。
3. **唯一空いているセル = end-user に「校正された確信度」をリアルタイム開示**。4軸採点で **校正透明性（axis3）は全社 0**。

## 4軸採点（① ターンキー ② 音声first ③ 校正透明性 ④ 常駐PC秘書）+ 脅威度

| 製品 | ① | ② | ③ | ④ | 脅威 | 一言 |
|---|---|---|---|---|---|---|
| **Microsoft Copilot (Windows)** | 3 | 2 | 0 | 2 | **最大/直接** | OS ネイティブ・無料・**koe の M1 surface(Windows)**。ただし Actions は off-by-default/opt-in=時間は買える |
| **Simular Sai** | 3 | 1 | 0 | 3 | **直接(最接近startup)** | 常駐ローカルPC操作+ターンキー。voice-first と校正だけ欠=**最重要 watch** |
| **Google Gemini Spark** | 3 | 2 | 0 | 2 | **大手/直接** | 24/7 だが cloud、Mac local file-op は summer-2026 roadmap |
| **Perplexity Personal Computer** | 3 | 2 | 1 | 2 | **直接** | Mac local + **ソース表示**=axis3 の半分に最接近 |
| **Anthropic Claude Cowork** | 2 | 1 | 0 | 2 | **直接** | 非開発者向けの常駐 coworker 路線 |
| **Apple Siri 2.0 (Gemini製)** | 3 | 3 | 0 | 1-2 | **大手潜在** | iOS 27 beta、Mac PC操作の深さ未検証 |
| **Amazon Alexa+** | 3 | 3 | 0 | 1 | **隣接** | 最強の voice-first turnkey agentic、but 家/web で **PC でない** |
| **Hermes Desktop (Nous)** | 1 | 1 | 0 | 2 | **隣接(発端)** | BYOK dev。音声追加は**category 検証**であって koe にならない |
| **ChatGPT Atlas/Agent** | 3 | 1 | 1 | 2 | **隣接** | browser/cloud。Mac voice 撤退(2026-01-15)だが **Windows は継続** |
| **Maven AGI** | — | — | **2** | — | **反例(重要)** | 唯一 校正済み confidence を実装、ただし **企業CX・内部向け・end user 非開示** |

## koe の "そのもの" は存在するか
**4軸全部を満たす単一製品は無い。最接近 = Simular Sai。** ただし軸 ①②④ は**コモディティ層（contested、急速に閉じつつある）**。
本当に空いているのは **②+③（特に ③ を end user に見せる）**。

## 透明性の堀 — 本物、ただしスコープ必須 + first-mover window
- **無条件版「校正確信度=製品0」は FALSE**。**Maven AGI** が校正済み tiered confidence（90%+/60-90%/<60%）+「Thinks Out Loud」を実装。
  救い = 企業向け support tool で **end user に raw score を見せない**。
- **正しい(真の)主張**: 「**消費者 × 音声 × PC秘書で、校正済み確信度を end user にリアルタイム開示 = 製品 0**」。
  → novelty report + `koe-sua` の「0製品」記述は**要修正**（無条件版を投資家に言うと Maven を出されて信用毀損）。
- **window**: 「Confidence UI」pattern が 2026 設計界でトレンド（実装例ゼロのカタログ）= アイデアは空気中。
  **堀はアイデアでなく実行品質**（精度一致した校正 + Calibration Memory `koe-sua.3` の AUROC 0.59→0.82 ループ）。

## 最大脅威 + 守れる優位
- **最大脅威 = Microsoft Copilot on Windows**（koe の M1 surface に OS ネイティブで ①②④ 無料投入）。
  OpenAI の Mac voice 撤退は **Windows 継続なので M1 を救わない**。Actions が opt-in/experimental なのが唯一の時間稼ぎ。
- **守れる優位 = end-user 向け校正 glass-box**。大手は構造的に出しにくい（「どれくらい自信がないか」を見せると魔法アシスタント UX を損なう）= koe の唯一の耐久ウェッジ。+ 音声=背骨(設計) + provider中立/ローカルfirst(「Microsoft でなくあなたの秘書」)。（**精緻化 2026-06-10 徹底レビュー stress-test: 「構造的に出せない」は Google double-check〔2023〜、検索照合ハイライト〕の前例があり、正確には「出しても使われなかったので再投資しない」。見せかけ確信度 UI〔confidence theater〕は数週間〜6ヶ月で模倣可能なため単独ウェッジでは弱く、防御は複合体 = provider 中立+ローカル主権 × 校正品質の実行 × 真正性。分離装置 = 正直レポート `koe-84w`。詳細 = `2026-06-10-exhaustive-review.md` §5 / `koe-20f` note**）

## 戦略 5 つ（`koe-20f` で tracked）
1. **ピッチを校正 glass-box 主役に再center**。「常駐音声PCエージェント」は支援行に降格（その見出しは Microsoft に直行、透明性見出しは空室に直行）。
2. **新規性主張を Maven 込みでスコープ修正**（deck/plan/`koe-sua`/novelty report）。
3. **校正（`koe-sua.2`/`.3`）を window が閉じる前に最速で出す＝会社の本体**。Calibration Memory ループをデモに。
4. **3段承認ゲートを差別化に売らない**（table-stakes）。
5. **定期 watch**: Simular Sai（voice-first か end-user confidence を付けたら head-to-head）+ Confidence UI pattern catalog（消費者実装例が出たら first-mover 失効）。

## 一行
あなたの懸念は**事実として正しい**（音声+常駐+PC操作はコモディティ、脅威は Hermes でも OpenAI でもなく **Windows の Microsoft**）。だが差別化は肝心な所で**腐っていない**: **end-user 向け校正 glass-box は真空**。2つだけ肝に銘じる — (1) 堀は**スコープした主張**（Maven は存在、ただし end user 非開示）(2) **永続でなく first-mover window**。**音声に頼るのをやめ、glass-box に賭け、正直にスコープし、最速で出す。**
