I have comprehensive, verified data from authoritative sources. Compiling the structured output now.

---

# HERMES SETTINGS/UX DATA — MEMORY, CONTEXT, APPEARANCE, PERSONA

Scope note: Two products are conflated in the prompt. (A) **OFFICIAL Hermes Agent** (NousResearch) — CLI + dashboard + config.yaml; this is where granular keys live. (B) **UNOFFICIAL fathah/hermes-desktop** — an Electron desktop GUI wrapping the same agent backend; it exposes a subset of (A)'s config as visual settings. Where the desktop GUI shows a value, it maps to a config.yaml key from (A). Discrepancies between the two are flagged.

---

## 1. MEMORY & CONTEXT

### 1a. Persistent memory (built-in)
- CONFIRMED — Built-in memory is two files in `~/.hermes/memories/`: `MEMORY.md` (agent's curated memory) and `USER.md` (user profile). Source: https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/configuration.md
- CONFIRMED — Config block and defaults:
  - `memory.memory_enabled: true` — toggle persistent agent memory on/off.
  - `memory.user_profile_enabled: true` — toggle the user-profile model on/off.
  - `memory.memory_char_limit: 2200` (~800 tokens) — **this is the "memory budget"** the user referenced. Cap on agent-memory size injected into the prompt.
  - `memory.user_char_limit: 1375` (~500 tokens) — **this is the "profile budget."** Cap on user-profile size injected.
  - Source: https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/configuration.md
- PLAIN TERMS: The agent keeps a small scratch-notepad about your projects (MEMORY.md) and a small dossier about you (USER.md). The two "budgets" are character caps so these notes don't eat the whole context window. Built-in memory is ALWAYS on alongside any external provider.

### 1b. MEMORY PROVIDERS (user said "3" — actual = 9 plugins + built-in)
- CONFIRMED — There are **9 external memory-provider plugins**, only **one active at a time**, always alongside built-in MEMORY.md/USER.md. Source: https://hermes-agent.nousresearch.com/docs/user-guide/features/memory-providers
- CONFIRMED full list (exact names): **Honcho** (cloud, dialectic user-modeling), **OpenViking** (self-hosted), **Mem0** (cloud, fastest setup), **Hindsight** (cloud/local, stores structured facts/entities not text chunks), **Holographic** (local), **RetainDB** (cloud), **ByteRover** (local/cloud), **Supermemory** (cloud), **Memori** (cloud). Source: https://hermes-agent.nousresearch.com/docs/user-guide/features/memory-providers
- CONFIRMED — Config key: `memory.provider: <name>`; also selectable via `hermes plugins → Provider Plugins → Memory Provider`. Source: same.
- INFERRED — The user's "3 memory providers" is almost certainly **what the desktop GUI dropdown currently surfaces as installed/discoverable**, not the full 9. The fathah/hermes-desktop README lists "Discoverable memory providers (Honcho, Hindsight, Mem0, RetainDB, Supermemory, ByteRover)" = 6 discoverable; a fresh desktop install likely shows ~3 (e.g., **None/Built-in + Honcho + Mem0**, the two zero/low-friction cloud ones) until others are installed. The "3" is a UI-state artifact, NOT the real ceiling. Source (6 discoverable): https://github.com/fathah/hermes-desktop
- PLAIN TERMS: "Memory provider" = an optional external brain that remembers across sessions more richly than the local .md files. Honcho models *how you think*; Mem0 is the quick plug-and-play one; Hindsight stores hard facts/relationships. Only one can be on.

### 1c. CONTEXT ENGINE (user said "3" — actual = "compressor" default + plugin engines)
- CONFIRMED — `context.engine: "compressor"` is the **default** (built-in lossy summarization). Source: https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/configuration.md and https://hermes-agent.nousresearch.com/docs/developer-guide/context-compression-and-caching/
- CONFIRMED — Alternatives are **plugin engines**, e.g. `"lcm"` (lossless context management). Plugin engines are **never auto-activated** — must be explicitly configured. Resolution: checks plugins first, falls back to built-in compressor. Source: https://hermes-agent.nousresearch.com/docs/developer-guide/context-compression-and-caching/
- INFERRED — The "3 context engines" in the desktop dropdown = **the built-in `compressor` + however many engine plugins are discoverable/installed** (likely something like: `compressor` (default) / `lcm` / `none`-or-passthrough). The exact 3 desktop labels are NOT confirmed in any source; treat "3" as install-state, with `compressor` being the only one a non-engineer should ever leave selected.
- PLAIN TERMS: The context engine is the strategy for what to do when the conversation gets long. Default ("compressor") summarizes old turns to save room. Alternatives are advanced/experimental plugins.

### 1d. AUTO-COMPRESSION (all user-cited numbers CONFIRMED)
- CONFIRMED config block + defaults (`compression:`):
  - `enabled: true` — master on/off for compression.
  - `threshold: 0.50` — **compression trigger position ≈ 0.5 CONFIRMED.** Fires when prompt tokens reach 50% of the model's context window. `threshold_tokens = threshold × context_length`.
  - `target_ratio: 0.20` — **the "compression target."** Fraction of the threshold preserved as the recent uncompressed tail (~20%).
  - `protect_last_n: 20` — **protect last N = 20 CONFIRMED.** Minimum most-recent messages never compressed. Tail protection is token-budget-based: walks backward accumulating tokens, falling back to this fixed count of 20.
  - `protect_first_n: 3` — pins the first 3 non-system head messages across compactions (hardcoded-ish; preserves opening context).
  - `hygiene_hard_message_limit: 400` — gateway safety valve.
  - Source: https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/configuration.md and https://hermes-agent.nousresearch.com/docs/developer-guide/context-compression-and-caching/
- CONFIRMED — Auxiliary compression model: `auxiliary.compression.model: ""` (empty = use main chat model), `provider: "auto"`. Source: configuration.md.
- PLAIN TERMS: When the chat fills to ~50% of capacity, Hermes auto-summarizes the older middle of the conversation, keeps the newest ~20 messages verbatim, and keeps the first 3. "Protect last 20" guarantees recent context isn't lossy-summarized.

### Memory/Context — non-engineer touch verdict
- AUTO-MANAGE / HIDE: memory budget, profile budget, threshold, target_ratio, protect_last_n, protect_first_n, context engine selection. These are tuned-for-you defaults; wrong values silently degrade quality or cost. A non-engineer should never touch these.
- OPTIONAL (single toggle worth exposing): "Use external memory provider?" with a curated 2–3 choice (None / Mem0 / Honcho) — that's the one memory decision a non-engineer might reasonably make (privacy + cross-session recall). Everything else: hide behind "Advanced."

---

## 2. APPEARANCE

### 2a. LANGUAGE — user's confusion ("zh-Hant = Japanese") is WRONG
- CONFIRMED — `display.language` default `en`. **Japanese IS supported** as a separate code `ja`. zh-Hant is **Traditional Chinese**, NOT Japanese. Source: https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/configuration.md
- CONFIRMED full supported list (16): `en` English (default), `zh` Simplified Chinese, `zh-hant` Traditional Chinese, `ja` Japanese, `de` German, `es` Spanish, `fr` French, `tr` Turkish, `uk` Ukrainian, `af` Afrikaans, `ko` Korean, `it` Italian, `ga` Irish, `pt` Portuguese, `ru` Russian, `hu` Hungarian. Unknown values fall back to English. Source: same. (Japanese localization landed in desktop v0.5.2; zh-TW/pt-PT in v0.4.5 — Source: https://github.com/fathah/hermes-desktop/releases)
- CONFIRMED scope caveat: `display.language` translates **static UI strings only** (approval prompts, gateway replies) — it does NOT translate the agent's actual responses or tool output. Source: https://hermes-agent.nousresearch.com/docs/user-guide/configuration
- CORRECTION TO RELAY: zh-Hant (繁體中文) = Traditional Chinese. Japanese = `ja`, a distinct entry. The user mis-mapped two different rows. Both exist independently in the list.

### 2b. THEME (Light/Dark/System) — split CONFIRMED/INFERRED by surface
- CONFIRMED (Web dashboard): an Appearance pane with a "Theme card" toggling **Light / Dark / System**, plus a Skin grid for accent palettes. Source: https://hermes-agent.nousresearch.com/docs/user-guide/features/skins (via search summary)
- CONFIRMED (CLI): no Light/Dark/System toggle; CLI uses named **skins** instead (light-oriented ones: `daylight`, `warm-lightmode`). Source: same.
- INFERRED (fathah desktop): the desktop GUI exposes Theme = Light/Dark/System (it inherits the dashboard model and v0.5.6 added "Multiple themes"). Source for "Multiple themes": https://github.com/fathah/hermes-desktop/releases

### 2c. ACCENT COLOR names — TWO distinct lists; don't conflate
- CONFIRMED (fathah/hermes-desktop GUI accent themes): **Midnight, Ember, Mono, Cyberpunk, Rose** — "only change the colour scheme of the site." Source: web result citing the desktop themes (https://github.com/NousResearch/hermes-agent/issues/18080 thread + https://hermes-agent.nousresearch.com/docs/user-guide/features/skins).
- INFERRED — User-cited "Amber" and "Slate" are likely (a) a different desktop version's palette, or (b) bleed-over from the CLI skin list. Not confirmed in the current desktop 5-theme set (Midnight/Ember/Mono/Cyberpunk/Rose). "Mono" and the cyber/red/rose families overlap conceptually with CLI skins, which is the probable source of confusion.
- CONFIRMED (CLI/dashboard built-in SKINS — different namespace, 9–10 named skins): `default` (gold + kawaii), `ares` (crimson/bronze), `mono` (grayscale), `slate` (cool blue, dev-focused), `daylight` (light), `warm-lightmode` (warm gold light), `poseidon` (deep blue/seafoam), `sisyphus` (austere grayscale), `charizard` (volcanic orange/ember), `cyberpunk` (neon: banner_border #FF00FF, banner_title #00FFFF). Switch via `/skin`; custom skins = YAML in `~/.hermes/skins/<name>.yaml`. Source: https://hermes-agent.nousresearch.com/docs/user-guide/features/skins
- KEY DISTINCTION: Desktop GUI "accent color" (Midnight/Ember/Mono/Cyberpunk/Rose) is a **cosmetic CSS palette** of the app chrome. CLI "skins" (mono/slate/ares/poseidon/…) are a **richer terminal theme system** (also sets spinner verbs, banner text, thinking verbs). The user is looking at the former.

### 2d. Tool-call display ("technical" vs simple)
- CONFIRMED — config key `display.tool_progress`, default `all`. Options: `off` | `new` | `all` | `verbose`. Source: https://hermes-agent.nousresearch.com/docs/user-guide/configuration
  - `off` — final answer only (simplest).
  - `new` — show only when the tool changes.
  - `all` — every tool call with a short preview (default).
  - `verbose` — full args, raw results, debug logs ("technical" mode).
- CONFIRMED related: tool calls/reasoning/tool output render as **collapsible sections** in desktop chat (v0.5.2). `display.tool_progress_command: false`, `display.show_reasoning: false`, `display.tool_preview_length: 0` are adjacent toggles. Source: configuration.md + https://github.com/fathah/hermes-desktop/releases
- MAPPING: the user's "technical" toggle = `verbose` (or the `show_reasoning`/raw-output toggles). "Simple" = `new` or `off`.

### Appearance — non-engineer touch verdict
- EXPOSE (safe, cosmetic, expected): Language, Theme (Light/Dark/System), Accent color. Zero risk, pure preference.
- EXPOSE with a sane default: Tool-call display — but relabel for non-engineers ("Show what the agent is doing: Off / Summary / Detailed"). Default `all` is fine; `verbose`/technical is for power users.
- LANGUAGE CAVEAT to surface in UI: tell users it changes UI chrome, not the agent's reply language (common confusion).

---

## 3. PERSONALITY / PERSONA

- CONFIRMED — Persona lives in `~/.hermes/SOUL.md` (slot #1 of the system prompt). The desktop has a dedicated **"Soul" screen**: "Edit and reset your agent's SOUL.md personality." Custom instructions = free-text in SOUL.md, not a discrete config key. Sources: https://github.com/fathah/hermes-desktop and https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/configuration.md
- CONFIRMED — `display.personality` config key, default value `"kawaii"` (this is the *named built-in persona/tone preset*, distinct from the free-text SOUL.md). The `default` skin is described as "gold and kawaii." Source: configuration.md + skins doc.
- CONFIRMED — `agent.reasoning_effort` (`none|minimal|low|medium|high|xhigh`, empty=medium) is adjacent and exposed in desktop as a "reasoning effort picker" (v0.5.8). Source: configuration.md + https://github.com/fathah/hermes-desktop/releases
- FIRST-PERSON (一人称) / NAME: NOT a CONFIRMED discrete setting. There is no documented `name` or `first_person` config key. INFERRED — first-person voice, the agent's name, and self-reference are governed entirely by free-text in **SOUL.md** (e.g., you write "Your name is X; refer to yourself as 私"). So it IS configurable, but only through the persona prose, not a structured field.
- WHAT'S CONFIGURABLE (summary): (1) SOUL.md persona prose = name + first-person + tone + custom instructions (free text); (2) `display.personality` named preset (e.g. kawaii); (3) reasoning effort.

### Persona — non-engineer touch verdict
- EXPOSE (this is the headline personalization a non-engineer wants): a simplified Soul/Persona editor — ideally a guided form ("Agent's name", "How it talks", "Things it should always/never do") that writes into SOUL.md behind the scenes. Raw SOUL.md editing = intimidating; wrap it.
- HIDE/DEFAULT: `display.personality` preset string and `reasoning_effort` — leave at defaults; advanced only.

---

## 4. TIMEZONE
- CONFIRMED — Config key `timezone`, default `""` (empty = server/system-local time). Accepts IANA strings (e.g. `America/New_York`, `Europe/London`, `Asia/Tokyo`, `UTC`). Affects log timestamps, cron scheduling, and **system-prompt time injection** (so the agent knows "now"). Source: https://hermes-agent.nousresearch.com/docs/user-guide/configuration and configuration.md
- CONFIRMED — Per-cron-job override exists: a schedule can carry its own `tz` independent of the global setting (`{"schedule":{"kind":"cron","expr":"0 9 * * *","tz":"Europe/Moscow"}}`). Source: https://github.com/NousResearch/hermes-agent/issues/26549
- User's claim "blank = system tz" = CONFIRMED correct.

### Timezone — non-engineer touch verdict
- AUTO-MANAGE: leave blank → uses system tz, which is right 99% of the time. Only expose if a user explicitly wants the agent to operate in a different zone (e.g., remote scheduling). Hide by default.

---

## 5. IMAGE SENDING / ATTACHMENTS
- CONFIRMED (desktop) — "Chat attachments — image and text-file attachments via click, drag-and-drop, and paste" (v0.4.5). Right-click copy/paste menu (v0.5.0). Agent-generated media renders inline in chat (v0.5.1 "Render agent-generated media in chat"). Source: https://github.com/fathah/hermes-desktop/releases
- CONFIRMED — Per-conversation **context folder**: pin a local folder to a conversation so its files are available as context (v0.5.0). This is the attachment-adjacent "give the agent files" feature. Source: same.
- CONFIRMED — Inbound image *analysis* is handled by the **vision toolset** (`auxiliary.vision`), which downloads image URLs with a configurable timeout. So "sending an image" → attachment UI → vision tool reads it. Source: https://hermes-agent.nousresearch.com/docs/user-guide/configuration
- NO dedicated "image attachments" config key exists; it's a UI capability + the vision toolset, not a setting to tune. Source: same (explicitly noted absent in config).

### Image/attachments — non-engineer touch verdict
- EXPOSE as pure UX (drag-drop/paste/click) — no settings needed. The only "setting" is whether the **vision toolset** is enabled (Tools screen). Keep vision on by default; surface it only in the toolset enable/disable list.

---

## CROSS-CUTTING NOTES FOR CURATION
- The user is looking at fathah/hermes-desktop GUI, which surfaces a *subset* of the full config.yaml. "3 memory providers" and "3 context engines" are **install/discovery-state counts in the GUI dropdowns**, not the real maximums (9 memory providers; 1 default engine + plugins). Don't hardcode "3."
- TWO accent-color namespaces exist: desktop GUI palette (**Midnight/Ember/Mono/Cyberpunk/Rose**, CONFIRMED) vs CLI skins (mono/slate/ares/poseidon/charizard/sisyphus/daylight/warm-lightmode/default/cyberpunk, CONFIRMED). User-mentioned "Amber/Slate" belong to the CLI/other-version namespace, not the current 5-theme desktop set (INFERRED).
- LANGUAGE CORRECTION (high priority): zh-Hant = Traditional Chinese; Japanese = `ja`. Both exist; they are different rows. The user's "zh-Hant = Japanese" mapping is factually wrong.
- Non-engineer EXPOSE set: Language, Theme, Accent, Persona (wrapped/guided), Tool-call verbosity (relabeled), image drag-drop. Non-engineer HIDE/AUTO set: all memory/profile budgets, compression threshold/target/protect_last_n, context engine, timezone (blank), personality preset string, reasoning effort.

### Sources
- https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/configuration.md
- https://hermes-agent.nousresearch.com/docs/user-guide/configuration
- https://hermes-agent.nousresearch.com/docs/developer-guide/context-compression-and-caching/
- https://hermes-agent.nousresearch.com/docs/user-guide/features/memory-providers
- https://hermes-agent.nousresearch.com/docs/user-guide/features/skins
- https://hermes-agent.nousresearch.com/docs/user-guide/features/kanban
- https://github.com/fathah/hermes-desktop
- https://github.com/fathah/hermes-desktop/releases
- https://github.com/NousResearch/hermes-agent/issues/26549
- https://github.com/NousResearch/hermes-agent/issues/18080