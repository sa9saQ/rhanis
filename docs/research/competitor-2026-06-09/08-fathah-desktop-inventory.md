I have a complete, ground-truth picture from the actual source. There is no Artifacts, Workspace, Safety, or "context engine" screen in the unofficial app — those are either official-Hermes concepts or misremembered by the user. The accent colors in the current source are syntax-themes (Dracula/Nord/etc.), not Mono/Midnight/Rose/Amber/Cyberpunk/Slate. I have enough to produce the exhaustive inventory.

---

# EXHAUSTIVE INVENTORY — UNOFFICIAL Hermes Desktop (fathah/hermes-desktop)

Method: read the **actual source** via GitHub API (`src/renderer/src/screens/`, `src/shared/i18n/locales/en/*`, `src/renderer/src/constants.ts`) — these are ground-truth, not README prose. Official Hermes Agent (NousResearch) used only to contrast. Every concrete claim cites a source. CONFIRMED = read in source/release. INFERRED = derived. MISMATCH = user's described item does NOT match this app's current source (likely older release or conflation with official Hermes).

---

## 0. CRITICAL FRAMING — two different products

The user described a settings tree of **Models / Chat / Appearance / Workspace / Safety / Memory & Context / Voice** with accents **Mono / Midnight / Rose / Amber / Cyberpunk / Slate**. That tree does **NOT** exist in fathah/hermes-desktop's current source. It partially matches **official Hermes Agent's web dashboard / CLI config** (Models/Chat/Memory&Context/Voice are official config categories; mono/slate are official *skins*). The user is conflating the two, OR looking at an older desktop release. The unofficial app's REAL structure is documented below.

- fathah/hermes-desktop is "Desktop Companion for Hermes Agent" — an Electron GUI that wraps the official `hermes-agent` CLI/gateway. CONFIRMED [https://github.com/fathah/hermes-desktop]
- Stack: Electron 39, React 19, TypeScript 5.9, Tailwind 4, Vite 7, better-sqlite3, i18next. CONFIRMED [https://github.com/fathah/hermes-desktop]
- Branding inside the app is "Hermes One" / "Hermes Agent". CONFIRMED (release v0.5.8 "Complete Hermes One branding"; settings.ts analytics string says "Helps improve Hermes One") [https://github.com/fathah/hermes-desktop/releases]

---

## 1. LEFT SIDEBAR / MAIN VIEWS — CONFIRMED (exact labels from navigation.ts + screens/ dir)

Source: `src/shared/i18n/locales/en/navigation.ts` and `src/renderer/src/screens/` directory listing. The sidebar labels (verbatim, in source order):

1. **Chat** (`chat`) — CONFIRMED
2. **Sessions** (`sessions`) — CONFIRMED
3. **Discover** (`discover`) — CONFIRMED (added v0.5.6 "Discover Tab")
4. **Profiles** (`agents` → label "Profiles") — CONFIRMED
5. **Office** (`office`) — CONFIRMED (3D "Claw3d"/Hermes Office; previews/office.png; "Inapp 3d office" v0.5.8)
6. **Models** (`models`) — CONFIRMED
7. **Providers** (`providers`) — CONFIRMED
8. **Skills** (`skills`) — CONFIRMED
9. **Persona** (`soul` → label "Persona") — CONFIRMED (SOUL.md editor)
10. **Memory** (`memory`) — CONFIRMED
11. **Capabilities** (`tools` → label "Capabilities") — CONFIRMED — *this is the "Skills & Tools"/toolset view the user described; internal key is `tools`, screen dir is `Tools/`, displayed as "Capabilities"*
12. **Schedules** (`schedules`) — CONFIRMED (cron)
13. **Kanban** (`kanban`) — CONFIRMED
14. **Gateway** (`gateway`) — CONFIRMED (messaging platform control)
15. **Settings** (`settings`) — CONFIRMED
16. Sidebar is collapsible: `collapseSidebar`/`expandSidebar` ("Collapse sidebar" added v0.5.6). CONFIRMED

Other screens in `screens/` that are flows, not sidebar items: Install, Setup, Welcome, SplashScreen, Layout. CONFIRMED [GitHub API tree]

**MISMATCH — items the user listed that do NOT exist as views in this app:**
- **"Artifacts"** — NO Artifacts screen exists in source (`grep artifact` = 0 hits). NOT REAL in this app. (Agent-generated media renders *inline in Chat* — "Render agent-generated media in chat" v0.5.0/v0.5.1 — but there is no standalone Artifacts view.) CONFIRMED-ABSENT
- **"Messaging (Discord/Telegram)"** as a named view — the messaging control surface is the **Gateway** screen + **Channels** concept; there is no separate "Messaging" sidebar item. The Gateway screen manages Telegram/Discord/etc. CONFIRMED (label is "Gateway")
- **"Projects"** — NO Projects screen. NOT REAL in this app. (Closest: per-conversation context folder, profiles.) CONFIRMED-ABSENT

---

## 2. SETTINGS TREE — CONFIRMED (exact, from settings.ts)

The Settings screen (`screens/Settings/Settings.tsx`, strings in `locales/en/settings.ts`) has these **top-level sections** (verbatim `sections`):
- **Hermes Agent** (`hermesAgent`)
- **Appearance** (`appearance`)
- **Privacy** (`privacy`)
- **Credential Pool** (`credentialPool`)

Plus inline sub-sections referenced by their own labels: **Connection** (Local / Remote / SSH Tunnel), **Network**, **Data**, **Logs**, **Community**, **Server Configuration**. CONFIRMED [settings.ts]

**MISMATCH:** The user's tree "Models / Chat / Appearance / Workspace / Safety / Memory & Context / Voice" is NOT the Settings tree. In this app, **Models / Memory / Capabilities(Tools) / Providers / Persona are TOP-LEVEL SIDEBAR SCREENS, not Settings sub-tabs.** "Chat / Workspace / Safety / Memory & Context / Voice" as Settings tabs do NOT exist here. Only **Appearance** matches.

### 2a. Settings → Appearance — CONFIRMED (settings.ts)
- **Theme** (`theme.label`): options **System** / **Light** / **Dark**. CONFIRMED
- **Rounded corners** (`roundedCorners`): toggle, "Turn off for squared-off corners throughout the app". CONFIRMED
- **Font** (`font`): **Manrope** / **System** ("Choose the interface font"). CONFIRMED
- **Language** (`language`): list below.
- Hint: "Choose your preferred interface appearance" (`appearanceHint`). CONFIRMED

### 2b. Settings → Privacy — CONFIRMED
- **Send anonymous usage analytics** toggle (PostHog, opt-out, `us.i.posthog.com`). Discloses: per-install UUID, platform/Electron/Node version, screen navigation; explicitly NOT collected: chat messages, file paths, API keys, model config, credentials. CONFIRMED [settings.ts; release v0.5.1 "surface analytics in Settings → Privacy"]

### 2c. Settings → Credential Pool — CONFIRMED
- Add/remove rotating API keys per provider for round-robin/load-balancing ("Hermes will cycle through them"). Key + optional Label. Keys redacted in list. CONFIRMED [settings.ts `poolHint`, `keyLabel`; release v0.5.2 "credential-pool schema", "crypto.randomBytes for credential-pool IDs"]

### 2d. Settings → Hermes Agent section (Connection/Network/Data/Logs) — CONFIRMED
- **Connection Mode**: **Local** / **Remote** / **SSH Tunnel** (tunnel to remote Hermes, default port 8642). CONFIRMED [settings.ts; SSH added ~v0.4.5]
- **Network**: **Force IPv4** toggle; **HTTP Proxy** (SOCKS/HTTP). CONFIRMED
- **Data**: **Export Backup** / **Import Backup** (config, sessions, skills, memory). CONFIRMED
- **Logs**: live log viewer, Refresh. CONFIRMED
- **Update Engine** / version display `v{{version}}` / **Run Diagnosis** / **Debug Dump**. CONFIRMED (this is the "Info/auto-update/version" the user described — it's the Hermes Agent section, not a Providers sub-tab)
- **OpenClaw migration** banner ("Migrate to Hermes"). CONFIRMED

---

## 3. APPEARANCE DETAIL

### 3a. Languages — CONFIRMED (settings.ts `language` + locales/ dirs)
Language picker options (verbatim native labels): **English**, **Bahasa Indonesia**, **日本語** (Japanese — **YES, INCLUDED**), **Español**, **中文** (Chinese), **Portuguese**, **Türkçe**. CONFIRMED [settings.ts]
- **Japanese IS included** — confirmed both by `japanese: "日本語"` string AND by a full `locales/ja/` translation directory. CONFIRMED [GitHub API tree; release v0.5.2 added "Japanese, Traditional Chinese (zh-TW), Simplified Chinese (zh-CN)"]
- Locale directories present in source: `en, es, id, ja, pl, tr` (and `pt-BR, pt-PT, zh-CN, zh-TW` exist as constants files → Chinese/Portuguese variants). CONFIRMED. Languages added over time: pt-PT/zh-TW (v0.4.5), Japanese/zh-TW/zh-CN (v0.5.2), Polish (v0.5.3), Spanish LATAM/Turkish (v0.5.6). CONFIRMED [releases]

### 3b. Theme / Accent colors — CONFIRMED list, MISMATCH vs user
The current source theme registry (`src/renderer/src/constants.ts`, `THEMES`) — these are the **actual** selectable themes (each is a full accent palette), verbatim id/name:
- **dark** "Dark", **light** "Light", **dracula** "Dracula", **nord** "Nord", **one-dark** "One Dark", **github-dark** "GitHub Dark", **monokai** "Monokai", **solarized-dark** "Solarized Dark", **gruvbox-dark** "Gruvbox Dark", **tokyo-night** "Tokyo Night", **github-light** "GitHub Light", **solarized-light** "Solarized Light". CONFIRMED [constants.ts]
- Theme selection: stored in localStorage, applied as `data-theme` attribute on `<html>`; "system" follows OS preference. CONFIRMED [ThemeProvider.tsx]
- "Multiple themes" + "Font Modifications" shipped in v0.5.6. CONFIRMED [releases]

**MISMATCH — user's accent names:** **Mono, Midnight, Rose, Amber, Cyberpunk, Slate do NOT appear in the current source theme list.** These match the **OFFICIAL Hermes CLI skins** (official skins include `mono`, `slate`; `cyberpunk` appears only as a custom-skin *example* in official docs; `default/ares/poseidon/sisyphus/charizard/daylight/warm-lightmode` are the official built-ins). Midnight/Rose/Amber are not built-ins in either. CONCLUSION: the user is either (a) on an older desktop release with a different theme set, or (b) recalling the official CLI/dashboard skin names. **Flag for downstream: these six accent names are UNVERIFIED for the current unofficial desktop app.** INFERRED-MISMATCH

### 3c. Tool-call display
- Live tool-call/reasoning rendering exists but as Chat-stream rendering, not an Appearance toggle: "Render structured live chat tool events" (v0.5.6), "Render richer live chat stream events" + "reasoning effort picker" (v0.5.8), "surface reasoning + tool rows live in the chat" (v0.5.1), "Tool calls, tool output & reasoning in history" (v0.5.0). CONFIRMED [releases]. A dedicated "tool-call display" Appearance setting is NOT in settings.ts. INFERRED-ABSENT (it's a Chat behavior, not a settings toggle)

---

## 4. CAPABILITIES / TOOLS view (the "Skills & Tools" + toolset list) — CONFIRMED

Screen `Tools/Tools.tsx`, label "Capabilities", subtitle "Enable or disable the toolsets your agent can use during conversations". **Full toolset list (verbatim label — description) from tools.ts:**
1. **Web Search** — "Search the web and extract content from URLs"
2. **X Search** — "Search posts and content on X (Twitter)"
3. **Browser** — "Navigate, click, type, and interact with web pages"
4. **Terminal** — "Execute shell commands and scripts"
5. **File Operations** — "Read, write, search, and manage files"
6. **Code Execution** — "Execute Python and shell code directly"
7. **Computer Use** — "Control the desktop—move the mouse, click, and type"
8. **Vision** — "Analyze images and visual content"
9. **Image Generation** — "Generate images with DALL-E and other models"
10. **Video Generation** — "Generate videos from text or image prompts"
11. **Text-to-Speech** — "Convert text to spoken audio"
12. **Skills** — "Create, manage, and execute reusable skills"
13. **Memory** — "Store and recall persistent knowledge"
14. **Session Search** — "Search across past conversations"
15. **Clarifying Questions** — "Ask the user for clarification when needed"
16. **Delegation** — "Spawn sub-agents for parallel tasks"
17. **Cron Jobs** — "Create and manage scheduled tasks"
18. **Mixture of Agents** — "Coordinate multiple AI models together"
19. **Task Planning** — "Create and manage to-do lists for complex tasks"

All CONFIRMED [tools.ts]. (19 toolsets in unofficial desktop vs 14 listed in the README prose — the source is authoritative.)

**MCP** lives inside this Capabilities/Tools screen — CONFIRMED [tools.ts `mcpServers` block]:
- "MCP Servers" — Add server / Browse catalog (Hermes MCP catalog) / filter / Test connection / Enable/Disable/Remove. Transport: **HTTP** or **stdio**. Fields: Name, Transport, URL, Authentication (None / Header), Command, Arguments, Environment. "MCP server management UI" added v0.5.6. CONFIRMED

---

## 5. SKILLS view — CONFIRMED
Screen `Skills/Skills.tsx`. Browse/toggle installed skills, install/uninstall, browse the skill **hub** (agentskills.io standard). "Skill Uninstall Fixes" (v0.5.9), "Harden skill content reads" (v0.5.3). CONFIRMED [releases; Skills.tsx]. This + Capabilities together form what the user called "Skills & Tools view."

---

## 6. PROVIDERS view — CONFIRMED
Screen `Providers/Providers.tsx`, "Configure LLM providers, API keys, and credential pools." Has **Subscription / OAuth Plans** section ("Sign in with a provider subscription instead of an API key. Authorization happens in your browser."). CONFIRMED [providers.ts].

**Provider list (verbatim, from constants.ts) — CONFIRMED:**
- API-key providers: OpenRouter, AIML API, Anthropic, OpenAI, OpenAI Codex, Ollama Cloud, Google, xAI, **Xiaomi MiMo**, Mistral, DeepSeek, Groq, Together AI, Fireworks AI, Cerebras, Perplexity, Hugging Face, NVIDIA NIM, Z.ai / GLM, Qwen, MiniMax, Nous (Nous Portal), **Atlas Cloud**.
- Local/OpenAI-compatible: LM Studio, **Atomic Chat**, Ollama, vLLM, llama.cpp, plus generic "custom OpenAI-compatible."
- OAuth/subscription sign-in: **ChatGPT (Codex Plan)**, **xAI Grok (OAuth)**, **Qwen (OAuth)**, **Gemini (CLI OAuth)**, **MiniMax (OAuth)**, **Kimi (Coding Plan)**, **Nous Portal (OAuth)**. CONFIRMED [constants.ts; releases v0.5.0 "In-app OAuth sign-in (ChatGPT Codex, xAI Grok, Qwen, Gemini CLI, MiniMax)"]
- Auto-detect provider from API key. CONFIRMED

**Per-model API keys: CONFIRMED REAL.** In the **Models** screen each model has its own "API Key" field ("Picks the matching env key based on the URL, or CUSTOM_API_KEY otherwise"). CONFIRMED [models.ts]. So the user's "PER-MODEL api keys" observation is CORRECT.

**"Tools & keys" / "Accounts":** Tool-provider keys (Exa, Tavily, Firecrawl, FAL, Browserbase, etc.) are managed via the Credential Pool / .env API-keys grouping; "Accounts" maps to the OAuth/Subscription Plans section. CONFIRMED-EQUIVALENT [README tool integrations; providers.ts oauth]
**"Archived chats":** NOT a Providers sub-item. Sessions screen handles delete/prune/rename/export; no "archived chats" surface in this app's source. CONFIRMED-ABSENT (Kanban has an "Archived" column, unrelated)

---

## 7. MEMORY view (the "Memory & Context" the user described) — CONFIRMED
Screen `Memory/`, label "Memory", subtitle "What Hermes remembers about you and your environment across sessions." Tabs (`MemoryTab`): **entries** / **profile** / **providers** / **soul**. CONFIRMED [Memory/types.ts, memory.ts]

- **Agent Memory** (`agentMemory`) + **User Profile** (`userProfile`) — both editable, with **char count + char limit** displayed (CapacityBar/CapacityCards components). This is the "memory budget / profile budget" — implemented as **character limits**, NOT token sliders. CONFIRMED [types.ts `charCount/charLimit`; CapacityBar.tsx, CapacityCards.tsx]. (Official Hermes defaults: memory ~2200 chars, profile ~1375 chars — INFERRED these are the underlying limits.)
- Stats: total sessions, total messages. CONFIRMED
- **Memory Providers (8) — CONFIRMED verbatim [memory.ts `providers`]:** **Honcho** (cross-session user modeling, dialectic Q&A), **Hindsight** (knowledge graph, multi-strategy retrieval), **Mem0** (server-side LLM fact extraction, auto-dedup), **RetainDB** (cloud, hybrid search, 7 memory types), **Supermemory** (profile recall, entity extraction), **Holographic** (local SQLite FTS5, trust scoring, no API key), **OpenViking** (tiered retrieval), **ByteRover** (knowledge tree via brv CLI). Each: Activate/Deactivate, env-var entry. Built-in memory always active alongside selected provider. CONFIRMED. (User asked "how many providers?" → **8** in this app.)

**MISMATCH — items the user listed under "Memory & Context" that are NOT in this app's UI:**
- **"memory providers (how many?)"** → 8 (above). CONFIRMED.
- **"context engine (how many?)"** → **NO "context engine" selector exists in this app's UI** (`grep context.engine` in screens = absent). "Context engine" is an OFFICIAL Hermes config concept (`context.engine` plugin), not surfaced in the unofficial desktop Memory screen. CONFIRMED-ABSENT in this app.
- **"auto-compression (trigger 0.5 / target / protect-last-20-messages)"** → **NOT a UI control in this app.** These are OFFICIAL Hermes `compression:` config defaults (threshold 0.50, target_ratio 0.20, protect_last_n 20) that the wrapped CLI uses, but the desktop app exposes no compression settings panel. CONFIRMED-ABSENT as desktop UI. The user likely saw these in official docs/config, not the desktop GUI.

---

## 8. VOICE / TTS — CONFIRMED (partial)
- **Voice input** in Chat: "Voice input" / "Stop recording" / "Transcribing…" — speech-to-text in chat (added v0.5.5 "Speech to Text in Chat"). CONFIRMED [chat.ts; useVoiceInput.ts]
- **Text-to-Speech** is a **toolset** (#11 above), "Convert text to spoken audio." CONFIRMED [tools.ts]. Underlying TTS = OpenAI (via Nous Portal Tool Gateway "text-to-speech (OpenAI)"). CONFIRMED [official README Nous Portal].
- **MISMATCH:** There is **NO dedicated "Voice" settings tab with a TTS-model picker** in this app's source. "Voice" as a top-level Settings section does not exist. TTS-model selection is not a desktop GUI control. CONFIRMED-ABSENT (the "Voice → TTS models (OpenAI etc.)" tree the user described is NOT in this app)

---

## 9. PERSONA / PERSONALITY & IMAGE SENDING — CONFIRMED
- **Persona** screen (`Soul/`, label "Persona"): edit agent personality/tone/instructions via **SOUL.md**, loaded fresh every conversation; Reset to default. CONFIRMED [soul.ts]. (Persona merged into Memory in v0.5.3 "Merge persona to memory"; Memory has a `soul` tab.) CONFIRMED
- A discrete "一人称/name" field is NOT a structured input — name/persona are free-text in SOUL.md / User Profile (profile placeholder example: "Name: Alex..."). CONFIRMED [soul.ts, memory.ts]. So "Personality (一人称/name)" = the free-text Persona/Profile, not a dedicated field. INFERRED
- **Image sending: CONFIRMED REAL.** Chat supports image attachments via click, drag-and-drop, paste; images auto-compressed to fit ("compress oversize images" v0.5.2; "Chat attachments — image and text-file attachments" v0.4.5; "Restore prompt images when reopening sessions" v0.5.6). Error string: "couldn't compress image to fit (animated GIFs…)." CONFIRMED [chat.ts; releases]

---

## 10. KANBAN — CONFIRMED
Screen `Kanban/Kanban.tsx`. In the unofficial desktop it began as a **read-only board** mirroring Claw3D/Hermes HQ ("Claw3D HQ read-only board" v0.5.0; "Kanban in SSH tunnel mode" v0.4.5; i18n added en+pt-PT v0.5.1; previews/kanban.png). CONFIRMED [releases]. Underlying official Kanban model (the engine it reflects): columns **Triage / Todo / Ready / Running / Blocked / Done** (+ Archived), tasks persisted in `~/.hermes/kanban.db`, multi-board, dispatcher auto-decompose. CONFIRMED [official kanban docs] — but the desktop view is primarily a board *viewer*, not the full CLI orchestration surface. INFERRED for desktop editability scope.

---

## 11. SCHEDULES / GATEWAY / OFFICE / DISCOVER / SESSIONS — CONFIRMED
- **Schedules** (`Schedules/`): cron job builder with delivery targets. CONFIRMED
- **Gateway** (`Gateway/`): start/stop + manage messaging platforms (Telegram, Discord, Slack, WhatsApp, Signal, Matrix, Mattermost, Email IMAP/SMTP, SMS Twilio/Vonage, iMessage/BlueBubbles, DingTalk, Feishu/Lark, WeCom, WeChat, Webhooks, Home Assistant — 16+ per README). "Show Gateway start failures in the UI" (v0.5.6), "Improve messaging platform management" (v0.5.6). CONFIRMED [releases; README]
- **Office** (`Office/office3d/`): in-app 3D "Claw3d"/Hermes Office visual interface. CONFIRMED [releases v0.5.8 "Inapp 3d office"; previews/office.png]
- **Discover** (`Discover/`): discovery tab (v0.5.6). CONFIRMED
- **Sessions** (`Sessions/`): browse/search/rename/delete/bulk-delete/export sessions, SQLite FTS5, per-row + bulk delete (v0.5.5 "bulk session deletion"; v0.5.2 per-row delete). CONFIRMED

---

## 12. AUTO-UPDATE / VERSION / RELEASE-PAGE FEATURES — CONFIRMED
- Auto-updater present; "Update Engine" button + `v{{version}}` display in Settings → Hermes Agent. CONFIRMED [settings.ts; README "Auto-updater"]
- Current release line: **v0.5.9 (June 9 2026)** is latest; recent: v0.5.8, v0.5.6, v0.5.5, v0.5.3, v0.5.2, v0.5.1, v0.5.0, v0.4.5. CONFIRMED [releases]
- Notable release-only features: reasoning-effort picker (v0.5.8), API Key Set Option (v0.5.8), Ollama Cloud provider (v0.5.8), network proxy persistence + collapsible sidebar + worktree terminal launcher (v0.5.6), IME/CJK Enter-key fix for Japanese/Chinese input (v0.5.6 "keep IME composition Enter from sending truncated CJK text"), Worktree view (v0.5.2), portable Windows build (v0.5.0), in-app OAuth (v0.5.0–v0.5.1). CONFIRMED [releases]
- Storage: `~/.hermes/` (.env, config.yaml, hermes-agent, profiles/, state.db, cron/jobs.json, kanban.db). CONFIRMED [README; official kanban docs]

---

## 13. VERDICT TABLE — which user-listed items are REAL in this app

| User's item | Status in fathah/hermes-desktop |
|---|---|
| Chat, Skills, Sessions, Kanban, Memory, Models, Providers, Schedules, Gateway | CONFIRMED REAL (sidebar) |
| "Skills & Tools" view | REAL — split as **Skills** + **Capabilities**(Tools) screens |
| Toolset list (big) | REAL — **19 toolsets** [tools.ts] |
| Messaging (Discord/Telegram) | REAL but the view is **Gateway**, not "Messaging" |
| **Artifacts** view | NOT REAL — no Artifacts screen (media renders inline in Chat) |
| **Projects** view | NOT REAL — no Projects screen |
| Settings tree Models/Chat/Appearance/Workspace/Safety/Memory&Context/Voice | MOSTLY NOT REAL — Settings sections are **Hermes Agent / Appearance / Privacy / Credential Pool**; Models/Memory/Tools/Providers are sidebar screens; **Chat/Workspace/Safety/Voice settings tabs do not exist** |
| Appearance → Theme Light/Dark/System | CONFIRMED REAL |
| Appearance → accents Mono/Midnight/Rose/Amber/Cyberpunk/Slate | NOT MATCHED — source themes are Dark/Light/Dracula/Nord/One Dark/GitHub Dark/Monokai/Solarized Dark/Gruvbox Dark/Tokyo Night/GitHub Light/Solarized Light (user's names = official CLI skins or older build) |
| Appearance → languages incl. Japanese | CONFIRMED REAL — 日本語 included (full ja/ locale) |
| Per-model API keys | CONFIRMED REAL (Models screen, per-model API Key field) |
| Providers → Accounts / Gateway / Tools&keys / MCP | PARTIAL — OAuth "Accounts" REAL; Gateway is own screen; tool keys via Credential Pool; **MCP lives in Capabilities/Tools screen**, not Providers |
| Providers → Archived chats / Info(version/auto-update) | Archived chats NOT REAL; version/auto-update REAL but under **Settings → Hermes Agent** |
| Models → auxiliary/vision/web-extraction/compression/skill hub/auth/MCP/timezone | NOT REAL as Models sub-items (those are official CLI config keys; this app's Models screen = model library manager) |
| Memory & Context → persistent memory, user profile | CONFIRMED REAL (Agent Memory + User Profile tabs) |
| Memory budget / profile budget | REAL as **character-count limits** (not token sliders) |
| Memory providers count | **8** providers CONFIRMED |
| **Context engine** count | NOT REAL — no context-engine selector in this app (official-only concept) |
| **Auto-compression** trigger 0.5/target/protect-last-20 | NOT REAL as desktop UI — those are official Hermes `compression:` config defaults, not exposed in this GUI |
| Voice → TTS model picker (OpenAI) | NOT REAL as a settings tab — TTS is a **toolset**; voice **input** STT is in Chat |
| Personality (一人称/name) | REAL as free-text **Persona/SOUL.md** + User Profile (no structured 一人称 field) |
| Image sending | CONFIRMED REAL (attach/drag/paste, auto-compress) |

---

## 14. KEY DATA-QUALITY FLAGS for downstream curation
1. The user's **settings tree and accent-color names match OFFICIAL Hermes Agent (CLI skins + web-dashboard/config), not the current unofficial desktop source.** Treat Mono/Midnight/Rose/Amber/Cyberpunk/Slate, "context engine," "auto-compression trigger 0.5," and "Voice TTS picker" as belonging to official Hermes config — UNVERIFIED in fathah/hermes-desktop ≥ v0.5.x main.
2. Authoritative source for the unofficial app = its `src/` (read here), not README/release prose, which undercounts (README says 14 toolsets; source has 19) and uses different names ("Capabilities" not "Tools," "Persona" not "Soul").
3. **Artifacts** and **Projects** views: do not exist in this app — if the user saw them, they may be on a future/forked build or confusing another product (e.g., Claude/ChatGPT Artifacts).

Sources: https://github.com/fathah/hermes-desktop · https://github.com/fathah/hermes-desktop/releases · (source files via GitHub API: `src/shared/i18n/locales/en/{navigation,settings,tools,models,memory,providers,soul,chat}.ts`, `src/renderer/src/constants.ts`, `src/renderer/src/components/ThemeProvider.tsx`, `src/renderer/src/screens/Memory/types.ts`) · https://hermes-agent.nousresearch.com/docs/user-guide/desktop · https://hermes-agent.nousresearch.com/docs/user-guide/features/web-dashboard · https://hermes-agent.nousresearch.com/docs/user-guide/features/kanban · https://hermes-agent.nousresearch.com/docs/user-guide/features/skins · https://hermes-agent.nousresearch.com/docs/user-guide/features/memory-providers · https://github.com/NousResearch/hermes-agent