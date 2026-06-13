# 製品名の確定: Rhanis （ラニス）

確定日: 2026-06-13 / 決定者: user / 記録: koe-0xy

## 結論

**フルの製品名 = 「Rhanis Agent」**（ラニスエージェント。user 確定 2026-06-13）。別表記 = Rhanis AI。
**短縮呼称・音声ウェイクワード = 「Rhanis」**（ラニス）。= 二層運用（フル名でロゴ/ストア表記、短縮で日常呼称・"Hey Rhanis"。cf. "Hey Google" / 製品名 Google Assistant）。
識別力の源は固有名詞 **Rhanis**（Agent は説明的接尾語）なので、商標出願の核は Rhanis。

**「Koe」 は全面 Rhanis に統一する（2026-06-13 user 決定で更新）**。当初案「Koe はコードネーム継続（内部識別子は koe のまま）」は撤回。フォルダ名・bd prefix・crate 名・GitHub repo まで含め `rhanis` に統一する。破壊的・大規模（特にフォルダ mv + Claude memory dir 引き継ぎでセッション断絶を伴う）ため、**次セッション（専用）で順序厳守の手順書に沿って実行**: `docs/reviews/2026-06-13-rhanis-migration-plan.md`。それまでの暫定状態では koe 識別子が残る（移行中の旧称）。

## 由来とコンセプト整合

- ギリシャ神話の水のニンフ **Rhanis**（῾Ρανίς、「水のしずく a raindrop」）。オケアノスとテテュスの娘、狩りの女神アルテミスに従うオケアニス（水の精）の一人。
- 中心思想「校正された透明性（calibrated glass-box）」と整合: **透明な一滴の水** = 隠すもののない、中が見える存在。
- 命名の系譜は Anthropic 流（神話・概念の固有名詞 = Hermes / Mythos と同じ棚）。意味は自分で語って浸透させる前提（Codex / Mythos と同型）。

## 選定経緯（要約）

Koe が音声 AI 領域で 3 製品衝突（koe.ai voice changer / koe.fm / koe.live）のため製品名に不可 → 約 50 候補を WebSearch で一次スクリーニング。確定した user 制約 = ①日本語系統は不可 ②純 CV 拍で読みが割れない（言いやすさ最優先） ③おしゃれ = 欧州ミニマル + テック洗練 + 学術ギリシャ/ラテン語の固有名詞（Codex/Anthropic/Hermes/Mythos 型） ④"◯◯ Agent / ◯◯ AI" 形式。
辞書系 clarity/light/truth/voice 語はほぼ全滅（各 2-5 の AI 製品が既使用）。user お気に入りの Selas/Fons/Krystallos/Katharos も接尾語を付けても前半固有名詞が既存 AI と衝突。同じ「水・ギリシャ語・ス終わり」音型で完全に空いていた **Rhanis** に収束。

## 検証結果（2026-06-13、一次スクリーニング）

| 項目 | 結果 | 手段 |
|---|---|---|
| AI/SaaS 製品の衝突 | クリア（ソフト製品ゼロ、神話 Wiki / WoW キャラ名のみ） | WebSearch |
| ドメイン rhanis.ai / .app / .io / .dev | 空き（NXDOMAIN） | DNS-over-HTTPS NS query |
| ドメイン rhanis.com | 登録済み・パーク中（2013 登録 / 2028 まで更新 / Network Solutions）→ 取得困難、.ai 主軸で回避 | Verisign RDAP |
| rhanisagent.com / getrhanis.com | 空き（NXDOMAIN） | DNS-over-HTTPS |
| npm `rhanis` | 空き | npm registry |
| GitHub `rhanis` | 個人ユーザーが取得済み → org は `rhanis-ai` / `getrhanis` 等で回避（cf. github.com/anthropics） | GitHub API |
| 商標 USPTO / EUIPO / J-PlatPat | 一次検索で "Rhanis" 商標の出現なし | WebSearch（DB 直接照会は未） |

## 残課題（実装・法務反映、製品名を参照する後続）

- **商標の精密調査（要・専門家）**: 称呼類似（Lannister 等の音近接、類 9 ソフトウェア / 類 42 SaaS）の最終判定は弁理士の領分。一次検索はクリアだが、出願前に専門調査を通すこと。`koe-n6s`（法務）に連動。
- **ドメイン取得**: rhanis.ai を主、rhanis.app / .io と rhanisagent.com を防御的に確保（Cloudflare Registrar、CLAUDE.md スタック方針）。
- **ハンドル確保**: GitHub org `rhanis-ai`、X `@rhanis` 系、npm scope `@rhanis`。
- **実コードへの反映は M1.5 配布タスクで段階的に**: `src-tauri/tauri.conf.json` の `productName`、バンドル識別子、`package.json` の name 等は署名証明書（`koe-44h`）・ストア登録・配布（`koe-8h0`）と整合を取る必要があるため、配布準備のまとまった作業で一括変更する（今は決定記録のみ、コード未変更）。
- **ピッチ / コピーへの反映**: `koe-20f`（競合対応・ピッチ）で "Rhanis — see what your AI is doing" 系の brand story を確定。
