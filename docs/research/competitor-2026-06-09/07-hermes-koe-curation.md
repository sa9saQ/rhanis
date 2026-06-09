This is a curation/synthesis task — no code, no repo writes. I have three verified inventories plus a fact-check that I should trust on conflicts. Let me produce the clean, decisive curation the user can act on.

A few load-bearing facts from the fact-check I'll honor over the inventories: Spotify/Home Assistant/Video Analysis are NOT in the desktop GUI (they're official-agent toolsets) but ARE real and worth stealing as consumer features; the desktop accent themes are 12 syntax themes (NOT Mono/Midnight/Rose/Amber/Cyberpunk/Slate — that's INV3's error); Japanese UI IS real; zh-hant is Traditional Chinese (separate from ja); memory providers = 8 desktop / 9 official (NOT 3); compression numbers are real but live in config.yaml, not a GUI panel.

---

# Hermes → koe curation (for a non-engineer voice secretary)

**One-line frame**: Hermes is a power-user agent with ~71 tools, 76 skills, 8-9 memory providers, and 4 settings sections of raw config. A normal person can't drive that by voice. The job is to **keep the consumer GOLD, voice-enable it, and bury or auto-manage the rest.** koe's glass-box thesis means the few things we DO show, we show *clearly* (what it's doing + source), and everything technical disappears.

---

## A) TOOLS CURATION

Lens: *would a normal person SAY this out loud to a voice secretary?* "ADOPT" = consumer gold, voice-first. "ADAPT" = useful but needs reshaping/guardrails. "SKIP" = dev-only, cut or bury in an "Advanced/Developer" drawer.

### Consumer GOLD — ADOPT (build these, voice-first)

| Hermes tool | koe verdict | Why (voice-use one-liner) |
|---|---|---|
| **Web Search** (`web_search`) | **ADOPT** | "What's the weather / who won / look that up" — the #1 secretary verb. Already in koe (M1). |
| **Web Extract** (`web_extract`) | **ADOPT (silent)** | "Read me that article" — runs *under* web search, never a separate user concept. |
| **Image Generation** (`image_generate`) | **ADOPT** | "Make me a picture of…" — delightful, obvious, zero learning curve. |
| **Text-to-Speech** (`tts`) | **ADOPT (core, not a tool)** | koe IS voice — TTS is the product's mouth, not an optional toolset. Always on. |
| **Vision** (`vision_analyze`) | **ADOPT** | "Look at this screenshot / what's in this photo" — pairs with image-send. |
| **Spotify** (`spotify_*`, 7 tools) | **ADOPT ★ flagship** | "Play some jazz" — the single most consumer-obvious voice action. *Not in Hermes desktop; steal from official agent.* OAuth login, no key typing. |
| **Home Assistant** (`ha_*`, 4 tools) | **ADOPT ★ flagship** | "Turn off the living room lights" — smart-home by voice is killer-app territory. *Not in Hermes desktop; steal from official.* |
| **Weather** | **ADOPT (build it)** | Hermes has NO weather tool (web-search-only). A real secretary needs "what's the weather today" as a first-class, instant answer. **Gap = koe opportunity.** |
| **Calendar** | **ADOPT (build it)** | Hermes only has it via the `google-workspace` *skill* (a CLI wrapper). For koe, "what's on my calendar / add a meeting" must be first-class, not a buried skill. |
| **Email** | **ADOPT (build it)** | Hermes hides it in the `himalaya` skill + IMAP gateway. For koe, "read my new email / reply to X" should be a guided OAuth connect, not an IMAP form. |
| **Cron / Schedules** (`cronjob`) | **ADAPT** | "Every morning at 8 give me a briefing" is gold — but relabel as **Routines/Reminders**, never "cron". |
| **Session Search** (`session_search`) | **ADOPT (silent)** | "What did we decide last week?" — koe's glass-box already records; expose as natural recall, no "FTS5" wording. |
| **Memory** (`memory`) | **ADOPT (silent)** | "Remember that I'm allergic to peanuts" — but it's an internal capability, not a user-facing toolset toggle. |
| **Clarify** (`clarify`) | **ADOPT (core UX)** | The agent asking "did you mean A or B?" is *exactly* koe's calibrated-transparency thesis. Make this a first-class voice/visual pattern. |
| **Video Analysis** (`video_analyze`) | **ADAPT** | "What happens in this clip" is consumer-ok, but lower priority than image. *Not in desktop; official only.* |
| **Video Generation** (`video_generate`) | **ADAPT** | Fun but expensive/slow — keep behind a "creative" surface, not a default. |

### ADAPT — useful but reshape (guardrails / rename / bury the mechanism)

| Hermes tool | koe verdict | Why |
|---|---|---|
| **File Operations** (`read/write/patch/search`) | **ADAPT (heavy guardrails)** | "Save this note / open that file" is consumer-real, but raw file write = danger. koe already gates this: SAFE read (allowlist) / CAUTION write (Documents+Desktop only) / DANGER delete (approval). Keep the *capability*, hide the *mechanics*. |
| **Computer Use** (`computer_use`) | **ADAPT (DANGER-gated, later)** | "Click that for me" is the dream, but it's macOS-only in Hermes and high-risk. koe already scopes it DANGER. Powerful but not M1. |
| **X Search** (`x_search`) | **ADAPT** | "What's trending / search X for…" is consumer-ish but niche + needs a key. Optional connect, not a default. |
| **Messaging gateways** (16: Telegram/Discord/WhatsApp/Signal/SMS/iMessage…) | **ADAPT (pick 2-3)** | A secretary *delivering* to your phone is great. But 16 is overwhelming — offer **SMS + one chat app**, not the full menu. Home Assistant-as-gateway: skip (confusing duplicate of the tool). |
| **Skills hub** (agentskills.io, 76 skills) | **ADAPT (curate hard)** | The open-skill idea is good, but 76 dev-skewed skills would bury a non-engineer. koe should ship a **hand-picked ~10 consumer skills** (Spotify, Hue/smart-home, Maps, Obsidian/notes, Calendar, Email, PDF/OCR, PowerPoint, Weather) and hide the rest. |

### SKIP — dev-only (cut, or hide in a "Developer" drawer the user never opens)

| Hermes tool / skill | koe verdict | Why |
|---|---|---|
| **Claude Code CLI** / **Codex CLI** / **OpenCode CLI** delegation skills | **SKIP** | "Delegate coding to a CLI" — pure developer workflow. A non-engineer will never say this. |
| **GitHub** (6 skills) | **SKIP** | Repos/PRs/issues — dev-only. |
| **Terminal** (`terminal`, `process`) | **SKIP (or DANGER-locked)** | Raw shell = the most dangerous + least consumer-relevant. koe already DENY_LISTs shell; default OFF. |
| **Code Execution** (`execute_code`) | **SKIP** | Running Python/shell — dev-only. |
| **Browser automation** (`browser_*`, 10 tools, CDP/Browserbase) | **SKIP** | Dev-flavored site automation. (Consumer "look something up" = web_search, not this.) |
| **MCP server config** (Add server / stdio / Header auth / env vars) | **SKIP from main UI** | Raw MCP plumbing — bury entirely behind Advanced/Developer. |
| **Delegation** (`delegate_task`) / **Mixture of Agents** (`moa`) | **SKIP (internal only)** | Multi-agent orchestration is an *implementation detail*, never a user toggle. |
| **Kanban** (9 tools, orchestrator) | **SKIP** | Task-board orchestration = engineer command-center. Not a secretary concept. |
| **Discord Server Admin** (`discord_admin`) | **SKIP** | Server moderation = admin/dev. |
| **Feishu Doc/Drive**, **Yuanbao** (Tencent), **DingTalk/WeCom/Lark** | **SKIP (region/enterprise)** | Enterprise/CN-specific. Out of scope for a global consumer MVP. |
| **`debugging` / `rl` / `safe` / `dogfood` / `godmode` / mlops (vLLM, abliterate, HF hub)** skills | **SKIP** | Research/dev/red-team — actively wrong for a consumer. |
| **Claude Design / design-md / architecture-diagram / p5js / touchdesigner** | **SKIP** | Creative-dev artifact tools. ("Claude Design" is just an HTML-artifact skill — *not an Anthropic product*; would confuse users.) |

**Bottom line for tools**: koe's consumer surface = **~12 voice verbs** (search, weather, music, lights, image, calendar, email, notes/files, reminders, recall, vision, clarify). Everything Hermes calls a "toolset" that's actually dev plumbing (terminal, code-exec, browser, MCP, delegation, kanban, GitHub, CLI-delegation) gets cut or locked.

---

## B) SETTINGS CURATION

Rule of thumb: **a non-engineer should never be asked to choose a value whose wrong choice silently degrades quality or costs money.** Those are AUTO-MANAGE. Cosmetic + personal = KEEP-SIMPLE. Real-but-technical = ADVANCED. Irrelevant = SKIP.

### KEEP-SIMPLE — show these to everyone (cosmetic / personal / zero-risk)

| Setting | koe action | Note |
|---|---|---|
| **Theme** (Light / Dark / System) | **KEEP-SIMPLE** | Universal, expected. Default = System (your OS-follow redesign already does this). |
| **Accent color** | **KEEP-SIMPLE** | A *small* curated set (3-5 named, on-brand), NOT 12 syntax themes. (Hermes desktop actually ships 12 dev syntax themes — Dracula/Nord/Monokai etc. — too many, too coder-y. Don't copy that.) |
| **Language (UI)** | **KEEP-SIMPLE** | koe is global/multi-language. Include 日本語. **Label clearly: "this changes the app's buttons/menus, not what the assistant says back to you."** |
| **Persona: name + how it talks** | **KEEP-SIMPLE (guided form)** | The headline personalization. A small form ("Assistant's name", "How it should talk", "Always/never do") — NOT a raw SOUL.md text box. |
| **Image send** (drag/drop/paste) | **KEEP-SIMPLE (just works)** | No setting — pure UX. The only toggle is "can it see images" = Vision on by default. |
| **Spending cap + balance** | **KEEP-SIMPLE ★ koe-specific** | "Balance ¥◯◯ (~◯ min) / monthly cap" — your prepaid model. This is koe's most important non-Hermes setting; keep it dead simple with the minutes hint. |

### AUTO-MANAGE — do it for them, NO UI (wrong values silently hurt)

| Hermes setting | koe action | Plain-words why |
|---|---|---|
| **Context engine** (`compressor`/`lcm`/…) | **AUTO-MANAGE** | "How to handle a long conversation." Default works; alternatives are experimental. Never show. |
| **Compression threshold (0.50)** | **AUTO-MANAGE** | "When to start summarizing old chat." A number nobody can reason about. Hardcode the good default. |
| **target_ratio (0.20)** | **AUTO-MANAGE** | "How much recent chat to keep verbatim." Same — invisible. |
| **protect_last_n (20) / protect_first_n (3)** | **AUTO-MANAGE** | "Always keep the last 20 + first 3 messages." Pure internal tuning. |
| **Auxiliary / compression / vision / web-extraction model** | **AUTO-MANAGE** | "Which mini-model does background summarizing/extraction." koe picks it. A non-engineer choosing a model = guaranteed wrong. |
| **Timezone** (blank = system) | **AUTO-MANAGE** | Read it from the OS. The agent needs to know "now" for reminders, but the user shouldn't type `Asia/Tokyo`. (Expose only inside a Routine if they schedule for another zone.) |
| **Memory char budgets (2200 / 1375)** | **AUTO-MANAGE** | "How much it remembers about you." Internal caps; surfacing "2200 chars" means nothing to a human. |
| **Tool-call verbosity** (`off`/`new`/`all`/`verbose`) | **AUTO-MANAGE → glass-box default** | This is koe's THESIS. Don't make it a setting — koe's whole point is the *right* level of "what it's doing + source" is always shown. Maybe one toggle: **"Show details: Off / On"**, default On. |
| **Credential pool / key rotation / round-robin** | **AUTO-MANAGE** | Load-balancing multiple keys = ops concern. With koe's managed-credit model, irrelevant. |

### ADVANCED — real, but bury behind an "Advanced / Power user" drawer

| Hermes setting | koe action | Why advanced not deleted |
|---|---|---|
| **BYOK: per-model API key** | **ADVANCED** | Your retired-but-kept BYOK path (`RealtimeAuth::Byok`). Power users want it; mass users use managed credit. Bury it. |
| **Memory provider** (8-9: Honcho/Mem0/…) | **ADVANCED (or SKIP)** | Mass user gets koe's built-in memory. *If* exposed, offer **None / one cloud option**, never a 9-way dropdown. |
| **Reasoning effort** (none…xhigh) | **ADVANCED** | koe picks a sane default; let tinkerers override. |
| **Connection mode / SSH tunnel / proxy / IPv4** | **ADVANCED** | Network ops — only self-hosting power users. |
| **Export/Import backup** | **ADVANCED (but easy)** | Worth having for data portability; not front-and-center. |
| **Voice provider** (OpenAI vs Google) | **KEEP-SIMPLE but framed as quality, not model** | Show as "Standard / High quality", not "gpt-realtime-2 vs Gemini Live". The mechanism is advanced; the *choice* is simple. |

### SKIP — irrelevant to koe

| Hermes thing | Why skip |
|---|---|
| **Usage analytics opt-out (PostHog), OpenClaw migration banner, "Office" 3D Claw3d, Kanban board, Discover tab, Gateway 16-platform manager** | Hermes-product-specific furniture. koe doesn't need a 3D office or a 16-gateway control panel. |
| **MCP server management UI, worktree terminal launcher, diagnostics/debug-dump** | Dev plumbing. |

---

## C) STEAL THESE (specific Hermes ideas koe should adopt)

1. **Per-provider OAuth login instead of pasting keys** *(Hermes v0.5.0)*. Hermes lets you "Sign in with ChatGPT/xAI/Google in your browser" rather than typing a key. **For koe: Spotify, Calendar (Google), Email (Gmail), smart-home — all should be a "Connect" button → browser OAuth → done.** Non-engineers can't safely handle raw API keys; OAuth is the unlock. (Keep raw per-model keys only in the BYOK/Advanced drawer.)

2. **Guided Persona instead of raw SOUL.md.** Hermes exposes a free-text SOUL.md (intimidating). koe should ship a **3-field form**: *Assistant's name* / *How it talks (friendly, brief, formal…)* / *Always-never rules* — and write the prose behind the scenes. This directly serves "一人称/name" personalization (Hermes has no structured first-person field; koe can do better).

3. **Timezone-as-silent-context.** Hermes injects "now + tz" into the system prompt so the agent knows the current time. **koe: read OS timezone automatically, never ask.** Critical for "remind me at 8am" and your Routines feature. Only surface tz inside a Routine if scheduling for elsewhere.

4. **"Choose your provider later / managed by default."** Hermes auto-detects provider from the key and has a managed Nous Portal gateway. **koe's managed-credit model is the consumer version of this** — ship with声=Standard/High-quality preselected, BYOK retired to Advanced. Don't make provider choice a launch gate.

5. **Image send by drag/drop/paste + auto-compress** *(Hermes v0.4.5/v0.5.2)*. Cheap, delightful, expected. Pair with Vision-on-by-default so "look at this" just works.

6. **Per-conversation context folder** *(Hermes v0.5.0)*. "Pin this folder to our chat" — a clean consumer metaphor for giving the assistant files without a file-picker every time. Good fit for koe's note/work flows.

7. **Clarify-as-a-first-class-pattern.** Hermes' `clarify` tool (agent asks before guessing) is *literally koe's calibrated-transparency thesis*. Make the "I'm not sure — did you mean A or B?" voice+visual moment a signature koe interaction, not a hidden tool.

8. **Routines (cron) in plain language.** Hermes' natural-language cron ("every morning send a briefing") is consumer gold *if renamed*. koe: **"Routines"** — "Every weekday at 8, tell me my calendar + weather." Never the word "cron".

9. **Accent themes — but restrained.** Take the *idea* (named color palettes), reject the *execution* (12 dev syntax themes). koe ships ~4 on-brand named accents that pair with your OS-follow + immersive-orb redesign.

10. **One curated skill shelf, not 76.** Hermes' open-skill catalog is powerful but dev-skewed. koe: a **"Capabilities" shelf of ~10 hand-picked consumer skills** (music, smart-home, maps, notes, calendar, email, PDF, weather) you can toggle, with everything else hidden.

---

## D) koe's SIMPLE DEFAULT — what a non-engineer ever sees

The entire visible settings surface for a normal koe user fits on **one short screen**, organized to match your immersive-orb + clear-console redesign:

**Top level (always visible):**
- **Balance & cap** — "¥◯◯ remaining (~◯ min) · Monthly cap ¥◯◯" + auto-recharge toggle. *(koe-specific, most important.)*
- **Voice** — Standard / High quality (quality words, not model names). On/off + push-to-talk.
- **Assistant** — Name + "How it talks" + Always/Never rules (the guided persona form).
- **Appearance** — Theme (System/Light/Dark) · Accent (4 choices) · Language (incl. 日本語, with the "UI only" note).
- **Connections** — a list of **"Connect" buttons**: Spotify, Calendar, Email, Smart home, (each = browser OAuth, no keys). This is the consumer "tools" surface.
- **Show details** — On/Off (the glass-box level; default On). One toggle, not four verbosity modes.

**Behind "Advanced" (one click, most users never open):**
- BYOK / per-model API keys · Voice provider raw selection · Memory provider · Reasoning effort · Connection mode / proxy · Export/Import backup.

**Never shown (auto-managed, no UI):**
- Context engine, compression threshold/target/protect-N, auxiliary/vision/web-extraction models, memory char budgets, timezone (from OS), credential pooling.

**The principle**: Hermes-grade power lives underneath (8 memory providers, compression tuning, 71 tools, per-model keys all *exist* in the engine) — but the non-engineer's whole world is **6 simple groups + a Connect list + one details toggle.** Power is opt-in, never a barrier.

---

## E) CORRECTIONS (gently, for the user)

1. **"zh-Hant = Japanese" — no.** `zh-hant` is **Traditional Chinese (繁體中文)**. Japanese is a separate code, **`ja`**. They're two different rows in Hermes' language list (the full list has both `ja` and `zh-hant` independently). Easy to mix up, but they're distinct.

2. **Yes, Hermes really does have a Japanese UI.** Not a half-measure — the desktop has a full `ja/` locale folder, Japanese landed in v0.5.2, and v0.5.6 even shipped a CJK/IME fix so the Enter key doesn't cut off Japanese input mid-composition. So 日本語 support is genuine. **Caveat for koe too:** "language" only changes the *interface chrome* (buttons/menus), not the language the assistant *replies in* — worth labeling explicitly so users aren't surprised.

3. **The accent colors you remembered (Mono / Midnight / Rose / Amber / Cyberpunk / Slate) aren't the desktop's themes.** The actual Hermes *desktop* themes are 12 **code-editor syntax palettes** (Dark, Light, Dracula, Nord, One Dark, GitHub Dark, Monokai, Solarized, Gruvbox, Tokyo Night…). "Mono" and "Slate" are real but they're **CLI terminal skins** (a different part of Hermes), and "Midnight/Rose/Amber" aren't in either list. *(One inventory claimed Midnight/Ember/Mono/Cyberpunk/Rose were confirmed desktop accents — that's incorrect per the source.)* Takeaway for koe: don't copy the 12 syntax themes; ship a few on-brand named accents.

4. **"Context engine" — plain words.** It's just **the strategy for handling a long conversation** so it fits in the AI's memory window. The default ("compressor") quietly summarizes the older middle of the chat. The alternatives are experimental. **There is no "3 context engines" dropdown** in the desktop app — that count was a guess; the desktop doesn't even expose this. For koe: auto-manage, never show.

5. **"Compression / trigger 0.5 / protect last 20" — plain words.** When the conversation fills to about **half** the AI's memory, Hermes auto-**summarizes the old middle** of the chat to make room, while keeping the **newest ~20 messages word-for-word** (and the first 3). The numbers (0.50 / 0.20 / 20 / 3) are real *defaults* — but they live in a **config file, not a settings screen** (the desktop has no compression panel). For koe: pick good defaults, never surface the knobs.

6. **"3 memory providers" — actually 8 (or 9).** The Hermes *desktop* lists **8** external memory providers (Honcho, Mem0, Hindsight, Supermemory, Holographic, OpenViking, RetainDB, ByteRover); the official agent has **9** (adds Memori). The "3" was likely what one install happened to show. **For koe: mass users get one built-in memory — don't expose a provider dropdown at all** (Advanced at most).

7. **Spotify / Home Assistant / Video Analysis aren't in the Hermes *desktop* — but they're real in the official agent.** So when you looked at the desktop's tool list and didn't see them, that's correct *for the desktop GUI*. They DO exist as official Hermes toolsets (`spotify_*`, `ha_*`, `video_analyze`), which is why I'm calling them consumer gold to **build into koe** — they're proven, not vaporware.

8. **"Per-model API keys" — you remembered right.** Hermes' Models screen genuinely gives each model its own API-key field (confirmed in source, v0.5.2 auto-populates them). For koe, keep this — but only in the **Advanced/BYOK** drawer; the default user never sees a key field thanks to managed credit + OAuth connect.