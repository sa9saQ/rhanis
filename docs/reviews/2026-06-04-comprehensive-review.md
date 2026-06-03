# koe 徹底レビュー (2026-06-04)

6 視点の並列監査 + SoT プラン精読による統合分析。実測ベース(cargo 383 + vitest 187 = 570 tests 全緑を実行確認、全関連ファイル Read、OpenAI/Gemini の現行 API 仕様もライブ確認)。

このレビューで洗い出した全項目は bd に反映済み(新規 22 issue は label `review-2026-06-04`、`bd list --label review-2026-06-04` で一覧)。

## 0. 総括

koe は **「安全クリティカルなコア(approval_gate / cost_tracker / validation / permission_policy / secret_store)が業界水準を超える品質で完成し、その周辺の "製品として箱に詰めて配る層" が構造的に空」** という偏りを持つ。

- **強い所**: BYOK 鍵の非露出、3 段安全ゲート、コスト fail-closed、path traversal/TOCTOU 防御、許可ポリシー優先順位 — クラッカー目線で潰しに行って**新規の悪用可能な穴はゼロ**。テストも「mock 緑で誤魔化す」罠に陥っていない。
- **弱い所**: ①記録が書かれず読めない(動線の断線) ②課金安全の核心が未検証(緑が嘘になりうる唯一の点) ③常駐 UX/配布/法務/監視が丸ごと未起票 ④中心思想は 1 行も実装されていない(土台のみ) ⑤オンボ UI 第一印象が生 HTML フォーム。
- **最重要の構造問題**: **プランと bd が双方向に乖離**。プランの M2-M4 中核作業(再接続/監視/配布/法務)が bd に降りておらず、逆に bd の大型 feature(議事録/STT/ペルソナ/スキルストア)がプランに反映されていなかった → 本レビューで両方向に解消。

## 1. 動線(wiring)

M1 コア音声動線(声→WS→mic/再生→function_call→分類→DANGER承認→実行→結果返却→可視化→コスト監視で停止)は**端まで配線済み**。frontend↔backend のイベント名 18 / コマンド名 18 は機械照合で**不整合ゼロ**。BYOK 鍵の保存先 `set_provider_api_key("openai")` と読込元 `get_api_key("openai_api_key")` が同一キーに解決するクロスリンクも健全。

| # | ギャップ | 根拠 | bd |
|---|---|---|---|
| 1 | **会話ログが一行も書かれない** | `log_conversation_event` 本番呼び出し 0 件。SQLite 完成(`sqlite.rs:309-342`)なのに session_manager は `save_cost_snapshot` のみ(`session_manager.rs:436,499`) | **koe-emd (新, P1)** |
| 2 | 保存メモ/履歴を見る経路がない | `write_note` 保存は✅だが `list_recent_notes` 本番 0、表示 UI なし | **koe-sh6 (新, P2)** |
| 3 | 手足(PC操作)tool の実装が空 | CAUTION/DANGER 6 tool 未実装・未登録、`tool_dispatcher.rs:326` の "not yet implemented" stub。安全ゲートは正しく先に発火 | 段階導入(koe-eal 等) |

## 2. セキュリティ/安全性

成熟度は非常に高い。M1 で実害が出ない最大の理由は **DANGER/CAUTION の実 tool が一つも実装されていない**こと(ゲートロジックは完成・副作用ゼロ)。**M1 出荷ブロッカーになる Critical/High は検出されず。** 実装が landed した瞬間に効く未追跡ギャップ:

| 深刻度 | 発見 | 根拠 | bd |
|---|---|---|---|
| 要対応 | `open_app` が policy 非対象 + CAUTION 通知のみ即実行 = 任意アプリ起動が確認なし。引数渡し実装で run_command の DENY/ALLOW 迂回経路に化ける | `permission_policy.rs:144-156` / `approval_gate.rs:153` | **koe-p1a (新)** |
| 要対応 | 禁止フォルダが実 IO 境界に反映されない = policy の禁止が「読めない」でなく「確認が出る」止まり | `tool_dispatcher.rs:288-308` / `read_file.rs:1125` | **koe-6as (scope 追記)** |
| 検討 | `write_file` の破壊強度が CAUTION(通知のみ) = 既存上書き(データ破壊)が確認なし | `approval_gate.rs:152` | **koe-p1a (新)** |

## 3. テスト/品質/CI

二極化。安全クリティカル 4 モジュール + sqlite + Stronghold crypto は overflow/timeout/race/symlink/IDN まで実物 IO で叩く厚さ。一方、実 IO 境界(usage payload・WS handshake・IPC 契約・cpal race・Windows symlink)は WSL で検証不能。

- **最大の落とし穴 = parse_usage の課金リスク**(`realtime_provider.rs:203-235`): token フィールド名が推測値(コードに「koe-ef8 で実確認」明記=未確認)。実 API と違えば**全課金 0 計上**になるが test は必ず緑。usage 欠落時は `Ignored` で継続(fail-open)。**「570 緑」が課金安全を保証していない唯一の点。** → **koe-2br を P1 昇格 + koe-ef8 acceptance に「実 usage で課金正計上」追加**。
- **CI が存在しない**: `.github` は壊れた character device、workflow 0 件。570 tests はローカル緑止まり。`pnpm-lock.yaml` は 0 バイト + 未コミット。→ **koe-0my (新, CI 起動) + koe-eco を P1 昇格**。
- **IPC/event 契約のパリティ test 不在**(両側手書き・無検証)。→ **koe-5sc (新)**。
- **koe-ef8 が不十分**だった → acceptance を ⑦実 usage 課金正計上 ⑧audio stop ⑨symlink 脱出 ⑩会話ログ記録 まで拡張。

## 4. UI/UX

ストア設計・状態機械・安全 seam・anti-AI-smell 回避・白テーマ側コントラストは品質が高い。一方「完成品」としては:

| 優先 | 問題 | 根拠 | bd |
|---|---|---|---|
| 最優先 | オンボーディング全画面が無スタイル(生 HTML)+ 設定パネルが白箱インライン挿入 | `OnboardingGate`/`BudgetOnboarding`/`ApiKeyInput` が `settings.css` 未 import、`.koe-onboarding-*` 定義 0 件 | **koe-iyr (新, P1)** |
| 最優先 | ApprovalModal にフォーカストラップ/初期フォーカス/Esc/focus restore/aria-live なし | `ApprovalModal.tsx:89-130` | **koe-471 (新, P1)** |
| 最優先 | コスト残高のライブ表示なし(cost_tracker 実装済なのに front 未読) | grep 0 件 | **koe-9xi (新, P1)** |
| 高 | 常駐 UX(tray/minimize/通知)が src 全体で 0 件 | grep 0 件 | **koe-944 / koe-hah (新)** |
| 中 | マイク権限拒否/デバイス UX が generic error | — | **koe-8t2 (新)** |

欠けている状態表現: 予算超過/オフライン/マイク権限拒否。

## 5. 中心思想「校正された透明性」

- **前提検証(ライブ API 確認済)**: OpenAI gpt-realtime-2 は preamble をデフォルト生成、Gemini Live は `includeThoughts:true` で thought summary を出す → **「Realtime API から思考を取れるか」という前提は成立**。
- **現状コードとの距離**: 中心思想の実装はコードベースに **1 行も存在しない**(`git grep thinking|confidence|calibrat` ゼロ)。土台(emit/listen・seq・bounded store・provider trait・recorder trait)は揃い増設しやすい。足りないのは縦線一本(provider.parse_frame → ProviderEvent::Thinking(新) → emit("thinking-event") → listen → store → UI → types.ts に ThinkingEvent/ConfidenceLabel)。
- **最大の技術リスク = 校正の信号源が未定義**: confidence 入力(モデルは自信スコアを出さない→koe 側のカテゴリ別統計)と outcome=ground truth(tool 成功≠答えが正しい。人間シグナル=承認 deny/訂正/無視が最有力だが観測が疎)の両取得経路が未設計。**ここが崩れると E2/E5 の数値が実機で再現せず思想が「見た目だけ校正済み」になる**。→ **koe-1r1 (新, P1) を koe-sua.2/.3 の前提 design spike として起票**。
- **段階導入の注意**: koe-sua.2(校正ラベル UI)を .3/.4(校正データ)より先に出すと E2 が警告する「校正なき自信表明」を踏む。.4 完成まで `enableCalibratedLabels=false` で内部に留める。koe-sua.1 で types.ts に `confidence?` を optional 予約しておくと .2 の型破壊変更を避けられる。

## 6. bd バックログ網羅性

37 active issues、孤児 0/循環 0 は健全だが「製品をリリースできる」状態を網羅していなかった。本レビューで新規 22 issue + 衛生修正を反映:

**依存衛生**: koe-zv3 を close(PR1 merged、PR2=koe-y1j に分離)。koe-y1j/koe-e2b に koe-zv3 への lineage、koe-2br に koe-ef8 依存を付与。

**新規起票 22 件(label `review-2026-06-04`)**:

| 領域 | issue |
|---|---|
| M1 完成 | koe-emd(会話ログ配線) / koe-iyr(onboarding style) / koe-471(ApprovalModal a11y) / koe-9xi(コスト残高表示) / koe-0my(CI) / koe-5sc(IPC parity test) / koe-8t2(マイク権限/デバイス UX) / koe-30t(初回チュートリアル+無制限必須選択) |
| M1 思想前提 | koe-1r1(校正の信号源確定 = sua.2/.3 前提) / koe-6af(プロダクト名/アイコン/ブランド) |
| M2 運用安定/常駐 | koe-byf(WS 自動再接続) / koe-3ai(observability+Sentry) / koe-944(トレイ常駐+autostart) / koe-hah(OS 通知) / koe-sh6(会話履歴 UI) / koe-4cw(Audio Preview フォールバック) / koe-p1a(手足tool risk tier 再設計) |
| M3 | koe-0k1(データエクスポート/全削除) / koe-2sm(設定バックアップ・移行) |
| M4 | koe-n6s(プライバシー/規約/録音同意) / koe-8h0(updater+コード署名 配布) / koe-yb4(買い切りモード A) |

## 7. クロスカッティング最重要(複数視点が同じ箇所を指す = 高信頼)

1. **parse_usage 課金リスク**(security+test) — 570 緑が嘘になる唯一の点。→ koe-2br P1 + koe-ef8 拡張。
2. **記録が書かれず読めない**(wiring+UI+bd) — 書く配線(koe-emd)も読む UI(koe-sh6)も無い三重一致。
3. **コスト残高が見えない**(UI+bd) — koe-9xi。
4. **常駐 UX(トレイ/通知)が丸ごと無い**(UI+bd) — koe-944/koe-hah。
5. **CI 無し+lockfile 空**(test+bd) — koe-0my/koe-eco。

## 8. 次セッションの実装順(推奨)

1. **M1 完成の前提 3 点を先に**(koe-ef8 が現在これらで blocked): koe-emd(会話ログ配線・1 行配線) → koe-9xi(コスト残高表示) → koe-iyr(onboarding style)。並行で koe-471(a11y)/koe-5sc(parity test)/koe-0my(CI)。
2. **koe-ef8(Windows 実機 E2E)**: ⑦実 usage payload 採取(koe-2br の前提)/⑧audio race(koe-pr3)/⑨symlink(koe-8kw)を同セッションで。
3. **中心思想 M1**: koe-1r1(信号源確定 spike)→ koe-sua.1(thinking-event)。実機で E1/E2 を早期 A/B。
4. M2 以降: 再接続(koe-byf)→ 常駐(koe-944/koe-hah)→ observability(koe-3ai)→ 校正層(koe-sua.2/.3/.4)。

---

*監査手法: 並列 6 エージェント(セキュリティ/動線/UI・UX/テスト/bd 網羅性/中心思想ギャップ)+ SoT プラン精読。全発見は file:line 根拠付き、実測(cargo/vitest 実行、API ライブ確認)に基づく。*
