# koe → Rhanis 全統一リネーム 移行手順書

作成 2026-06-13 / user 決定: **内部識別子も含め全て Rhanis に統一**（旧記述「Koe はコードネーム継続」は撤回）。
実行 = **次セッション（専用）**。本手順書どおり順序厳守で。bd issue = koe-（移行で rhanis- になる、本手順 step2 参照）。

## なぜ順序が命か

末尾の 2 つ（フォルダ mv・memory dir mv）を実行した瞬間に **Claude のセッションパスが変わり再起動が必要**になる。だから「戻せる・セッションを切らない」step を先に、「破壊的・断絶を伴う」step を最後に置く。**前例の轍**: AI-Marketer → aimsel の rename で `~/.claude/projects/-home-zsaku-projects-ai-marketer` が**孤児化**して残っている（memory dir を引き継がなかった）。step6 でこれを必ず手当てする。

## 対象と現状（grep 確認 2026-06-13）

| # | 対象 | 現状値 | 規模 |
|---|---|---|---|
| 1 | tauri.conf.json `productName`/`identifier` | `"koe"` / `"com.zsaku.koe"` | 2 行 |
| 1 | package.json `name` | `"koe"` | 1 行 |
| 1 | Cargo.toml crate | `koe` / `koe_lib`（`koe_lib` import は src 内 1 箇所） | 2+1 |
| 2 | bd prefix | `koe-` | issue ID が CLAUDE.md+docs に **928 箇所** |
| 3 | GitHub repo | `sa9saQ/koe` | — |
| 4 | git remote URL | `https://github.com/sa9saQ/koe.git` | 1 |
| 5 | ローカルフォルダ | `~/projects/koe` | — |
| 6 | Claude memory dir | `~/.claude/projects/-home-zsaku-projects-koe`（+ `-koe-3su` / `-koe-ef8-ci` / `-koe-src-tauri-src`） | 4 dir |

決定: **新名 = `rhanis`**（フォルダ・crate・repo・bd prefix）。identifier = `com.zsaku.rhanis`。productName 表示 = `Rhanis Agent`。

---

## 実行順序（厳守）

### step 1 — コード識別子（Claude が PR、低リスク・いつでも戻せる）
- `src-tauri/tauri.conf.json`: `productName` → `"Rhanis Agent"`、`identifier` → `"com.zsaku.rhanis"`（※署名証明書 `koe-44h` の CN/ストア登録と最終整合。M1.5 で確定するなら identifier だけ後送りも可）
- `package.json`: `name` → `"rhanis"`（npm 規約 lowercase）
- `src-tauri/Cargo.toml`: `name = "koe"` → `"rhanis"`、`name = "koe_lib"` → `"rhanis_lib"`。`koe_lib` を使う src 内 import（1 箇所）を `rhanis_lib` に。`tauri.conf.json` の `mainBinaryName`/ビルド参照があれば追従。
- 検証: `cargo build`（ALSA workaround）+ `pnpm tauri build` でバンドル名/About 表示、`grep -rn "koe_lib\|name = \"koe\"" src-tauri/` 残 0。R-B/R-C → PR → merge。
- ※ これは既存 `koe-52p` と重複するので、step1 完了時に koe-52p を close。

### step 2 — bd prefix + docs 928 置換（Claude が PR、中リスク・置換漏れ注意）
- **まず独立 DB 確認**: `bd info` で count と DB パス（`.beads/embeddeddolt`）を見て、他プロジェクト issue が混ざっていないこと（ai-marketer 共有 planning DB 事故の轍）。
- `bd rename-prefix koe rhanis`（or 相当コマンド。`bd --help` で正確なサブコマンド確認）。実行後 `bd ready`/`bd show rhanis-ef8` で ID が変わったこと、件数不変を verify。
- docs/CLAUDE.md の 928 箇所を一括置換: `grep -rl 'koe-[a-z0-9]' CLAUDE.md docs/ | xargs sed -i -E 's/koe-([a-z0-9]{3})/rhanis-\1/g'`（**実行前にバックアップ + 実行後に件数 diff で過剰置換チェック**。"koe-44h" 等は正しく rhanis-44h に。ただし英単語中の "koe" 等を誤爆しないよう ID パターン `koe-[a-z0-9]{3}` に限定）。
- memory（`~/.claude/projects/.../memory/` + bd memory）内の `koe-` ID 参照も同様に置換 or bd remember 再保存。
- `.beads/issues.jsonl` を `bd export` し直して commit に同梱。
- 検証: `grep -rc 'koe-[a-z0-9]' CLAUDE.md docs/` が 0、`bd ready` 全件が rhanis- prefix。R-B/R-C → PR → merge。
- ※ git.md 教訓: bd 書込後 checkout は post-checkout import で巻き戻る。jsonl 同梱 + bd show verify を厳守。

### step 3 — GitHub repo rename（**user 手動**、GitHub 画面）
- GitHub → repo Settings → Rename → `koe` → `rhanis`。GitHub が旧 URL を自動リダイレクト。
- bd sync remote（`refs/dolt/data`）は repo 内 ref なので追従する見込みだが、step4 後に `bd sync` 動作確認。

### step 4 — git remote URL 更新（Claude or user）
- `git remote set-url origin https://github.com/sa9saQ/rhanis.git`（リダイレクトでも動くが明示更新）。`git fetch origin` で疎通確認。

### step 5 — ローカルフォルダ mv（**user shell 必須**、cwd 問題で Claude 不可）
```bash
# 別ターミナルで、koe フォルダの外から
cd ~/projects
mv koe rhanis
# worktree があれば一緒に（git worktree list で確認、なければ不要）
```
- これ以降、旧 `~/projects/koe` を参照する設定（CARGO_TARGET_DIR / .worktreeinclude / loop.md / .claude/settings 等）を新パスに更新。

### step 6 — Claude memory dir mv（**断絶対策、最重要**）+ Claude 再起動
- フォルダが `rhanis` になると Claude Code は新パス `-home-zsaku-projects-rhanis` でこのプロジェクトを認識し直す → **過去の auto-memory/session が新パスから見えなくなる**（前例 ai-marketer の轍）。防ぐには手で引き継ぐ:
```bash
cd ~/.claude/projects
mv -- -home-zsaku-projects-koe -home-zsaku-projects-rhanis
# 派生 dir も（worktree/CI 由来、存在すれば）
for s in 3su ef8-ci src-tauri-src; do
  [ -d "-home-zsaku-projects-koe-$s" ] && mv -- "-home-zsaku-projects-koe-$s" "-home-zsaku-projects-rhanis-$s"
done
```
- その後 **新パス `~/projects/rhanis` で Claude Code を起動し直す**。`bd prime` + memory が新パスから読めることを確認（= 移行成功の最終判定）。

---

## ロールバック / 安全装置

- step1-2 は PR なので revert 可。step3 は GitHub で再 rename 可（リダイレクト）。step5-6 は `mv` を逆に打てば戻る（破壊的だが可逆、データ削除はしない）。
- **各 step 後に検証コマンドを必ず実行**してから次へ。特に step2（928 置換）と step6（memory 引き継ぎ）は verify 必須。
- 迷ったら step1-2（コード+bd、Claude 完結・PR で安全）まででいったん止め、step3-6（破壊的）は別の落ち着いたタイミングで。

## 完了の定義

全 6 step 後、`~/projects/rhanis` で Claude を起動 → `bd ready` が rhanis- prefix で出る + 過去 memory が読める + `grep -rn '\bkoe\b' CLAUDE.md docs/ src-tauri/` がコードネーム言及（履歴記述）以外 0 + CI 緑。
