# koe 競合デザイン/機能研究 — Codex App & Hermes Desktop (2026-06-09)

ユーザー依頼: 「CodexApp / Hermes(Helmeth) Agent のデスクトップアプリを参考に、koe に不足している
**機能・設定・デザイン**を徹底研究」。Dynamic Workflow で 5 観点を深堀り(各一次ソース必須) →
敵対的ファクトチェック → koe 思想に照らした一次合成(計 7 エージェント、79 万トークン、14 分)。
生ドシエ全文 = `docs/research/competitor-2026-06-09/`(01〜06)。本書はそれを **既存 bd バックログと
重複排除**した上での意思決定版。

---

## 0. 一行結論(ここを外さない)

**Codex App も Hermes Desktop も「開発者向けコマンドセンター」**(チャット主役・マルチセッション・
ペイン分割・ファイルブラウザ・worktree/profile/MCP サーバ chrome)。**koe は「消費者向け音声秘書 +
校正された透明性」**で、画面は呼吸する orb 一個 + 思考の窓。

→ だから盗むのは彼らの **(a) ミクロなデザインクラフト** と **(b) プロダクト層の衛生(常駐/通知/同意/
更新/プライバシー)**。捨てるのは **彼らの情報設計(IA): サイドバー/スレッド/worktree/gateway/
パレット主導ナビ/ファイルブラウザ**。競合リスクは「彼らも透明性を持っている」ではなく
(**両者とも校正済み確信度ラベルも、音声でのリアルタイム開示も持たない**)、
「**koe が借り物の開発者 chrome の下に自分の orb を埋めてしまう**」こと。

---

## 1. 信頼性(敵対検証で割り引いた主張)

合成は以下を踏まえている(全文 = `06-fact-check.json`)。**事実として断定してはいけない**もの:

- **Hermes が「Apple風デザイン」= 幻覚**。全取得ソースに該当語なし、公式は "modern & thoughtfully
  designed UI" のみ。**Hermes の font/色/余白/モーションの具体仕様は一切主張しない**。
- **Codex のアプリ内ブラウザが「Atlas 派生」= 二次情報のみ**。公式 /features は Atlas に触れない。
- **Codex の `v26.415/417/519` バージョンタグと機能帰属 = 取得可能な changelog 範囲外(二次のみ)**。
  機能自体は実在し得るが版の帰属は未確認。Codex が Electron である点も二次のみ。
- **画像生成は `gpt-image-2`**(`1.5` は陳腐化)。**Hermes のサンドボックス backend は版で 5〜6**。

逆に**強く一次確認できた**もの(安心して使える): Codex の 3 ペイン(sidebar/thread/review)・
3 実行モード(local/worktree/cloud)・computer use(macOS 背景+「locked use」/ Windows 前景のみ /
EEA・UK・スイスで launch 時不可 / 自己改変・管理者昇格・自分の承認は**ハード禁止**)・全ショートカット・
slash コマンド・review ペイン。Hermes の 3 ゾーン+右プレビューレール・status bar の model picker +
**per-session YOLO トグル**・Cmd/Ctrl+K パレット・全 sidebar ペイン・xAI Grok OAuth・補助モデル警告・
リモート gateway(:9119)・**3 段階アンインストール**・"Choose provider later" オンボーディング・
zh-Hans 切替・半段ズーム・voice 双方向・tccutil reset 脱出路。

---

## 2. Codex App とは(検証済み)

- **正体**: 「エージェントのコマンドセンター」。複数 Codex スレッドを**並列**で回す開発者デスクトップ。
  macOS(Apple Silicon+Intel)+ Windows(MS Store)。Linux は予告のみ。
- **3 ペイン**: project sidebar(上位=プロジェクト、中=スレッド=エージェント1体、local/cloud/worktree を
  視覚区別) / active thread / review ペイン(diff: Unstaged/Staged、per-file/per-hunk の stage・revert、
  ホバー+「+」でインラインコメント、gh CLI で PR コンテキスト)。
- **タスク sidebar**(ライブ): Plan(分解ステップ)/ Sources(参照したファイル・URL)/ Artifacts
  (生成物を直接開く)/ Summary。「tool call が流れ去るのを眺めるのでなく、何をしている/作った/次に
  何をするつもりかの構造化ビュー」。→ **この "Sources" の思想だけは koe の思考の窓に取り込む価値**。
- **実行3モード**: Local / Worktree(現ブランチを別 worktree に clone して 2 環境並列)/ Cloud。
- **computer use**: 自分のカーソルで見る/クリック/タイプ。macOS は背景+並列+「locked use」(Apple 認可
  プラグインでロック画面参加、安全装置つき)、Windows は前景のみ。Screen Recording+Accessibility 権限、
  per-app「Always allow」。**ハード禁止: 端末アプリ自動化 / Codex 自身 / 管理者認証 / セキュリティ
  プロンプト承認**。EEA/UK/スイスで launch 時不可。
- **その他**: アプリ内ブラウザ(localhost プレビュー、認証フロー非対応)、画像生成(gpt-image-2)、
  メモリ、プラグイン、skills(sidebar)、automations(thread 自動化=コンテキスト保持の wake-up /
  project 自動化)、floating pop-out + stay-on-top、tray の usage-limit 表示、通知(完了/承認)、
  「Prevent sleep while running」、command palette(Cmd+K)テーマ切替、role ベースオンボーディング。

## 3. Hermes Agent Desktop とは(検証済み)

- **正体**: Nous Research の自己改善エージェント Hermes の**ネイティブ GUI**(v0.15.2 公開プレビュー
  〜2026-06-03、MIT、Electron+React が Python の hermes dashboard backend を駆動)。CLI と完全に同じ
  core/config/keys/skills/memory を共有(=「1 ランタイム・多フロントエンド」)。Win/Mac は直接インストーラ、
  Linux は端末スクリプト `--include-desktop` のみ。
- **3 ゾーン**: 左 sidebar ナビ / 中央チャット(ストリーミング+ライブ tool 活動+構造化 tool-call 要約)/
  右プレビューレール(Web ページ・ファイル・tool 出力を並べて表示)。下 status bar に inline model picker +
  **per-session YOLO トグル(危険コマンド承認を丸ごとバイパス)**。
- **sidebar ペイン**: Skills(閲覧/install/管理)/ Cron(自然言語の定期ジョブ)/ Profiles(隔離設定・
  skills・人格・モデルを切替)/ Messaging(Telegram/Discord/Slack/WhatsApp/Signal/Email — desktop で
  「8 番目のサーフェス」)/ Agents & Command Center(マルチエージェント編成)/ Sessions(アーカイブ・
  ID 検索・複数プロファイル同時・cross-profile `@session` 参照)。ファイルブラウザ(`--cwd`)。
- **設定**: Providers(OAuth、xAI Grok を第一級)/ Models(全カタログ+補助モデル分割の警告)/
  Tools/Toolsets(GUI から backend install)/ MCP servers / Gateway(リモート backend :9119、
  OAuth or basic-auth、profile ごと)/ Sessions / credential pools / backup-import / log viewer /
  network / theme / UI 言語(zh-Hans 含む)/ rebindable shortcuts / 半段ズーム / 背景更新+ワンクリック /
  **3 段階アンインストール(GUI のみ / +agent データ保持 / 全消去)**。
- **voice**: 入出力双方向(CLI と同じ)、macOS は初回マイク許可、tccutil reset 脱出路。
  ※ ただし Hermes の voice は CLI 由来の付随機能で、koe のような「音声主役」ではない。
- **オンボーディング**: 統一オーバーレイ design system、"Choose provider later"、CLI 設定の自動検出。

**示唆**: Hermes は「成熟した開発者エージェントの設定/機能の網羅チェックリスト」として最良の教材。
ただし大半は開発者ワークフロー前提で、消費者音声秘書 koe には**そのまま入れると毒**。

---

## 4. デザイン提言 — 没入型 orb(koe-ios epic 向け)

> 既存 `docs/design/2026-06-09-immersive-orb-design-brief.md` を**強化する具体値**。全て koe-ios に取り込む。

### 4-1. orb は ONE ジオメトリ。状態は「動きの種類 × 彩度」で表す(形を入れ替えない)
Siri/ChatGPT が守る最重要則: orb↔波形↔粒子を状態ごとに**入れ替えると一貫性が壊れる**。1 つの幾何を保ち、
**動きの種類 + 彩度**で読ませる。各状態に**非色の手がかり**も必須(色覚配慮 = 透明性の一部。色覚障害者に
見えない透明性は透明でない)。

| 状態 | 動きの種類 | 彩度 | 非色の手がかり | 声 |
|---|---|---|---|---|
| idle | 遅い呼吸 ~4-6s、scale 0.97↔1.03、ease-in-out-sine | 低彩度 | 微小ラベル「話しかけて」 | 無音 |
| connecting | 粒子が**内側へ収束**して orb を組成(Siri spring-out の反転) | 上昇 | 「接続中」+spinner | 無音 |
| conversing | 表面が**音声にライブで波打つ**(音声反応する唯一の状態) | 満彩度=健全 | ライブ字幕 | 会話 |
| working | 音声反応を止め、**同心円パルス/低速回転が外へ放射**(動きの"種類"を変える) | アクセント、リング色=リスク段 | 思考の窓の tool 行 | §4-6 沈黙規律 |
| reconnecting | **モノ/灰白へ退色**+低振幅の弱い脈(死んでない) | 脱彩度 | 「再接続中」 | 無音 |
| error | **脱彩度の赤へ沈静**、ほぼ静止 | 赤+アイコン+文言 | 再試行ボタン | ブロッキング時のみ短く発話 |

「退色=異常」「満彩度=健全ライブ」は**学習済みの慣習**。劣化状態に満彩度を絶対使わない。working の
パルスを**リスク段インジケータ**(SAFE 中立 / CAUTION 琥珀 / DANGER 赤リング)に兼用 = タダの透明性。

### 4-2. ウィンドウ全周のリムグロー(中央 orb 単独より強い状態信号)
Apple は中央 Siri orb を**画面端グロー**へ置換した(枠の方が非汎用で高級な状態チャネル)。koe は**両方**:
中央 orb=「AI が居る場所」、**リム halo=「いまどの状態か」**。発話開始時にグローが**下の思考の窓から
立ち上る**方向性は無料の高級ディテール。

### 4-3. 音声反応 orb の具体値(React 層へ移植可)
本番実装からのマジックナンバー: 音声レベル `level=(avg-16)/90` clamp 0-1、**FFT 1024**。
**二段平滑化**(生きてる感の核心): `levelRef += (norm-levelRef)*0.15` → `setLevel(p => p+(levelRef-p)*0.25)`。
**2 つの動きチャネルを別ゲインで**: scale `1+level*0.35`(±35%「聞いている」)、グロー opacity
`0.25+level*2.45`(~10倍「発声している」)。静的: orb 200px、blur 130px、box-shadow `0 0 90px`。
**汎用ブルー `[0.3,0.6,1]` を koe の暖色低彩度アクセントに差し替える**(下記)。

### 4-4. 色 & タイポ — Claude 系の "暖かい" 方向(Linear/Raycast の "冷たい console" を脱する)
現状のダーク管制盤は、まさにリデザインが捨てる Raycast/Linear レーン。
- **ライト地**: 純白でなく**暖かいオフホワイト `#faf9f5` 系**(Claude の "tinted cream")。
  階調 `#faf9f5 → #f5f0e8 → #efe9de`。墨 `#141413`、本文 `#3d3d3a`、減衰 `#6c6a64`。
- **ダーク地**: 暖色寄りの近黒(Linear の青黒 `#010102` は冷たいので**使わない**)。
- **低彩度アクセント 1 色**(orb コア+アクティブリム)。**シアン/電子ブルー(=汎用 AI 臭)と
  purple→blue グラデを避ける**。Claude のテラコッタ `#cc785c`(文字を載せる CTA は `#a9583e` でコントラスト
  ≥4.5:1)が良いアンカー。
- **赤は DANGER/error 専用**。アクセントを赤にしない(安全信号が死ぬ)。CAUTION=琥珀 `#d4a017`。
- **Inter を主フォントにしない**(koe の anti-ai-smell + 現地調査): Latin=**Geist / Plus Jakarta Sans**。
  JP は OS ネイティブ顔を勝たせる `"Hiragino Sans","Yu Gothic UI","Zen Kaku Gothic New",system-ui`。
  **JP 固有: `line-height:1.9`、`font-feature-settings:"palt"`、サイズを Latin 比 -10〜15%**。
- **オフセンター放射+グレイン**: 中心放射は平面的、オフセンターは「実光」に見える。`feTurbulence
  baseFrequency=0.65 numOctaves=3` のグレインを opacity 0.06 で下に敷く(banding=AI 臭を殺す。
  既存 anti-ai-smell ルールと一致)。

### 4-5. モーション — 静かが既定、ヒーローモーメントは 1 つだけ
役割別イージング(toggle 100ms/hover 150ms/modal 200-300ms/route 250-400ms、全要素 300ms 統一禁止)を維持。
Material 3 の 2 スキームに対応: **Standard(高減衰・overshoot なし)を既定**(静かな秘書)、
**Expressive の overshoot スプリングは "オンボーディングの orb 点火" の 1 回だけ**に予約。
**reduced-motion 必須**: WCAG は色/blur/opacity を "motion" 除外 → `prefers-reduced-motion:reduce` では
**遅い明度/彩度の脈だけ残す**(合法・生きてる)、状態変化は色クロスフェードに、scale 呼吸/音声変形/収束粒子/
回転は止める。**盗む 2 マイクロインタラクション**: (1) 推論中の orb への**微小な不規則ジッター(thinking
wiggle)**=spinner でなく "有機的な思考"、(2) **OS アクセント色サンプリング**(Tauri が Win/Mac の
アクセント色を読み orb コアに反映)= "OS追従配色" を light/dark より一段深く満たす。

### 4-6. 最小オーバーレイ — 同時に "うるさい" のは 1 レイヤだけ
440×680 で「ログをびっしり並べない」を守る積層則:
- **L0 orb ステージ(上 ~440px)**: orb+bloom+リム。何も競合しない。状態微小ラベルは低コントラストで下に浮かべ
  会話中はフェード。
- **L1 思考の窓(下 ~240px)**: 3 開示の**厳格な密度制限** — 現在の思考(💭1 行)+ アクティブ/直近 tool 行
  (アイコン+名+対象+✓/spinner/✕)+ 小さな確信度バッジ。**薄い直近行は最大 ~3**、全履歴は「履歴」タップ。
  **orb と同等に作り込む**(高級タイポ・抑制色・グリフ+文字)。デバッグログにしない。
- **L2 周辺メタ(端、静か)**: コスト「今月 $0.42 / 上限 $20.00」を caption サイズで隅に。設定歯車を隅に。
  **コストは常時可視・決してうるさくしない**(koe の信頼機能)。
- **L3 DANGER モーダル(orb を覆う唯一の存在)**: orb を scrim で暗転、単一決定「◯◯を実行してもいいですか?
  [許可][拒否]」+30s カウントダウンリング、fail-closed 既定=拒否。Codex の承認 UI が
  「サードパーティ Mac アプリで見た中で最高」と評された通り、**チェックリストでなく単一決定モーダル**を盗む。
- **積層法則**: idle→orb だけうるさい。conversing→orb うるさい+窓 中。working→窓 うるさい+orb 環境光。
  approval→モーダルうるさい+他暗転。

### 4-7. オンボーディング = "世界が灯る"
設定 = 点火。暗いステージに 1 点の弱い光で cold open。① 予算 ② BYOK キー を入れると**粒子が点へ収束 → orb が
点火し最初の呼吸**(Expressive スプリングをここで 1 回)。"端末内に暗号保管・外部送信しない" の安心コピーを
**orb が形成される最中に**出し、セキュリティを法的速度バンプでなく魔法の一部に。2 ステップ厳守、機能ツアー禁止。

---

## 5. 機能ギャップ表 — ADOPT / ADAPT / REJECT(意見つき)

> 「既存 bd」列 = koe バックログで既にカバー済み(=二重起票しない)。空欄=新規候補。

| 機能(出所) | 判定 | 理由 | 既存 bd |
|---|---|---|---|
| tray 常駐 + 4 状態 tray アイコン | **ADOPT** | koe は "常駐" が本体。orb 窓を閉じた時の永続アンカー | koe-944(4状態は追記) |
| 「Prevent sleep while running」トグル | **ADOPT** | 常駐音声秘書はスリープ抑止必須。安価・高価値 | koe-944(追記) |
| floating pop-out + stay-on-top | **ADOPT** | **リデザイン方向の追認** — 常時可視のフロート orb = まさに koe | koe-ios |
| グローバルホットキー召喚 + グローバル PTT/ミュート | **ADOPT** | 窓を前面化せず話す/黙らせる。`tauri-plugin-global-shortcut` | (新規) |
| 通知 3 段(never/背景のみ/常時)+ OS トースト | **ADOPT** | 目を離す前提。承認要求/完了/エラーにトースト | koe-hah(3段を追記) |
| メモリ閲覧/編集/削除 + on/off + 全削除 | **ADOPT** | 消費者信頼+GDPR/BIPA。校正メモリ L4 にも資する | koe-0k1 / koe-sua.3 |
| 3 段階アンインストール(GUIのみ/+agent/全消去) | **ADOPT** | 「会話/キーは残す?」の勾配は消費者標準 | koe-0k1(追記) |
| 会話履歴の検索(FTS) | **ADOPT** | SQLite 記録済み、検索 UI は自明な勝ち | koe-sh6 |
| テレメトリ **opt-in(既定 OFF)** + PII redaction | **ADOPT** | 音声+メモを扱う以上、明示同意必須 | koe-3ai(既定OFFを追記) |
| 自動更新 + コード署名/pubkey | **ADOPT** | 配布の table-stakes | koe-8h0 |
| i18n / UI 言語切替 | **ADOPT** | JP 第一だが UI/声ペルソナの JA/EN は必要 | koe-mfr |
| OS マイク/画面権限 UX + tccutil reset 回復路 | **ADOPT** | マイク権限 UX + 詰まり回復 | koe-8t2 / koe-8kw |
| 「koe doctor」診断 | **ADOPT** | マイク OK?/キー有効?/予算設定?/プロバイダ到達? | (新規) |
| 使用量サーフェス(分話した/¥使った/上限まで) | **ADOPT** | 前払い残高+時間併記モデルに整合 | koe-9xi拡張 |
| キャッシュ vs ライブ Web 検索トグル | **ADOPT** | キャッシュ=安価+安全な既定。予算上限消費者向けの綺麗なレバー | koe-8fw近接 |
| 機微操作 "資格情報/決済/ネットワークは立ち会って" コピー | **ADOPT** | DANGER 段の音声コピーとしてほぼ流用可 | koe-p1a |
| ハード禁止クラス(自己改変/管理者昇格/自分の承認) | **ADOPT** | 恒久 DENY。DENY_LIST 強化、自己承認の穴を塞ぐ | koe-p1a(追記) |
| ロックダウン/paranoid 1 スイッチ | **ADOPT** | fail-closed 思想に合致、Web/外部送信を制限 | koe-gap |
| ストリーミング tool 活動の可視化 | **ADAPT** | koe は thinking-event 済。だが**確信度段+ソースまで**流す(差別化、コピーでない) | koe-sua.1 |
| 確信度の可視化(CVP) | **ADAPT** | **校正 3-4 段の発話+視覚、生%禁止**。語彙チューニングが製品本体 | koe-sua.2/.3 |
| status bar の inline model picker | **ADAPT** | koe の等価=体験ラベル("標準/高品質")の声プロバイダ選択。orb 近傍に最小、dev status bar にしない | 実装済 |
| 補助モデル切替の警告 | **ADAPT** | 声プロバイダを会話中に変えると校正基線/原価が壊れる時に警告 | koe-y1j近接 |
| MCP クライアント + per-tool Allow/Ask/Block | **ADAPT** | 3 stance を SAFE/CAUTION/DANGER に重ねる。ただし**起動した MCP tool を毎回音声告知**、サーバ一覧は orb から隠す | koe-eal/dcj |
| 「このセッションは承認」スコープ | **ADAPT** | 疲労軽減。ただし fail-closed 維持、**DANGER 段には絶対適用しない** | koe-p1a |
| per-app「Always allow」一覧 | **ADAPT** | koe の folder/URL allowlist と同形。per-tool へ拡張、削除可、fail-closed | koe-351 |
| 自然言語 cron + レビューキュー | **ADAPT** | 「毎朝メモ要約して」を声で。留守中の所業をレビュー=透明性。idle-curator | koe-sua.6/koe-l0p |
| スマホから遠隔承認 | **ADAPT/DEFER** | 常駐に魅力("家の orb の DANGER をスマホ承認")だが重インフラ。M4+ | koe-9uk近接 |
| 人格/トーンプリセット | **ADAPT-注意** | 秘書の物腰は合うが、"自信家" 人格が校正された確信度の誠実信号を腐らせる。トーンを校正に従属させ上書きさせない | koe-owz |
| 軽量人格プロファイル(SOUL.md 的) | **ADAPT** | "私は誰/どう呼ぶか" の軽量プロファイルは安い勝ち。dev ルール装置は捨てる | koe-owz |
| ファイル D&D → 要約 | **ADAPT** | "ファイルを落とすと koe が読む" は秘書に合う。permission policy で gate | koe-351 |
| スクショ→エージェント/"画面見て" | **ADAPT** | 声コマンドに合うが、画面キャプチャ=機微なので CAUTION/DANGER で gate | koe-p1a |
| **per-session YOLO トグル** | **REJECT** | fail-closed と「人間が介入を判断」に真っ向対立。ワンクリック安全バイパスはブランド毒 | — |
| 並列スレッド / Git worktree | **REJECT(dev専用)** | koe は 1 本の連続会話。マルチスレッド並列はコーディング概念で orb を薄める | — |
| マルチプロファイルセッション / cross-profile @session | **REJECT(dev専用)** | パワーユーザー dev 人間工学。koe は単一の環境セッション | — |
| 作業 dir ファイルブラウザ + Git diff/stage/revert/PR ペイン | **REJECT(dev専用)** | IDE 級 review ペインは没入 orb の対極(**ソース開示の思想だけ**思考の窓に取り込む) | — |
| マルチエージェント Command Center / サブエージェント | **REJECT** | koe は 1 つの声、群れでない。command center は orb を埋める | — |
| コンテナサンドボックス backend(Docker/SSH/Modal…5-6) | **REJECT** | koe は tool をローカル async task で回す。コンテナ隔離は過剰 | — |
| メッセージング gateway(Telegram/Discord/Slack/WhatsApp/Signal/Email) | **REJECT** | 「人のように声で」に矛盾。遠未来の "要約をテキストで" は最大 1 channel | koe-9uk(補助限定) |
| 200 モデルプロバイダカタログ | **REJECT** | koe は声 2-3 を意図的に厳選。200 モデルは消費者の単純さを破壊 | — |
| プラグインマーケット(90+) | **DEFER** | M2+ の手足拡張に合うが**別設定画面**、orb 近傍にしない | koe-och |
| command palette(Cmd+K) | **DEFER/最小** | **声が koe のコマンド面**。あっても mute/stop/設定の極小、orb に従属 | — |
| エンタープライズ MDM/RBAC/OIDC | **REJECT(企業専用)** | M1-M4 消費者製品の範囲外 | — |
| アプリ内フルブラウザ埋め込み | **REJECT** | ブラウザは orb を薄める。koe の需要は "どの URL を参照したか" の表示だけ | — |

---

## 6. 設定ギャップ — 2026 成熟エージェントが持ち koe が欠くもの(消費者音声秘書に効くもの)

koe 既存: 暗号化マルチプロバイダ BYOK / 声プロバイダ選択 / 予算ハードキャップ+オンボーディング /
許可フォルダ・URL(allow+deny, fail-closed)/ コスト残高ライブ表示 / ダークテーマ。**欠落(優先順)**:

**プライバシー & データ(最重要 — koe は常時聴取レコーダ)**
- **録音同意 + "いま聴取中/ミュート/VAD 待機(マイク開・非取得)" の三状態ライブ表示**。net-new・koe 固有・
  P0。競合は誰も常時聴取レコーダではないので**この問題に直面していない**。(新規 / koe-n6s 連携)
- **メモリ/書き起こしの 閲覧・編集・削除 + マスタ on/off + 全消去**(GDPR/BIPA 形)。(koe-0k1)
- **レコーダ既定 = 文字起こし後に音声破棄(Granola 型)**、"何を保存したか" を明示+思考の窓に**ライブ表示**
  ("録音は破棄、メモのみ保存")= コンプラを思想デモに転換。(新規 / koe-bts)
- **テレメトリ opt-in(既定 OFF)+ PII redaction**(koe-3ai)。**ToS/EULA 受諾 gate**(koe-n6s)。
  **ロックダウン/paranoid トグル**(koe-gap)。

**常駐 & 通知**
- **tray 常駐 + "最小化で何が起きるか"**(WebSocket+コスト gate 維持、4 状態 tray アイコン)(koe-944)。
- **"Prevent sleep" トグル**(koe-944)。**通知 3 段 + OS トースト**(承認/完了/予算)(koe-hah)。
- **音声固有の DND 分離**: **mic-DND(聴取停止)と output-DND(発話停止だが作業継続)を別トグル** — 競合は
  分離していない。koe はすべき。(新規)
- **マイク起動モデル 3 段**(完全 open-mic / ウェイク語ゲート連続[既定] / 機微時 PTT)(koe-dcq)。

**権限の粒度**
- **per-tool Allow/Ask/Blocked**(3 段に重畳)+ **"このセッションは承認"**(DANGER は不可)(koe-p1a)。
- **ハード DENY クラス**(自己改変/管理者昇格/自分のモーダル承認)を不変として可視化(koe-p1a)。
- **地理ゲート意識**(Codex は EEA/UK/CH で computer-use を不可化。koe は EU/UK 録音 + AI Act 露出を考慮)。

**サーフェス & アクセシビリティ**
- **OS 追従テーマ**(light/dark + OS アクセント色サンプリング)= 定義的(koe-ios)。
- **UI ズーム/文字スケール**(思考の窓のテキスト、半段)(koe-ios)。**グローバル PTT/ミュート**(+最小 rebinding)(新規)。
- **koe 自身の発話のライブ字幕** = 1 機能 3 得(アクセシビリティ WCAG 1.2.1 + glass-box 監査ログ +
  スクリーンリーダー衝突の代替)(新規)。**"音声をスクリーンリーダーに譲る" モード**(koe TTS と SR の衝突)(新規)。
- **診断("koe doctor")+ ローカルログビューア**(新規)。

**人格 & コスト**
- **声ペルソナ/"どう呼ぶか" プロファイル**(校正に従属、上書きしない)(koe-owz)。
- **使用統計**(分話した/¥使った/上限まで)+ 上限状態サーフェス(koe-9xi)。

---

## 7. koe が勝つべき領域 — moat(Codex も Hermes も埋めない真空)

これは "追いつき" 項目ではなく**空のカテゴリ**。テキスト dev エージェントには連続音声も、ターンテイクも、
録音同意の重荷も、"話すか黙るか" の判断も存在しない。各々、音声主役・常駐でしか**存在しない** UX 次元:

1. **校正された "発話" 確信度(3-4 段、%でない)** = 最も鋭い刃。LLM は系統的に過信し、誤っていても断定口調。
   ユーザー信頼は**非単調** — 生 "100% 確信" も自由形式 "自信ないけど…" も**両方**信頼を毀損する。koe 自身の
   E2 で生確信度の直出力は作業ログ基線**未満**(6.5% < 7.1%)だった。勝ち筋 = 正解率に合わせた小さな段語彙を
   "うるさくなく聞かせる" = まさに koe の計画(koe-sua.2/.3、conformal/ACI)。Codex/Hermes は精々ログに
   "(low confidence)" を付すだけ。**言葉を正しくするのが全て、これを唯一やっているのが koe**。
2. **発話 vs 沈黙の規律** = テキストは全部印字するが koe は**何を声にするか選ぶ**。stakes で振る:
   SAFE→無音の画面開示+earcon / CAUTION→簡潔発話+画面詳細("URL を開きます — たぶんこれで合ってます") /
   DANGER→必ず発話(承認の問い)+モーダル。確信度は**人間の判断を変える時だけ**発話。声を疎で意味あるものに。
3. **4 状態リアルタイム環境存在**(idle/listening/thinking/speaking)。テキストは精々 spinner、ChatGPT の
   2 状態脈は listening/speaking を混同し透明性製品には不足。
4. **思考の窓 = ターン間隙そのもの(オーバーヘッドでない)**。koe は重大な PC 操作をするので臨床/金融帯の
   500-700ms ターン間隙に住む — glass-box 思想はまさにその間隙に可視窓を**必要とする**。"遅い間隙" は
   営業 bot には害だが **koe には思想通り**。300-700ms が tool ルーティング+校正の知覚的な隠れ蓑。
   **思想と音声制約が同じものを欲しがる**central convergence。
5. **常時聴取の同意を正しくやる = 信頼の濠**(BIPA/二者同意/環境録音の重罪、Otter/Fireflies/Apple-Lopez 訴訟)。
   Granola 型(ローカル取得・音声即破棄・無保存・書き起こし内同意)が唯一安全。"音声を消した/このメモを保存する"
   を**透明性窓にライブ表示** = P0 法的重荷を思想デモに転換。競合は誰も直面しない、正しくやれば機能。
6. **VAD ゲート常駐 = プライバシー=電池=コストが同一ゲート**。OpenAI Realtime に 24/7 ストリームするのは
   コスト悪夢(Wispr Flow の常時 ON は 800MB/8% CPU)+電池悪夢+不気味さ(OS マイクドット点灯)。高価な
   Realtime ストリームを**安いローカル VAD/ウェイク前段の後ろに置く**。1 つの設計判断でコスト+電池+
   プライバシーを同時に満たす。
7. **自前音声(Qwen3.5-Omni)= 透明性イネーブラ(コストだけでない)**。自前は BYOK API が出せない隠れ状態の
   確信度信号(SEP)を読める濠。コストは見出し(規模で 10-36×)、真の濠は自前だけが許す校正。
8. **OS マイクドット信頼管理 + 音声↔スクリーンリーダー共存**。常時 ON は OS ドットを常時点灯(不気味)、
   koe 自身のインジケータが OS ドットより**粒度高く信頼できる**必要。発話字幕が SR 衝突と監査ログを同時に解く。

**差別化テーゼ(一行)**: koe の校正 glass-box 思想と常駐音声制約は**収束する** — 思想が要る思考の窓は
パイプラインが要るターン間隙そのもの、校正段(生%でない)選択はまさに信頼研究が要求するもの、法が要求する
文字起こし後破棄レコーダは "何を保存したか" のライブデモ、電池/コストが要る VAD ゲートは不気味な OS マイク
ドットも縮める。**あらゆる常駐音声の制約が、koe 流に扱えば透明性を競合でなく強化する** — だからテキスト発の
機能コピーは koe を薄め、これは koe が所有すべきカテゴリ。

---

## 8. ロードマップ — bd マッピング(新規/既存追記/却下)

### 8-1. 新規起票(既存バックログに無い真の穴) — label `competitor-2026-06-09`(起票済)

| bd ID | 提案 | P | 連携 |
|---|---|---|---|
| **koe-es8** | 録音インジケータ三状態(聴取中/ミュート/VAD待機)+ 録音同意 UX | P1 | koe-n6s, koe-ios, koe-dcq |
| **koe-6ul** | Realtime WS を VAD/ウェイク前段でゲート(コスト×電池×プライバシー単一ゲート) | P1 | koe-dcq, session_manager, cost_tracker |
| **koe-0bc** | レコーダ既定=文字起こし後に音声破棄(Granola型)+ "保存物"を思考の窓にライブ表示 | P1 | koe-bts, koe-n6s |
| **koe-9jp** | 開示の発話/沈黙ルーティング(SAFE沈黙/CAUTION簡潔/DANGER発話) | P1 | koe-sua.1 |
| **koe-i9a** | koe自身の発話のライブ字幕 + スクリーンリーダー譲渡モード(字幕=監査ログ兼用) | P2 | koe-l75, koe-471 |
| **koe-b9x** | "koe doctor" 診断(マイク/キー/予算/プロバイダ到達)+ ローカルログビューア | P2 | koe-3ai |
| **koe-6hu** | グローバル PTT/ミュート ホットキー + mic-DND/output-DND 分離トグル | P2 | koe-944, koe-hah |

### 8-2. 既存 issue への追記(二重起票しない、本書を根拠に scope を足す)

- **koe-944**(常駐): 4 状態 tray アイコン / "Prevent sleep" トグル / floating pop-out+stay-on-top を scope に。
- **koe-ios**(orb redesign): §4 の具体値全部(暖色パレット #faf9f5 階調/暖色近黒、OS アクセント色サンプリング、
  thinking wiggle、リムグロー、音声反応 orb の 0.15/0.25 平滑化・FFT1024、JP タイポ palt/line-height1.9/-10-15%、
  reduced-motion=明度/彩度脈のみ、点火スプリング 1 回)。
- **koe-p1a**(risk tier): per-tool Allow/Ask/Blocked / "このセッション承認"(DANGER 不可)/ ハード DENY クラス
  (自己改変・管理者昇格・自分の承認)を不変として。
- **koe-hah**(通知): 3 段(never/背景のみ/常時)。 **koe-3ai**: テレメトリ既定 OFF + PII redaction。
- **koe-0k1**: 3 段階アンインストール + stronghold キーバックアップ。 **koe-30t**: 点火型オンボーディング。
- **koe-dcq**: barge-in E2E 受け入れ基準(<150ms 総 / <60ms TTS flush / <40ms LLM cancel / 200-300ms 最小持続ガード /
  自己発話エコーウェイク gate)。 **koe-sua.1**: 確信度段+ソースまでストリーム(tool 出力だけにしない)。

### 8-3. 明示的に却下(IA を取り込まない — orb を埋めない最重要ガードレール)

per-session YOLO トグル / 並列スレッド・worktree / マルチプロファイルセッション / ファイルブラウザ・Git ペイン /
マルチエージェント Command Center / コンテナサンドボックス / 8 メッセージング gateway / 200 モデルカタログ /
アプリ内フルブラウザ / command-palette 主導ナビ / エンタープライズ MDM。

**全項目の単一ガードレール**: P0/P1 は全て orb+透明性サーフェスを鋭くするか、見えない配管(常駐/テレメトリ/
同意)のいずれか。**dev ツール IA を 1 つも輸入しない**。リスクは「彼らも透明性を持つ」でなく
「koe が借り物 dev chrome で orb を埋める」。本ロードマップは koe = 1 orb・1 思考の窓・一度に 1 決定を保つ。

---

## 9. 追加ビジョン（2026-06-09 ユーザー対話で確定）

本研究を起点に、ユーザーが koe の像を拡張。bd label `vision-2026-06-09`、bd memory `koe-2026-06-09-vision-expansion`。

### 9-1. 声のコクピット（声で何でも = 委譲）
- koe は全機能を自前で抱えず、tool/MCP/エージェントを声で呼ぶ操縦席に。コードも Codex 劣化版にせず `koe-eal` の上に code tool（コードエージェント委譲）。
- 外出先 = OpenClaw 方式のチャネル常駐（`koe-pj1`）。Discord/Telegram/LINE bot + Discord VC 音声繋ぎっぱなし。電話(SIP)・スマホアプリは M4+。

### 9-2. モデル選択を見せる（`koe-45n`）
- M4『モデル名を隠す（標準/高品質）』を撤回。モデルギャラリー（名前+説明+言語 日/英/両+非エンジニア向けおすすめ+おまかせ自動）。

### 9-3. 全プロバイダ＋グローバル多言語（`koe-7yy`）
- JP-first 撤回、言語非依存。API=OpenAI/Gemini/Nova Sonic/Grok Voice/InWorld、OSS=Qwen3.5-Omni/Moshi/J-Moshi/PersonaPlex/Nemotron。trait `koe-zv3` で段階導入。
- 2026-06 リアルタイムモデル地図は §付録 `05-voice-alwayson-ux.md` + 本会話の追補。**J-Moshi（日本語全二重, 名古屋大）が新発見**。

### 9-4. OSS提供＋課金（`koe-5ed` decision）
- 前払い残高1本（サブスク不採用=`koe-1mf`）。koe-hosted GPU=規模後（損益分岐 月42-100h `koe-aja`）、on-device(Moshi級)=無料完全ローカル=最強プライバシー、BYO-endpoint。

### 9-5. 視覚グラウンディング（epic `koe-jhk`）= 目玉
- 注釈オーバーレイ『指して話す』(`koe-jhk.1`)/ライブ画面共有(`koe-jhk.2`)/視覚指示→computer_use(`koe-jhk.3`)。
- 裏取り: OpenAI Realtime=画像入力, Gemini Live=画面共有1FPS。
- computer_use は OSWorld 2026 で 38–79%=不安定 → 透明性+DANGER 承認で人間ループ保険（中心思想と収束）。

### 9-6. 競合追補: OpenClaw
- Peter Steinberger 2026-01 公開, GitHub 10万星(48h), Jensen Huang「個人AIの OS」。チャネル経由（WhatsApp/Telegram/Discord/LINE/iMessage…）常駐の個人 AI エージェント。シェル/ブラウザ(Playwright)自動化中心で、本物の GUI computer-use（マウスで任意アプリ操作）は Claude/OpenAI Operator/Codex/Gemini 系とは別物。koe との差は voice-first + 校正された透明性。

---

## 付録: 生ドシエ(全文)

| ファイル | 内容 |
|---|---|
| `docs/research/competitor-2026-06-09/01-codex-app-teardown.md` | Codex App 完全分解(検証ラベル付き) |
| `docs/research/competitor-2026-06-09/02-hermes-desktop-teardown.md` | Hermes Desktop 完全分解 |
| `docs/research/competitor-2026-06-09/03-design-craft.md` | 視覚/インタラクションのクラフト(Siri/Claude/Granola 等) |
| `docs/research/competitor-2026-06-09/04-feature-settings-taxonomy.md` | 成熟エージェント設定/機能の網羅タクソノミ |
| `docs/research/competitor-2026-06-09/05-voice-alwayson-ux.md` | 音声/常駐 UX(barge-in/同意/a11y) |
| `docs/research/competitor-2026-06-09/06-fact-check.json` | 敵対的ファクトチェック(割り引くべき主張) |

調査 workflow run = `wf_fc99dc69-254`(セッション内、揮発)。本書 + ドシエ 6 本 + bd issue が永続版。
