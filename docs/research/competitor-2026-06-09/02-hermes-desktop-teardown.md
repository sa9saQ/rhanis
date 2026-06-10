I have comprehensive coverage from all primary sources plus corroborating coverage. Let me compile the exhaustive teardown.

---

# HERMES AGENT DESKTOP — EXHAUSTIVE TEARDOWN (raw research data)

Metadata note: prompt states v0.15.2 / public preview ~2026-06-03. Live sources now show the agent at **v0.16.0 "The Surface Release" (2026-06-05)** with the desktop GUI as the headline; the desktop GUI itself was introduced alongside v0.15.2 (~2026-06-02/03). Treat v0.15.2 as the desktop's launch version and v0.16.0 as the current agent release that promotes it. (https://github.com/nousresearch/hermes-agent ; https://www.marktechpost.com/2026/06/03/nous-research-releases-hermes-desktop-a-native-cross-platform-front-end-for-hermes-agent-v0-15-2-with-streaming-tool-output/)

Source legend per claim: [C]=CONFIRMED in a primary source (URL cited); [I]=INFERRED.

---

## 0. PRODUCT IDENTITY & POSITIONING

- [C] Product = **Hermes Desktop / Hermes Agent Desktop App**, "a native application for macOS, Windows, and Linux" providing a GUI for the open-source Hermes Agent. (https://www.marktechpost.com/2026/06/03/...)
- [C] Tagline of the parent agent: **"The Agent That Grows With You"**; positioned as "Not a coding copilot tethered to an IDE or a chatbot wrapper around a single API" but "An autonomous agent that lives on your server, remembers what it learns, and gets more capable." (https://hermes-agent.nousresearch.com/)
- [C] License: **MIT** ("Open Source"), enabling "audit, self-hosting, and modification." (https://hermes-agent.nousresearch.com/ ; https://www.marktechpost.com/2026/06/03/...)
- [C] Creator: **Nous Research**. Bundle identifier `com.nousresearch.hermes` (seen in macOS mic-reset command). (https://hermes-agent.nousresearch.com/docs/user-guide/desktop)
- [C] Status: **"in public preview,"** users should "expect rough edges." (https://www.marktechpost.com/2026/06/03/...)
- [C] Stack (reported, not officially confirmed in docs): **Electron + React over a Python backend.** Dev workflow uses "Vite + Electron with Python backend." (https://www.digitalapplied.com/blog/hermes-desktop-v0-15-2-... ; https://hermes-agent.nousresearch.com/docs/user-guide/desktop)
- [C] Supported OS: **macOS, Windows, Linux**; Windows has both native and WSL2 install paths; macOS 12+, Windows 10/11 cited by third party. (https://hermes-agent.nousresearch.com/docs/user-guide/desktop ; https://www.digitalapplied.com/blog/...)
- [C] Install: macOS/Windows ship **direct installers**; Linux installs **via terminal with `--include-desktop` flag**. (https://www.marktechpost.com/2026/06/03/... ; https://www.digitalapplied.com/blog/...)

---

## 1. WINDOW & LAYOUT

### Overall geometry — chat-first, 3-zone
- [C] Chat-first layout with three primary zones: **(1) Left Sidebar Navigation** (management panes + profile switching), **(2) Center Chat Workspace** (primary interaction), **(3) Right Preview Rail** (concurrent file/webpage/tool-output rendering). (https://hermes-agent.nousresearch.com/docs/user-guide/desktop)

### Left sidebar — panes
- [C] Houses management panes and the profile switcher. Panes confirmed: **Skills browser, Cron panel, Profiles switcher, Messaging, Agents & Command Center, Sessions list.** (https://hermes-agent.nousresearch.com/docs/user-guide/desktop ; WebSearch corroboration)
- [C] Skills are also "viewable in the sidebar" (parity language with Codex sidebar). (Hermes desktop doc)

### Center workspace
- [C] **Streaming responses with live tool activity visualization.** ("The window shows streaming responses and live tool activity.") (https://hermes-agent.nousresearch.com/docs/user-guide/desktop ; https://www.marktechpost.com/2026/06/03/...)
- [C] **Structured tool-call summaries** rendered as agents execute. (Hermes desktop doc)
- [C] **Drag-and-drop file attachment anywhere in the chat area.** (Hermes desktop doc)
- [C] Session history is **shared across CLI / TUI / Web Dashboard / desktop** in the same workspace. (Hermes desktop doc)

### Right preview rail
- [C] Renders **web pages, files, and tool outputs** "side by side while you keep chatting" — "in real time as the agent works." (https://hermes-agent.nousresearch.com/docs/user-guide/desktop ; https://www.digitalapplied.com/blog/... ; https://www.marktechpost.com/2026/06/03/...)
- [I] This is the structural differentiator vs. a plain chat client: a persistent live artifact surface decoupled from the conversation stream (analogous to koe's ActivityLog + a render pane). Hermes pushes raw artifacts (pages/files); koe pushes "thinking-events / verifiable actions."

### File browser
- [C] **Dedicated file-browser panel** for exploring the working directory "without leaving the app," for "following along as the agent reads, writes, and edits files." (Hermes desktop doc)
- [C] Working dir set via `hermes desktop --cwd <path>` or `HERMES_DESKTOP_CWD` env var. (Hermes desktop doc)

### Status bar (bottom of chat)
- [C] **Inline model picker** to switch models mid-session "straight from the status bar." (Hermes desktop doc ; WebSearch corroboration)
- [C] **Per-session YOLO toggle** — "flip YOLO on or off for just that session (matching the TUI)"; YOLO = **dangerous-command approval bypass** ("bypasses the dangerous-command approval prompts"). (Hermes desktop doc ; WebSearch corroboration)
- [C] **Live session state indicators.** (Hermes desktop doc)
- [I] UX implication for koe: YOLO is the inverse of koe's fail-closed approval_gate; Hermes makes the safety bypass a one-click per-session toggle in the chrome, koe makes approval mandatory and per-operation. Hermes' model is "trust the user to flip risk off"; koe's is "system enforces tiers."

### Composer
- [C] **History navigation**: up/down arrows in an empty composer "recall and reuse previous prompts." (Hermes desktop doc ; WebSearch corroboration)
- [C] **Queue editing**: "edit messages you've queued up before they're sent." (WebSearch corroboration ; Hermes desktop doc)
- [C] **Drag-drop attach** (covered above). (Hermes desktop doc)

### Command palette / keyboard
- [C] **Command Palette** = **Cmd+K** (Ctrl+K on Windows/Linux) "to jump to actions and navigate from keyboard." (Hermes desktop doc)

---

## 2. MANAGEMENT PANES

### Skills browser
- [C] "Browse, install, manage skills." Skills system = "Procedural memory, Skills Hub, creating skills," compatible with the **agentskills.io standard**. Agent supports **autonomous skill creation after complex tasks**, and skills self-improve during use. (Hermes desktop doc ; https://hermes-agent.nousresearch.com/ ; https://github.com/nousresearch/hermes-agent)

### Cron / scheduled jobs
- [C] **Cron Panel** — "View and manage scheduled jobs in natural language." Backed by a "Built-in cron scheduler with delivery to any platform." Used for "reports, backups, and briefings." (Hermes desktop doc ; https://www.digitalapplied.com/blog/... ; https://github.com/nousresearch/hermes-agent)
- [C] Cron jobs persist across restarts (live under `~/.hermes/cron/jobs.json`, outputs `~/.hermes/cron/output/`). (WebSearch — Userorbit/docs corroboration)

### Profiles switcher
- [C] "Change between isolated Hermes profiles (config/skills/sessions)." A profile = "a separate Hermes home directory" with **its own config.yaml, .env, SOUL.md, memories, sessions, skills, cron jobs, and state database.** (Hermes desktop doc ; https://hermes-agent.nousresearch.com/docs/user-guide/profiles)
- [C] Profiles enable "separate agents for different purposes — a coding assistant, a personal bot, a research agent — without mixing up Hermes state." (WebSearch corroboration)
- [C] **Important non-guarantee:** profiles do **NOT** provide filesystem sandboxing — "On the default `local` terminal backend, the agent still has the same filesystem access as your user account." (https://hermes-agent.nousresearch.com/docs/user-guide/profiles)
- [C] Each profile runs "its own gateway as a separate process with its own bot token" → simultaneous multi-agent execution on one machine. (profiles doc)

### Messaging gateways
- [C] **Messaging pane** = "Set up gateway channels (Telegram, Discord, etc.)." (Hermes desktop doc)
- [C] Full gateway surface list from the agent: **Telegram, Discord, Slack, WhatsApp, Signal, Email** ("Lives Where You Do"). Includes **voice-memo transcription** and **cross-platform conversation continuity**. (https://hermes-agent.nousresearch.com/ ; https://github.com/nousresearch/hermes-agent)
- [I] The desktop "Messaging" pane is a GUI config front-end for these gateway processes (which normally require editing `.env` bot tokens) — i.e. it surfaces credential entry + channel enable/disable per profile.

### Agents + Command Center (multi-agent orchestration)
- [C] **Agents & Command Center** = "Multi-agent orchestration surfaces." Underlying capability "Delegates and Parallelizes — Spawn isolated subagents for parallel workstreams" / "Isolated subagents with their own conversations, terminals, and Python RPC scripts." (Hermes desktop doc ; https://hermes-agent.nousresearch.com/)
- [I] Command Center is the GUI for monitoring/steering spawned subagents (live status of parallel workstreams), distinct from "Agents" (configuration of agent definitions). Maps conceptually to Codex's "thread coordination + background subagents with stable identicons."

### Sessions
- [C] **Session List Overhaul** with "archiving and hygiene tools to manage growing session count." (Hermes desktop doc)
- [C] **Search by ID** to "find specific sessions directly." Underlying search = **FTS5 session search with LLM summarization.** (Hermes desktop doc ; https://www.marktechpost.com/2026/06/03/...)
- [C] **Concurrent Multi-Profile Sessions** — "Run sessions across multiple profiles simultaneously." (Hermes desktop doc)
- [C] **Cross-Profile @session References** — "Link to sessions in other profiles." (Hermes desktop doc)
- [C] Cross-surface continuity: "A conversation started in the desktop resumes in the CLI or TUI. The reverse also works, because state is not duplicated." (https://www.marktechpost.com/2026/06/03/...)

---

## 3. VOICE MODE

- [C] Desktop "Full voice input/output matching the voice mode available elsewhere." macOS: "OS will prompt once for microphone access." (Hermes desktop doc)
- [C] **macOS mic-reset escape hatch** documented: `tccutil reset Microphone com.nousresearch.hermes` to clear a stuck permission prompt. (Hermes desktop doc)
- Underlying voice engine (from voice-mode doc; desktop reuses the same runtime):
  - [C] **Input/STT**: push-to-talk **Ctrl+B** (CLI), 880Hz beep + live audio-level bar. Two-stage silence detection (RMS threshold 200, ≥0.3s speech confirm, 3.0s continuous-silence stop, 15s no-speech abort). **3-strike auto-exit.** (https://hermes-agent.nousresearch.com/docs/user-guide/features/voice-mode — fetched)
  - [C] STT providers: **faster-whisper local** (base/small/large-v3, free/recommended), **Groq** whisper-large-v3-turbo (~0.5s), **OpenAI** (whisper-1 / gpt-4o-transcribe, paid), **Mistral**, **xAI**. (voice-mode doc)
  - [C] **Hallucination filtering**: 26 known phantom phrases across languages + regex for repetitive variations. (voice-mode doc)
  - [C] **Output/TTS** — streaming, sentence-by-sentence as text generates (buffers deltas into ≥20-char sentences, strips markdown + `<think>` blocks). Providers: **Edge TTS** (free, ~1s, no key), **ElevenLabs** (paid, ~2s), **OpenAI TTS** (paid, ~1.5s), **NeuTTS** (free local). (voice-mode doc)
  - [C] Voice deps: `pip install "hermes-agent[voice]"`; system deps portaudio/ffmpeg/opus/espeak-ng. Config under `voice:` in `~/.hermes/config.yaml` (`record_key`, `silence_threshold`, `silence_duration`, `beep_enabled`). (voice-mode doc)
- [I] Desktop likely surfaces voice as a composer mic button rather than Ctrl+B keybind (the doc's Ctrl+B is CLI-specific); the "prompt once" macOS flow is the only GUI-specific permission detail confirmed. No Windows/Linux mic-permission flow documented for the GUI.

---

## 4. SETTINGS / CONFIGURATION

Top-level driver: [C] "Manage providers, models, tools, MCP servers, and credentials from a real interface instead of editing YAML." (WebSearch corroboration ; Hermes desktop doc)

### Providers
- [C] **Providers pane** = "dedicated accounts/API-keys UX for signing in and storing credentials per provider." (Hermes desktop doc)
- [C] **xAI Grok OAuth as a first-class provider** with **browser sign-in flow.** (Hermes desktop doc)
- [C] Full provider catalog matching CLI (`hermes model`). Agent-level provider list: **Nous Portal, OpenRouter (200+ models), NovitaAI, NVIDIA NIM, Xiaomi MiMo, z.ai/GLM, Kimi/Moonshot, MiniMax, Hugging Face, OpenAI** (and xAI per desktop doc). (Hermes desktop doc ; https://github.com/nousresearch/hermes-agent)

### Models
- [C] "Every provider and model surfaced in menus rather than a curated subset." (Hermes desktop doc)
- [C] **Auxiliary-model warning**: warns when switching the main model while **helper tasks (titling, summarization) remain pinned to another provider.** (Hermes desktop doc)
- [I] This aux-model warning is a notable UX honesty detail — it surfaces a hidden config split (main vs background model) that would otherwise silently cost on a second provider. Relevant to koe's aux/voice-provider split (OpenAI vs Google) — same class of "you changed one model but background tasks still hit the old one" footgun.

### Tools / Toolsets
- [C] **Install tool backends directly from the GUI** ("Tool-backend post-setup installation directly from GUI (no terminal drop required)"). Agent ships **"40+ tools, toolset system."** (Hermes desktop doc ; https://github.com/nousresearch/hermes-agent)

### MCP servers
- [C] **MCP (Model Context Protocol) server configuration** in settings; "support for external tools." (Hermes desktop doc ; https://www.marktechpost.com/2026/06/03/...)

### Gateway
- [C] **Gateway configuration tab**, including a **Remote gateway** sub-section (see remote backend below). (Hermes desktop doc)

### Session management
- [C] **Session-management settings tab** (archiving/hygiene controls noted under Sessions). (Hermes desktop doc)

### Credential pools / backup / import
- [C] **Credential pool / backup / import surfaces** in settings. (Hermes desktop doc)
- [I] "Credential pool" implies rotating/multiple keys per provider (load-balancing or rate-limit spreading) — a power-user feature absent from most consumer agent UIs.

### Log viewer
- [C] **Log viewer** tab. Boot logs at `HERMES_HOME/logs/desktop.log`, tail with `hermes logs gui -f`. (Hermes desktop doc)

### Network settings
- [C] **Network settings** tab (unspecified beyond name — likely proxy/host config). (Hermes desktop doc)

### Theme
- [C] **Theme selection** tab. (Hermes desktop doc) [I] light/dark/system — specifics not documented.

### UI language switch
- [C] **In-app UI language switcher including Simplified Chinese (zh-Hans).** (Hermes desktop doc)
- [I] Inclusion of zh-Hans as a named, shipped locale signals an explicit non-English target audience for the preview (notable for an English-first US lab).

### Remote backend / VPS support
- [C] **Remote backend over a VPS.** On remote machine set `HERMES_DASHBOARD_BASIC_AUTH_USERNAME/PASSWORD` + optional `HERMES_DASHBOARD_BASIC_AUTH_SECRET` in `~/.hermes/.env` (mode 0600), run `hermes dashboard --no-open --host 0.0.0.0 --port 9119`. In app: **Settings → Gateway → Remote gateway**, enter Remote URL, authenticate via **username/password form OR OAuth button ("Sign in with Nous Research")**. Credentials stored **per-profile.** (Hermes desktop doc)
- [I] Big architectural UX implication: the GUI is a thin React client that can point at *any* Hermes dashboard backend (local or a $5 VPS), so "desktop app" and "remote always-on agent" are the same product with a URL swap. koe is the opposite (single-machine native runtime); Hermes decouples frontend from runtime location.

### Rebindable shortcuts
- [C] **Rebindable keyboard shortcuts panel** — "Remap all keyboard shortcuts." (Hermes desktop doc)

### Custom zoom
- [C] **Custom zoom in half-step increments** for finer text-size control. (Hermes desktop doc)

### Updates
- [C] **Background update checks with one-click upgrade.** "App closes to finish uninstall cleanup (removes running bundle and venv after exit)." Manual update also works. (Hermes desktop doc)

### Uninstall — 3 levels
- [C] Via **Settings → About → Danger zone**:
  1. **"Uninstall Chat GUI only"** — removes app/data; agent, config, chats remain (`hermes uninstall --gui`)
  2. **"Uninstall GUI + agent, keep my data"** — removes app/agent; config/chats/secrets stay (`hermes uninstall`)
  3. **"Uninstall everything"** — complete removal incl. all user data (`hermes uninstall --full`) (Hermes desktop doc)
- [I] Three-tier uninstall mirroring koe's planned 3-level uninstall (`koe-` not yet built) — Hermes ships the data-preservation gradient (GUI-only / +agent / everything) that koe's plan only sketches.

---

## 5. ONBOARDING

- [C] First-run onboarding **redesigned on a "unified overlay design system."** (Hermes desktop doc ; WebSearch corroboration)
- [C] **"Choose provider later"** option — "skip provider setup and get into the app first" / "enter app immediately." (Hermes desktop doc ; WebSearch corroboration)
- [C] **CLI config auto-detection** — onboarding detects existing CLI config so existing CLI users don't re-enter setup. (Hermes desktop doc)
- [C] On first launch the **Hermes Agent runtime installs to `HERMES_HOME`** (`~/.hermes`, or `%LOCALAPPDATA%\hermes` on Windows) — "identical to CLI install layout." A `.hermes-bootstrap-complete` marker tracks setup; removing it forces clean re-setup. (Hermes desktop doc)
- [I] "Choose provider later" lowers the activation wall vs. typical agent apps that hard-gate on an API key first — you reach the chat surface before committing a provider (good first-run conversion design).

---

## 6. VISUAL DESIGN LANGUAGE

- [C] Described as **"a modern and thoughtfully designed UI."** (WebSearch — sourced to Nous docs/coverage)
- [C] Marketed as a **"native"** / **"polished graphical interface"** — third-party framing: "wraps a polished graphical interface around the same agent." (https://www.everydev.ai/tools/hermes-desktop-nous-research ; WebSearch)
- [I] **"Apple-style"** specifically: NOT found verbatim in any fetched primary source (the digitalapplied and marktechpost articles explicitly contain *no* aesthetic descriptors; both are functional). The "modern & thoughtfully designed" phrasing is the only confirmed design-language claim. Treat "Apple-style" as the prompt's framing, unconfirmed.
- [I] Concrete design signals that ARE confirmed and imply an Apple/macOS-native sensibility: Cmd+K command palette, half-step zoom, rebindable shortcuts, unified-overlay onboarding, system-style "Danger zone" in About, per-profile credential UX, OAuth sign-in buttons. These are HIG-adjacent conventions but typography/color/density/motion specifics are **not documented anywhere in the sources** — do not fabricate values.
- [I] Caution for downstream synthesis: any claim about specific fonts, color tokens, spacing scale, or motion curves would be invented. The honest design-language finding is: "self-described modern/thoughtfully-designed + native installers + macOS-idiom keyboarding; no published type/color/motion spec."

---

## 7. UNIFIED-RUNTIME / MANY-FRONTENDS ARCHITECTURE & UX IMPLICATIONS

- [C] Core architectural claim: the desktop app uses **"the same agent you get from the CLI and the gateway — same config, same API keys, same sessions, same skills, same memory. It is not a separate product or a lightweight clone."** A **React renderer communicates with the `hermes dashboard` backend over standard gateway APIs**, reusing the agent rather than reimplementing it. (Hermes desktop doc)
- [C] "Another surface over one agent, not a fork." Surfaces enumerated: **CLI, TUI, Web Dashboard, Desktop GUI, and messaging gateways (Telegram/Discord/Slack/WhatsApp/Signal/Email).** State is shared, "not duplicated." (https://www.marktechpost.com/2026/06/03/... ; Hermes desktop doc)
- [C] One runtime can run on **"a $5 VPS, a GPU cluster, or serverless infrastructure"** (Daytona/Modal "serverless persistence"). Six terminal/container backends: **local, Docker, SSH, Singularity, Modal, Daytona** (marktechpost lists five; GitHub README lists six incl. Daytona). (https://hermes-agent.nousresearch.com/ ; https://github.com/nousresearch/hermes-agent ; https://www.marktechpost.com/2026/06/03/...)
- UX implications [I]:
  1. **Zero-migration cross-device handoff** — start in desktop, continue in CLI/TUI/Telegram, because state isn't copied. This is the strongest UX differentiator vs. siloed desktop agents (incl. koe, which is single-surface).
  2. **The GUI is replaceable/optional chrome** — uninstalling the GUI leaves a fully working agent (uninstall level 1). The product survives its own frontend.
  3. **Remote-first by URL swap** — same React client points at local or VPS dashboard; "desktop app" is really "dashboard viewer." Implies the heavy lifting (tools, memory, subagents) is backend, the Electron shell is "minimal footprint."
  4. **Config honesty surfaces** (aux-model warning, credential pools, per-profile creds) exist *because* one runtime is shared by many surfaces — settings drift would otherwise be invisible.
  5. **Trade-off**: shared-runtime means the GUI inherits CLI-era complexity (40+ tools, 6 backends, profiles, gateways) — onboarding's "choose provider later" + unified overlay + CLI-config detection are the mitigations for that surface-area.

---

## APPENDIX A — CODEX APP (OpenAI) — COMPARISON BASELINE

(For the downstream synthesis to contrast against Hermes; all [C].)

- **What:** "a focused desktop experience for working on Codex threads in parallel, with built-in worktree support, automations, and Git functionality." OS: macOS (incl. Intel) + Windows; **Linux coming, not yet available.** (https://developers.openai.com/codex/app)
- **Layout:** project sidebar / active-thread workspace / review pane for code changes / integrated terminal (Cmd+J, scoped to project|worktree) / artifact previews (PDF, spreadsheets, docs, presentations in sidebar). (https://developers.openai.com/codex/app ; .../features)
- **Execution modes:** **Local / Worktree / Cloud.** "Use Worktree when you want to try a new idea without touching your current work." (https://developers.openai.com/codex/app/features)
- **Git:** diff view, inline comments on changes, stage/revert chunks, commit, push, create PRs in-app. (.../features)
- **Automations:** two types — **Standalone** (recurring tasks across projects) and **Thread automations** (recurring check-ins preserving conversation context). "wake up the same thread for ongoing checks." (.../features ; https://developers.openai.com/codex/app)
- **Computer use:** "see and operate graphical user interfaces on macOS or Windows" — view screen, screenshot, click/type/navigate, windows/menus/keyboard/clipboard. **Excludes EEA, UK, Switzerland at launch.** Requires **Computer Use plugin**; macOS needs **Screen Recording** (see) + **Accessibility** (interact) permissions. Windows = **foreground only** ("can't operate in the background"). Guardrails: cannot automate terminal apps or Codex itself, cannot authenticate as admin or approve security prompts; "File edits and shell commands still follow Codex approval and sandbox settings." "Always allow" per-app option. (https://developers.openai.com/codex/app/computer-use)
- **Appshots:** send "the frontmost Mac app window to Codex with a screenshot and available text." (https://developers.openai.com/codex/app)
- **Voice:** **voice dictation via Ctrl+M** (input only; no TTS output mentioned, unlike Hermes which has both). (https://developers.openai.com/codex/app/features)
- **Browser:** in-app browser for local dev servers + commenting + Codex-operated workflows; separate **Chrome extension** for signed-in tasks / Google Docs-Sheets-Slides context capture. (.../features ; changelog 26.527)
- **Image gen:** native **`gpt-image-2`** for UI assets/illustrations. (.../features)
- **Extensibility:** Skills, plugins, **MCP** (settings "sync across Codex tools automatically"); **Sites plugin** (preview launched 2026-06-02) to "create, save, deploy, and inspect websites, dashboards, internal tools, web apps, and games hosted by OpenAI." (.../features ; changelog 2026-06-02)
- **Remote:** steer/approve/review from **ChatGPT mobile app** on a connected host; remote control supports Windows (changelog 26.527, 2026-05-29). (https://developers.openai.com/codex/app ; changelog)
- **Other UX:** floating pop-out windows, IDE-extension sync w/ auto-context, notification controls, sleep-prevention toggle, default terminal location (bottom/right) setting (26.601, 2026-06-01), keypress-search in shortcut settings, stable identicons for background subagents. (.../features ; changelog 26.527/26.601)
- **Changelog anchors:** 26.602 (2026-06-04, activity insights/share cards, CU startup, onboarding role choices); Sites preview (2026-06-02); 26.601 (2026-06-01); 26.527 (2026-05-29, CU on Windows + remote control Windows + thread coordination + content/branch-name search). (https://developers.openai.com/codex/changelog)

### Hermes vs Codex — sharpest contrasts [I]
- **Scope:** Codex = coding/dev-centric (worktrees, git, PRs, IDE sync); Hermes = general autonomous PA (messaging gateways, persistent memory, voice I/O, 6 container backends). Hermes' right rail renders generic web/files; Codex's review pane renders diffs/code.
- **Voice:** Hermes = full duplex (STT in + streaming TTS out, 4 TTS providers); Codex = dictation-only (Ctrl+M).
- **Runtime model:** Hermes = one shared runtime across many surfaces (state not duplicated, GUI removable, VPS-swappable); Codex = desktop app + cloud + mobile-steering but tied to OpenAI account/infra (no MIT, no self-host).
- **License/openness:** Hermes MIT/self-hostable/BYOK-any-provider (incl. xAI Grok OAuth, OpenRouter 200+); Codex proprietary, OpenAI models, gpt-image-2.
- **Safety affordance:** Hermes = per-session **YOLO toggle** (bypass approvals); Codex = approval prompts + sandbox + Screen Recording/Accessibility OS perms, regional exclusion (EEA/UK/CH) for computer use. Opposite default postures.

---

## APPENDIX B — SOURCE-FETCH NOTES / GAPS

- **403 (blocked):** `https://openai.com/index/introducing-the-codex-app/` and `https://openai.com/index/codex-for-almost-everything/` returned HTTP 403 — NOT fetched. Codex data above is sourced from `developers.openai.com/codex/app`, `/features`, `/computer-use`, `/changelog` (all fetched OK).
- **429 (rate-limited):** `everydev.ai/tools/hermes-desktop-nous-research` returned HTTP 429 on direct fetch; its content reached us only via WebSearch summary (lower fidelity — "native"/"polished" framing).
- **Thin source:** `digitalapplied.com/blog/hermes-desktop-v0-15-2-...` is a strategic/architecture analysis with **no UI granularity and no aesthetic descriptors** (confirmed via two fetches). The richer digitalapplied confirmations (YOLO/model-picker/composer wording) came via WebSearch index, not the direct fetch.
- **Primary UI source of truth:** essentially ALL layout/pane/settings detail traces to ONE page — `hermes-agent.nousresearch.com/docs/user-guide/desktop`. Corroborated structurally by GitHub README (v0.16.0 "Surface Release"), marktechpost, and Userorbit/EveryDev via search. Single-source risk is highest for: exact settings-tab names, "unified overlay design system" phrasing, cross-profile @session, credential pools.
- **NOT confirmed anywhere (do not assert downstream):** specific typography, color palette/tokens, spacing/density scale, motion/animation curves, "Apple-style" verbatim, Windows/Linux voice mic-permission flow (only macOS "prompt once" is confirmed), exact theme options beyond "Theme selection."
- **Version reconciliation:** desktop GUI shipped with **v0.15.2** (~06-02/03 public preview); current agent = **v0.16.0 "The Surface Release" (06-05)** which centers the GUI. Both true; cite per-claim.