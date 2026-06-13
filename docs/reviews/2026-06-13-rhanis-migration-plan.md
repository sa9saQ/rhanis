# koe → Rhanis 全統一リネーム 手順書 (rhanis-zs8)

決定: 2026-06-13 (user) / 実行: 2026-06-14 / SoT: `rhanis-zs8`（旧 `koe-zs8`）+ 本ファイル
関連: 製品名確定 = `docs/reviews/2026-06-13-product-name-rhanis.md`（`rhanis-0xy`）

## 決定（最上位）

- **製品名 = 「Rhanis Agent」**（短縮呼称・音声ウェイクワード = 「Rhanis」）。
- **「koe」は旧開発コードネーム。2026-06-13 の `rhanis-zs8` 決定で「コードネーム継続」案を撤回し、bd prefix・リポジトリ名・内部識別子も含め全て Rhanis / rhanis に全統一**（旧 `koe-52p`/`koe-0xy` の「内部は koe 継続」記述を上書き）。
- 盲目的な `s/koe/rhanis/g` は禁止。`koe-` は ①bd ID ②CSS クラス ③crate/識別子 で意味が違い、さらに ④外部競合製品 `koe.ai`/`koe.fm`/`koe.live` と ⑤リネーム対象外の研究フォルダ `~/research/koe-*` が混在するため、surface 別に処理した。

## Surface マップと処理結果

| # | Surface | 旧 | 新 | 担当 | 状態 |
|---|---|---|---|---|---|
| A | tauri productName | `koe` | `Rhanis Agent` | Claude | ✅ |
| A | tauri identifier | `com.zsaku.koe` | `com.zsaku.rhanis` | Claude | ✅ |
| A | tauri mainBinaryName | (なし) | `rhanis`（新規追加） | Claude | ✅ |
| A | tauri window title | `koe` | `Rhanis Agent` | Claude | ✅ |
| A | package.json name | `koe` | `rhanis` | Claude | ✅ |
| A | Cargo package / lib | `koe` / `koe_lib` | `rhanis` / `rhanis_lib`（+main.rs, Cargo.lock 同期） | Claude | ✅ |
| B | SQLite DB ファイル名 | `koe.db` | **据え置き**（永続ストア = 改名で会話履歴を孤児化。下記「意図的に変更しない」参照） | Claude | ✅ |
| B | Stronghold スナップショット / partition / keychain | `koe-secrets.stronghold` / `b"koe-secrets"` / `com.zsaku.koe` | **据え置き**（保存済み API キーを孤児化 + secret_store.rs に「stable across versions」契約あり） | Claude | ✅ |
| B | 設定ファイル | `koe-settings.json` | **据え置き**（設定を孤児化） | Claude | ✅ |
| B | screenshot prefix / audio thread 名 | `koe-screenshot-` / (なし) | `rhanis-screenshot-` / `rhanis-audio`（出力名・ラベル = 永続ストアでない） | Claude | ✅ |
| C | CSS クラス / DOM id / aria | `.koe-*` | `.rhanis-*`（全 *.css + className/id/aria-controls + 連動 test） | Claude | ✅ |
| C | UI 文字列 | サイドバーbrand・オンボ見出し/本文・e2e assert | `Rhanis` | Claude | ✅ |
| D | bd ID prefix | `koe-` | `rhanis-`（`bd rename-prefix`、159 件、foreign 混入 0） | Claude | ⏳ commit3 |
| D | コード/設定コメント内 bd-ID | `koe-<id>` | `rhanis-<id>` | Claude | ✅ |
| E | project CLAUDE.md / AGENTS.md | `koe` | `Rhanis`（メタ文は手動修正、研究パス保護） | Claude | ✅ |
| E | ci.yml / .gitignore コメント | `koe` | `Rhanis`（機能影響なし） | Claude | ✅ |
| E | ローカル html ドラフト | `docs/koe-*.html` | `docs/rhanis-*.html`（gitignored、mv+内容変換） | Claude | ✅ |
| F | グローバル `~/.claude` | plan / loop / lessons / memory | `Rhanis`/`rhanis-` | Claude | ⏳ |
| G | project `.claude/loop.md` | `koe` | `rhanis`（git 未追跡） | Claude | ⏳ |
| H | GitHub repo 名 | `sa9saQ/koe` | `sa9saQ/rhanis` | **user** | 🤝 handoff |
| H | git remote / `.beads/config.yaml` sync.remote | `.../koe.git` | `.../rhanis.git` | **user**（repo rename 後） | 🤝 handoff |
| H | プロジェクトフォルダ | `~/projects/koe` | `~/projects/rhanis` | **user**（WSL cwd 制約で Claude 不可） | 🤝 handoff |
| H | Claude memory dir（4個） | `-home-zsaku-projects-koe{,-3su,-ef8-ci,-src-tauri-src}` | `...-rhanis...` | **user** + 再起動 | 🤝 handoff |

## 意図的に変更しない（記録）

**原則: ローンチをまたいでユーザーデータを保持する永続ストアのファイル名 / パーティション名 / サービス名は `koe` で安定維持する**（不可視の内部識別子であり、改名は保存済みデータを孤児化するだけでブランド上の利益ゼロ）。ブランド / 識別 / 出力名のみ `rhanis` 化した。コードベース自身の文書化方針（`secret_store.rs`「a rename would orphan it」「Stable across versions」）に従う。R-B.5（CodeRabbit CLI）が CLIENT_PATH の互換破壊を major 指摘 → 本原則で対応。

- **Stronghold 資格情報ストア（3点セット）**: `koe-secrets.stronghold`（スナップショットファイル）/ `secret_store.rs::CLIENT_PATH = b"koe-secrets"`（partition）/ `lib.rs::KEYCHAIN_SERVICE = "com.zsaku.koe"`（OS キーチェーンの復号鍵サービス名）。3点は連動し、1つでも `rhanis` に変えると既存スナップショットの復号 or partition 解決に失敗し**保存済み API キーが消失**する。`secret_store.rs:29` に「Stable across versions」契約が明文化済み。**注**: バンドル識別子（tauri.conf.json `identifier`）は `com.zsaku.rhanis` に変更済み（こちらはユーザー可視の識別子）。両者は別物。
- **`koe-settings.json`**（`JsonSettingsStore`）: 音声プロバイダ / 許可ポリシー / 予算設定の永続ファイル。改名で設定が初期化される。
- **`koe.db`**（`SqliteAdapter`）: 会話ログ / ノート / コストの永続 DB。改名で履歴孤児化。
- **`.beads/metadata.json` の `dolt_database: "koe"`**: 内部 Dolt DB のストレージ名。`bd rename-prefix` は issue ID を変えるが Dolt DB 名は変えず、手で書き換えると embedded Dolt と齟齬し bd 破損リスク。対外露出なしのため**据え置き**。
- **外部競合製品 `koe.ai` / `koe.fm` / `koe.live`**: 命名研究で「koe が衝突する既存製品」として登場する第三者製品。リネームすると事実を破壊するため**保護（不変）**。
- **研究フォルダ `~/research/koe-voice-agent-novelty-2026` / `~/research/koe-integration-tech-2026-06`**: 本リネームの対象外（research アーカイブ）。パス参照は**保護（不変）**。
- **過去の研究/レビュー/設計ドキュメント（`docs/research/**`, `docs/reviews/2026-06-04..06-11`, `docs/design/**`）**: 時点記録（point-in-time）として**原則保持**。製品名 prose とともに外部 `koe.*` 参照を多数含むため盲目的置換は危険。bd-ID は旧 `koe-<id>` のまま残るが、`bd rename-prefix` 後も suffix 一致で機械的に対応可（`koe-ef8` ⇔ `rhanis-ef8`）。**全面書き換えが必要なら別途依頼**（外部 koe.* を保護した慎重なパスが必要）。

## 実行順序

1. **commit1（コード識別子+UI、A/B/C/D-comment）** ✅ — `cargo test 526 / vitest 271 / tsc` green で検証済み。
2. **commit2（docs: CLAUDE.md / AGENTS.md / ci.yml / .gitignore / 命名doc / 本手順書）** ⏳
3. **commit3（bd rename-prefix koe→rhanis + jsonl 同梱）** ⏳ — foreign 混入 0 を `bd info`/jsonl prefix 分布で照合済み。
4. **グローバル `~/.claude`（F）+ project loop.md（G）** ⏳ — 直接編集（PR 外）。
5. **R-B（review-loop skill）→ R-C（codex-review skill）→ push → PR → 自律マージ** ⏳
6. **handoff（H）= user 操作** 🤝 — 順序: GitHub rename → remote/`.beads/config.yaml` 更新 → フォルダ mv → memory dir mv → 再起動 → 検証（`bd ready` が `rhanis-` + 過去 memory 読込 + CI 緑）。step 5-6 は会話断絶を伴うため最後。

## 検証

- フロント: `pnpm test`（vitest）/ `./node_modules/.bin/tsc --noEmit`。
- Rust: `cargo test --manifest-path src-tauri/Cargo.toml`（WSL は CLAUDE.md Testing の ALSA workaround）。バイナリ名が `rhanis` になることを doc-test 出力で確認。
- 残存照合: `git grep -in koe -- '*.rs' '*.css' '*.ts' '*.tsx' '*.js' '*.toml' '*.json'` が「意図的据え置き（dolt_database / 外部 koe.* / 研究パス / 過去 docs）」以外 0。

## ロールバック

- commit1-2: `git revert` で復元可（コード/ docs）。
- commit3（bd）: `bd rename-prefix koe-` で逆変換可（Dolt DB はローカルで可逆、jsonl 再 export）。
- handoff（H）: GitHub repo は再 rename 可、フォルダ/memory は逆 mv 可。
