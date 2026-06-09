I now have exhaustive primary + secondary source coverage. Synthesizing the taxonomy.

---

# COMPREHENSIVE SETTINGS & FEATURE TAXONOMY — Mature Desktop AI Agent Apps (2026)
## Gap-analysis checklist FOR koe (consumer voice secretary, glass-box thesis)

Legend per item: **[App: which apps confirmed]** · **CONFIRMED** (seen in primary source) / **INFERRED** · **koe-relevance: HIGH / MED / LOW / DEV-ONLY (skip)** with 1-line reason tied to koe's voice + transparency thesis.

Source key: Codex=OpenAI Codex App; Hermes=Nous Hermes Desktop; CD=Claude Desktop; ChatGPT=ChatGPT desktop; Raycast=Raycast AI; Cursor=Cursor.

---

## 1. PROVIDERS / MODELS / AUTH

- **Sign-in with account OAuth OR API key (dual auth)** — Codex: "sign in with your ChatGPT account or an OpenAI API key". CONFIRMED [Codex]. koe-relevance: HIGH — koe M1=BYOK, M4=managed-credit; the "account login OR own key" dual path is exactly koe's M1→M4 arc. https://developers.openai.com/codex/app
- **API-key-only caveat ("some functionality might not be available")** — Codex explicitly degrades features on API-key auth. CONFIRMED [Codex]. koe-relevance: MED — koe should surface which features need managed-credit vs BYOK (e.g. self-hosted voice). https://developers.openai.com/codex/app
- **Dedicated Providers settings pane with API-key storage** — Hermes: "Providers settings pane … inference providers with API-key storage". CONFIRMED [Hermes]. koe-relevance: HIGH — koe already has multi-provider encrypted key UI (koe-31u); validates the pattern.
- **First-class OAuth provider with browser sign-in (xAI Grok)** — Hermes: "xAI Grok OAuth … browser sign-in flow". CONFIRMED [Hermes]. koe-relevance: MED — relevant when koe adds providers (Gemini Live planned); OAuth-per-provider vs raw key.
- **Credential pool / multiple backend auth modes** — Hermes: username/password (trusted nets), OAuth (Nous Portal), self-hosted OIDC. CONFIRMED [Hermes]. koe-relevance: LOW — koe is single-user desktop, no team backend; OIDC is enterprise-only.
- **Inline model picker in status bar (per-session switching)** — Hermes; Codex model selection. CONFIRMED [Hermes]. koe-relevance: MED — koe's analog = voice-provider picker (OpenAI/Google) labeled by experience tier ("standard/high-quality"), already built. Placement matters: koe should keep it minimal near the orb, not a dev status bar.
- **Auxiliary-model warning when switching providers mid-session** — Hermes: "Auxiliary-model warning". CONFIRMED [Hermes]. koe-relevance: MED — koe should warn if changing voice provider mid-conversation breaks calibration baseline / cost model.
- **Broad provider catalog** — Hermes: Nous Portal, OpenRouter (200+), Novita, NVIDIA NIM, GLM, Kimi, MiniMax, HF, OpenAI; Codex: Amazon Bedrock provider (26.601). CONFIRMED [Hermes, Codex]. koe-relevance: LOW — koe deliberately curates 2-3 voice providers, not a 200-model catalog (would dilute consumer simplicity).
- **Local-model integration (Ollama, 100+ models, no cloud/keys)** — Raycast since v1.99. CONFIRMED [Raycast]. koe-relevance: HIGH-future — koe's planned self-hosted Qwen3.5-Omni is the voice analog; local = privacy + transparency (hidden-state SEP calibration signal only possible self-hosted).
- **BYOK for multiple providers (Anthropic/Google/OpenAI)** — Raycast BYOK. CONFIRMED [Raycast]. koe-relevance: HIGH — already koe's M1 model.
- **Cloud model-provider integration (Bedrock)** — Codex 26.601. CONFIRMED [Codex]. koe-relevance: LOW — enterprise infra, not consumer voice.
- **Secrets stored in dedicated secrets manager (not plaintext)** — Hermes: Bitwarden Secrets Manager integration replaces plaintext keys; koe uses stronghold. CONFIRMED [Hermes]. koe-relevance: HIGH — koe already encrypts via stronghold; competitor confirms "no plaintext keys" is table-stakes.

## 2. PERMISSIONS & SAFETY

- **Multi-mode sandbox: read-only (default) / auto / full-access** — Codex: read-only project scope default; "Full access mode … enables live web search"; Local/Worktree/Cloud execution envs. CONFIRMED [Codex]. koe-relevance: HIGH — direct parallel to koe's 3-tier gate (SAFE/CAUTION/DANGER); "full access" warning UX pattern transferable.
- **Native, open-source, configurable system-level sandboxing** — Codex CLI+App. CONFIRMED [Codex]. koe-relevance: MED — koe sandboxes file/URL via validation.rs + permission policy; OS-level sandbox is heavier than koe needs now.
- **Approval scope granularity: "approve once" vs "approve for this session"** — Codex. CONFIRMED [Codex]. koe-relevance: HIGH — koe's DANGER modal is currently one-shot; adding "allow for this session" reduces fatigue but must stay fail-closed. Maps to koe-p1a risk-tier redesign.
- **Per-app "Always allow" list (computer use) with removable entries** — Codex: "remove apps from the Always allow list in the Computer Use section". CONFIRMED [Codex]. koe-relevance: MED — koe has folder/URL allow+deny (koe-351); per-tool "always allow" is the same shape.
- **Automatic review / routes approvals through review policy** — Codex "Auto-review" / "configured review policy". CONFIRMED [Codex]. koe-relevance: LOW — review-policy routing is dev-team workflow; koe's human-in-the-loop is the user themselves.
- **Per-session YOLO toggle (bypass dangerous-command approval)** — Hermes: "Per-session YOLO toggle". CONFIRMED [Hermes]. koe-relevance: MED-CAUTION — koe is fail-closed by thesis; a YOLO toggle contradicts "calibrated transparency / human judges when to intervene". If offered, must be heavily gated + off by default + never for DANGER tier. Likely SKIP for brand integrity.
- **Named permission profiles / custom profiles** — Codex `/permissions` shows "named permission profiles" + "custom profiles" (0.135.0); environment-scoped approvals (0.137.0). CONFIRMED [Codex]. koe-relevance: LOW — profiles are multi-project dev ergonomics; koe is single ambient session.
- **Per-tool 3-stance permission (Allow / Ask / Blocked)** — CD: each MCP tool Allow/Ask/Blocked. CONFIRMED [CD]. koe-relevance: HIGH — cleaner model than koe's binary; koe's SAFE/CAUTION/DANGER ≈ Allow/Ask/Block but per-tool override UI is worth adopting.
- **Sensitive-action extra confirmation (account/security/privacy/payment/network/credentials)** — Codex computer-use: "ask for permission before sensitive or disruptive actions"; "Stay present for account, security, privacy, network, payment, or credential-related settings". CONFIRMED [Codex]. koe-relevance: HIGH — matches koe DANGER tier (external_upload, run_command); the explicit "stay present for credentials" guidance is good voice-UX copy.
- **Hard-blocked action classes (can't authenticate as admin, can't approve OS security prompts, can't automate terminal/itself)** — Codex computer-use. CONFIRMED [Codex]. koe-relevance: HIGH — koe should permanently DENY: self-modification, admin elevation, approving its own approval modals. Reinforces koe's DENY_LIST.
- **OS-level permission prompts (Screen Recording, Accessibility on macOS; mic)** — Codex (screen recording/accessibility), Hermes (mic permission + `tccutil reset Microphone`). CONFIRMED [Codex, Hermes]. koe-relevance: HIGH — koe needs mic permission UX (koe-8kw / koe-8t2 mic permission UX already tracked); the `tccutil reset` recovery path is a concrete pattern to copy for Mac (M3).
- **Lockdown Mode (limit web/external access to reduce prompt-injection exfiltration)** — ChatGPT: "Lockdown Mode … limits web and external service access … reduce data exfiltration risk from prompt injection". CONFIRMED [ChatGPT]. koe-relevance: HIGH — a one-switch "paranoid mode" fits koe's fail-closed thesis; relevant to koe-gap (open_url/web_search exfil).
- **Promptware / injection defenses: tool-result delimiters, memory threat-pattern scanning, control-plane file-write protection, mTLS for MCP** — Hermes. CONFIRMED [Hermes]. koe-relevance: HIGH — koe already delimits user input (security.md rule); memory-scanning + control-plane protection are forward-looking for when koe adds MCP/memory.
- **Command allowlist + denylist** — Hermes (command allowlist, OpenClaw migration); koe DENY_LIST/ALLOW_LIST. CONFIRMED [Hermes]. koe-relevance: HIGH — koe already does deny-then-allow shell gating; confirms ordering (deny first).
- **Container hardening / namespace isolation (read-only rootfs, dropped Linux caps)** — Hermes 5-6 backends. CONFIRMED [Hermes]. koe-relevance: LOW — koe runs tools as local async tasks, not containers; over-engineering for a voice secretary.

## 3. TOOLS / TOOLSETS / MCP / PLUGINS / SKILLS

- **MCP support with settings UI + auto-sync across surfaces** — Codex ("MCP and Connectors", "Model Context Protocol settings auto-sync"); Hermes (MCP config in settings); CD (Settings→Connectors, Allow/Ask/Block per tool); Raycast (local stdio + HTTP SSE/Streamable, @-mention to invoke). CONFIRMED [Codex, Hermes, CD, Raycast]. koe-relevance: HIGH — koe's MCP client is planned (koe-eal/koe-dcj); the @-mention invocation + per-tool permission UI are direct patterns. Voice twist: koe must voice-announce which MCP tool it's invoking (transparency thesis).
- **Plugin browser / curated directory in GUI with one-click install + uninstall** — Codex: dedicated Plugins tab under "New Thread", `/plugins`, "Uninstall plugin"; 90+ plugins bundling skills+apps+MCP. CONFIRMED [Codex]. koe-relevance: MED — a GUI tool marketplace fits koe's M2+ "手足 tool" expansion, but must not clutter the orb; likely a separate settings screen, not orb-adjacent.
- **Bundled plugins = skills + app integrations + MCP servers as one unit** — Codex plugin model. CONFIRMED [Codex]. koe-relevance: MED — useful packaging concept if koe ever ships pre-built tool bundles (e.g. "Calendar pack").
- **App integrations / connectors (GitHub, Slack, Linear, Google Drive, Notion, M365)** — Codex; CD M365; Raycast (GitHub/Notion/Drive/Brave). CONFIRMED [Codex, CD, Raycast]. koe-relevance: MED — koe's recorder adapters (Obsidian/Notion planned) overlap; consumer connectors (Calendar, Email read) more relevant than dev connectors (GitHub/Linear).
- **Skills (reusable instructions/workflows), portable SKILL.md, agentskills.io open standard, self-improving** — Codex ("Reusable instructions and workflows across app/CLI/IDE", OpenAI Docs skill bundled, extra skill roots); Hermes (SKILL.md, 19,932-entry catalog, GEPA self-improve every 15 tool calls, autonomous skill creation). CONFIRMED [Codex, Hermes]. koe-relevance: MED — koe could expose user "routines" but the dev-skill ecosystem is heavy; koe's analog is curated voice workflows, not a 19k catalog.
- **Tools/toolsets management pane (enable/disable, browse, install)** — Hermes (`hermes tools`, tool-backend post-setup install from GUI); Codex (`tools enabled config`). CONFIRMED [Hermes, Codex]. koe-relevance: HIGH — koe needs a "which手足 tools are active" toggle screen; ties to permission policy.
- **Built-in first-party tools: web search (cached vs live), image generation, browser automation, vision, TTS, multi-model reasoning** — Codex ("first-party web search", `$imagegen`, in-app browser); Hermes (web search, browser, vision, imagegen, TTS). CONFIRMED [Codex, Hermes]. koe-relevance: HIGH — koe has web_search/file_ops/computer_use/recorder; cached-vs-live web search distinction is a good cost/safety toggle (cached=cheaper+safer default).
- **In-app browser / browser comments / operate local browser flows** — Codex. CONFIRMED [Codex]. koe-relevance: LOW-MED — a full in-app browser dilutes the orb; koe's transparency need is "show which URL it referenced", not embed a browser.
- **Computer use (GUI control: click/type/screenshot, per-app approval, foreground vs background)** — Codex (macOS background / Windows foreground only, EEA/UK/Switzerland excluded). CONFIRMED [Codex]. koe-relevance: HIGH — koe has computer_use tool + DANGER gating; the EEA/UK/CH geo-restriction is a compliance flag koe must consider; "Windows foreground-only" matches koe's Windows-first reality.
- **Python RPC scripts collapse multi-step pipelines / delegation spawns isolated subagents** — Hermes. CONFIRMED [Hermes]. koe-relevance: LOW — multi-agent delegation is a dev power-feature; koe is one ambient voice, not a swarm.

## 4. SCHEDULING / AUTOMATIONS / CRON / ALWAYS-ON RESIDENCY

- **Standalone scheduled automations (instructions + optional skill, schedule fields, results to review queue)** — Codex: "Schedule recurring tasks"; "results land in a review queue". CONFIRMED [Codex]. koe-relevance: MED — koe's idle curator (koe-sua.6, M4) is the analog; a review queue fits koe's transparency (you review what it did while away).
- **Thread automations / "wake up the same thread for ongoing checks" preserving context** — Codex. CONFIRMED [Codex]. koe-relevance: MED — "wake the same conversation" = ambient secretary checking in; aligns with koe always-on thesis.
- **Cloud-based triggers (run in background even when computer is closed)** — Codex (building out). CONFIRMED [Codex]. koe-relevance: LOW-M4 — requires server backend; koe M1-M3 is local-only desktop.
- **Built-in cron scheduler with natural-language schedules + delivery to any platform** — Hermes ("hermes cron", NL "reports, backups, briefings", deliver to messaging platform). CONFIRMED [Hermes]. koe-relevance: MED — NL scheduling by voice ("every morning summarize my notes") is a natural koe feature; "deliver to platform" less so (koe delivers by voice).
- **Cron management pane (view/manage scheduled jobs)** — Hermes "Cron pane". CONFIRMED [Hermes]. koe-relevance: MED — a "scheduled tasks" screen if koe adds recurring jobs.
- **Always-on background residency / tray daemon** — Codex ("run continuously in the background"); koe planned tray/always-on (koe-944). CONFIRMED [Codex] / koe planned. koe-relevance: HIGH — koe's defining trait is "always-on"; tray residency (koe-944) is a core M1-product-layer gap, not optional.
- **"Prevent sleep while running" toggle** — Codex. CONFIRMED [Codex]. koe-relevance: HIGH — an always-on voice secretary must offer keep-awake; cheap, high-value.

## 5. SESSIONS / PROFILES / HISTORY / SEARCH / MEMORY

- **Parallel threads side-by-side + quick switch + Git worktree isolation** — Codex. CONFIRMED [Codex]. koe-relevance: DEV-ONLY (skip) — multi-thread parallelism is a coding-agent concept; koe is one continuous conversation. Copying this would dilute koe.
- **Concurrent multi-profile sessions + cross-profile @session links** — Hermes. CONFIRMED [Hermes]. koe-relevance: DEV-ONLY (skip) — profiles/cross-links are power-user dev features.
- **Session archive/unarchive (protect from resume/fork)** — Codex `/archive` `/unarchive` (0.136.0); Hermes session archiving. CONFIRMED [Codex, Hermes]. koe-relevance: MED — koe's conversation log could support archive; fits history UI (koe-sh6).
- **Session search (by id; full-text incl. conversation content + branches)** — Codex (search incl. conversation + Git branches, 26.527); Hermes (FTS5 search by id). CONFIRMED [Codex, Hermes]. koe-relevance: HIGH — koe records conversations (SQLite); searchable history is a clear consumer feature (history UI koe-sh6).
- **Composer history recall (↑/↓ in empty composer) + queue editing before send** — Hermes. CONFIRMED [Hermes]. koe-relevance: LOW — koe is voice-first; text composer history is secondary.
- **Memory: persistent, agent-curated, periodic save-nudges, view/edit/delete, cross-session recall via FTS+LLM summarization, user modeling** — Codex (Chronicle, "preview of memory", screen-context recovery, enable in settings); ChatGPT (Settings→Personalization→Memory toggle, view/edit/delete, auto-update, Manage Memory); Hermes (4-layer stack, agent-curated, Honcho dialectic user modeling). CONFIRMED [Codex, ChatGPT, Hermes]. koe-relevance: HIGH — koe's calibration memory L4 (koe-sua.3) + bi-temporal memory (planned, Zep/Letta koe-9ds) are exactly this; **the user-facing view/edit/delete + on/off toggle is mandatory for consumer trust and data-deletion (koe-0k1)**.
- **Memory threat-pattern scanning (security on memory)** — Hermes. CONFIRMED [Hermes]. koe-relevance: MED — when koe stores memory, scan for injection persistence.
- **Cross-session / cross-surface conversation continuity (resume on any device/surface, state not duplicated)** — Codex (IDE sync, Auto Context); Hermes ("sessions shared across surfaces, state not duplicated"); ChatGPT (sync web/desktop/mobile). CONFIRMED [all]. koe-relevance: LOW-M4 — koe is single-device desktop M1-M3; multi-surface sync is M4+ if mobile ever ships.
- **Project context files (AGENTS.md / SOUL.md / USER.md / context that shapes every conversation)** — Hermes (SOUL.md persona, USER.md, context files); Codex Rules. CONFIRMED [Hermes, Codex]. koe-relevance: MED — koe could let users write a "who I am / how to address me" profile that shapes voice persona; lightweight win.
- **Personality presets (Default/Cynic/Robot/Listener) + custom instructions** — ChatGPT (4 personalities); Hermes (`/personality`). CONFIRMED [ChatGPT, Hermes]. koe-relevance: MED — voice persona/tone presets fit koe well (a secretary's demeanor); BUT must not undermine calibrated-confidence honesty (a "confident" persona could miscalibrate the transparency signal). Tie to calibration carefully.

## 6. MESSAGING GATEWAYS / REMOTE / MULTI-DEVICE / SYNC

- **Messaging gateways (Telegram, Discord, Slack, WhatsApp, Signal, Email, Home Assistant)** — Hermes (8 surfaces + separate gateway process); Codex (Slack integration). CONFIRMED [Hermes, Codex]. koe-relevance: LOW — koe's surface IS voice + the orb; bolting on chat gateways contradicts the "talk like a person" thesis. Possible far-future "text me a summary" but not core.
- **Remote backend connection (connect GUI to remote host)** — Hermes (`HERMES_DESKTOP_REMOTE_URL`, per-profile remote host); Codex (remote control pairing, ChatGPT mobile steers connected host). CONFIRMED [Hermes, Codex]. koe-relevance: LOW-M4 — only if koe adds a managed backend (M4 managed-credit) or self-hosted voice server; not M1.
- **Remote control from mobile (start/steer/approve/review on connected host)** — Codex; mobile Face ID/passcode lock. CONFIRMED [Codex]. koe-relevance: MED-future — "approve a DANGER action from your phone while away" is a compelling always-on secretary feature (the orb runs at home, you approve remotely), but heavy infra.
- **Cross-device account sync (conversations/memory/instructions/plan)** — ChatGPT. CONFIRMED [ChatGPT]. koe-relevance: LOW-M4.
- **DM pairing / device pairing auth** — Hermes (DM pairing); Codex (remote pairing start/status). CONFIRMED [Hermes, Codex]. koe-relevance: LOW.

## 7. OBSERVABILITY / LOGS / USAGE & BILLING / TELEMETRY

- **Streaming responses with live tool activity + structured summaries** — Hermes ("Streaming responses with live tool activity", right-hand preview rail rendering tool outputs in real time). CONFIRMED [Hermes]. koe-relevance: HIGH — this is the closest competitor analog to koe's thinking-event/activity visualization (koe-sua.1), BUT koe goes further (calibrated confidence + verifiable-action framing, not just raw tool output). Hermes streams *what tool ran*; koe must also stream *how confident* + *which source*.
- **Desktop app logs to file + CLI tail (`hermes logs gui -f`, desktop.log) + boot-failure diagnostics** — Hermes. CONFIRMED [Hermes]. koe-relevance: MED — koe needs local logs for support/debug; tie to observability gap (koe-3ai).
- **Diagnostics command (`codex doctor` / `hermes doctor` — env, Git, terminal, app-server, thread inventory)** — Codex (0.135.0), Hermes. CONFIRMED [Codex, Hermes]. koe-relevance: MED — a "koe doctor" (mic OK? key valid? budget set? provider reachable?) is genuinely useful for a desktop app with hardware deps.
- **Usage stats / token activity / profile insights / activity insights & share cards** — Codex (Profile section 26.602/26.527: usage stats, token activity, activity insights, share cards; `/insights`, `/usage`). CONFIRMED [Codex, Hermes]. koe-relevance: HIGH — koe has cost balance live display (koe-9xi); usage stats ("you spoke ~X minutes / spent ¥Y this month") align with koe's prepaid-balance + time-併記 model.
- **Usage-limit / credit-limit surfacing (monthly credit limits for enterprise, account token usage exposure)** — Codex (0.137.0/0.138.0). CONFIRMED [Codex]. koe-relevance: HIGH — koe's monthly budget hardcap + "上限到達で停止" is exactly this; surfacing the cap state in UI is required (koe already has budget onboarding).
- **Billing/plan tiers + priority/flex processing toggles** — Codex (Plus/Pro/Business/Edu/Enterprise; priority processing; flex processing for cost). CONFIRMED [Codex]. koe-relevance: MED — koe M4 managed-credit; a "quality vs cost" voice-tier toggle is the analog (already: standard/high-quality voice labels).
- **Telemetry events + opt-in / developer mode** — Codex (telemetry: turn profiling, sandbox outcomes, error tracking; "Developer mode"). CONFIRMED [Codex]. koe-relevance: HIGH — koe needs a **telemetry opt-in** for Sentry (koe-3ai); consumer privacy demands explicit opt-in, off by default.
- **Sentry / error observability 3-layer + PII redaction** — koe planned (from Enitar), no direct competitor confirm. INFERRED. koe-relevance: HIGH — koe-3ai; PII redaction is critical because koe handles voice + personal notes.
- **Agent traces / evals improvement loop** — Codex ("Traces, Evals"). CONFIRMED [Codex]. koe-relevance: MED — koe's calibration layer needs trace data (AUROC tracking from research E5); internal, not user-facing.

## 8. UPDATES / UNINSTALL / BACKUP-IMPORT / DATA EXPORT & DELETION

- **Background update check + one-click install + manual update** — Hermes; Codex (Changelog, Feature Maturity); koe planned tauri-plugin-updater (M4). CONFIRMED [Hermes] / koe planned. koe-relevance: HIGH — koe M4 auto-updater + pubkey signing (code-signing gap koe-8h0); table-stakes for a distributed desktop app.
- **Tiered uninstall (GUI only / GUI+agent keep data / everything incl. user data)** — Hermes ("Uninstall Chat GUI only" / "keep my data" / "Uninstall everything"). CONFIRMED [Hermes]. koe-relevance: HIGH — consumer apps need clean uninstall with a "keep my conversations/keys?" choice; directly serves koe's data-deletion requirement (koe-0k1).
- **Data export** — INFERRED (CD/ChatGPT export conversations; Codex artifacts). koe-relevance: HIGH — export conversation history / notes (koe records to SQLite/Obsidian); ties to recorder adapters + data portability.
- **Explicit data deletion / view-edit-delete memory** — ChatGPT (Manage Memory delete). CONFIRMED [ChatGPT]. koe-relevance: HIGH — koe-0k1 data deletion; GDPR-style "delete everything" + per-item delete for voice transcripts + memory.
- **Backup / import (migrate from OpenClaw: keys, allowlist, TTS assets, workspace instructions)** — Hermes (`hermes claw migrate`). CONFIRMED [Hermes]. koe-relevance: LOW — no migration source for a new product; backup/restore of settings+keys is MED (stronghold backup).
- **Config files / advanced config / config reference / env vars / settings reset** — Codex (Config Basics/Advanced/Reference/Sample, env vars, Rules); Hermes (`~/.hermes/`, `.env`, `cli-config.yaml`). CONFIRMED [Codex, Hermes]. koe-relevance: LOW — koe is GUI-first consumer; power-user config files are dev-only. Keyboard-shortcut reset (Codex 26.527) is MED.

## 9. UX SURFACES (palette, shortcuts, zoom, theme, i18n, tray, notifications, voice)

- **Command palette (Cmd+K)** — Codex ("Cmd+K opens the command palette"); Hermes (Cmd+K / Ctrl+K); Raycast (the whole app is a palette). CONFIRMED [Codex, Hermes, Raycast]. koe-relevance: LOW-MED — koe is voice-first; voice *is* the command interface. A minimal palette for power actions (mute, stop, settings) is fine but secondary to the orb.
- **Rebindable / searchable keyboard shortcuts + reset-all** — Hermes ("Rebindable shortcuts"); Codex (keypress search, reset-all, configurable interrupt binding). CONFIRMED [Hermes, Codex]. koe-relevance: MED — koe needs at minimum a global push-to-talk / mute / stop hotkey (always-on app); full rebinding is nice-to-have.
- **Custom zoom / interface scaling (half-step)** — Hermes. CONFIRMED [Hermes]. koe-relevance: LOW-MED — koe's tall-narrow orb window has fixed-ish layout; accessibility scaling is worth it for the thinking-window text.
- **Theme: dark/light + OS-following** — Codex (dark/light imagery); koe planned OS-following light/dark (koe-ios redesign). CONFIRMED [Codex] / koe core. koe-relevance: HIGH — koe's redesign explicitly does OS-following ambient color; this is a defining surface, not optional.
- **i18n / UI language switcher (incl. Simplified Chinese)** — Hermes ("UI language switcher"). CONFIRMED [Hermes]. koe-relevance: HIGH — koe is Japanese-first; i18n (JA/EN) is needed for both UI and the voice persona; voice TTS supports 36 langs (Qwen3-TTS research).
- **Tray / menubar residency** — INFERRED (always-on apps standard); koe planned (koe-944). koe-relevance: HIGH — core to always-on; the orb may minimize to tray.
- **Notifications: never / background-only / always modes + system notifications** — Codex ("never/background-only/always"). CONFIRMED [Codex]. koe-relevance: HIGH — koe notifications gap (koe-hah); 3-mode control is the right granularity; voice secretary needs "notify when I'm away / approval needed".
- **Pop-out / floating window + "stay on top" toggle** — Codex (pop-out thread, stay-on-top). CONFIRMED [Codex]. koe-relevance: HIGH — koe's orb as an always-visible ambient floating window with stay-on-top is exactly the redesign direction; this validates it.
- **Voice dictation / voice input+output / voice mode** — Codex (Ctrl+M transcription); Hermes (voice input+output, voice memo transcription, mic permission); ChatGPT (9 voices, Advanced Voice Mode — note macOS voice *retired* Jan 2026). CONFIRMED [all]. koe-relevance: HIGH (CORE) — voice is koe's entire surface; note ChatGPT *retired* desktop voice → market gap koe fills. koe needs barge-in/semantic interruption (Qwen3.5-Omni research), voice picker, mic permission UX.
- **Drag-and-drop file attachment** — Hermes, Codex (appshots/screenshots). CONFIRMED [Hermes, Codex]. koe-relevance: MED — drop a file for koe to read/summarize fits a secretary; keep minimal around orb.
- **File browser / working-directory explorer + preview rail** — Hermes, Codex (sidebar: plans/sources/file previews; Git diff panel). CONFIRMED [Hermes, Codex]. koe-relevance: DEV-ONLY mostly (skip the Git diff/working-dir browser); BUT the "sources/preview rail" maps to koe's thinking-window showing referenced sources — adopt the *source-disclosure* part, skip the file-tree.
- **First-run / onboarding overlay + "choose provider later" deferral** — Hermes (redesigned unified overlay, "Choose provider later"); koe has budget onboarding. CONFIRMED [Hermes] / koe. koe-relevance: HIGH — koe needs initial tutorial (koe-30t) + mic/key onboarding; "defer setup" pattern good for trial-first UX.
- **Appshot / screenshot-to-agent** — Codex ("Send the frontmost Mac app window to Codex with a screenshot"). CONFIRMED [Codex]. koe-relevance: MED — "look at my screen" voice command fits a secretary; gated as CAUTION (screen capture = sensitive).

## 10. PRIVACY / CONSENT / RECORDING CONSENT / TERMS / DATA RESIDENCY

- **Local-only execution option ("Make sure Local is selected to work on your machine")** — Codex. CONFIRMED [Codex]. koe-relevance: HIGH — koe is local-first M1; "your data stays on device" is a selling point (matches Raycast "all data stored locally").
- **Data-stored-locally privacy posture + provider no-train agreements** — Raycast ("all your data stored locally", "providers prohibited from training on interactions"). CONFIRMED [Raycast]. koe-relevance: HIGH — koe should state the same; BYOK means OpenAI's API data policy applies, must disclose.
- **Recording consent (voice/transcript)** — INFERRED (no competitor is always-listening voice); koe planned (koe-n6s 録音同意). koe-relevance: HIGH (UNIQUE) — **koe is always-listening; recording consent + a visible "listening" indicator on the orb is legally and ethically mandatory and is a differentiator competitors don't face.** No competitor confirms this because none are ambient voice recorders. This is koe-specific net-new.
- **Terms of service / EULA acceptance** — INFERRED; koe planned (koe-n6s 規約). koe-relevance: HIGH — required for distribution + recording.
- **Data residency / geo-restrictions (computer use unavailable in EEA/UK/Switzerland)** — Codex. CONFIRMED [Codex]. koe-relevance: HIGH — koe must consider EU/UK voice-recording + AI-act compliance; geo-gating risky features (computer_use) is a real pattern.
- **Enterprise managed policy / MDM config / extension allowlist** — CD (admin console, managed settings files, MDM via Jamf/Kandji/Intune reading `com.anthropic.claudefordesktop`, extension allowlist overriding in-app). CONFIRMED [CD]. koe-relevance: DEV/ENTERPRISE-ONLY (skip for M1-M4 consumer) — revisit only if koe ever sells to orgs.
- **"Your data" / data-use documentation page** — Codex, Raycast (privacy page). CONFIRMED [Codex, Raycast]. koe-relevance: HIGH — koe needs a plain-language privacy doc (esp. re: voice).
- **Safety best practices / cybersecurity checks docs** — Codex. CONFIRMED [Codex]. koe-relevance: MED — koe's safety thesis warrants a public safety doc.

---

## SYNTHESIS NOTES (for downstream step — what to ADOPT vs SKIP)

**ADOPT (table-stakes koe is missing or under-built, HIGH relevance):**
- Tray/always-on residency + "prevent sleep" (koe-944) — core to always-on identity.
- Notifications 3-mode (never/background/always) (koe-hah).
- Memory view/edit/delete + on-off toggle + data deletion + tiered uninstall (koe-0k1, koe-sua.3).
- Searchable conversation history UI (koe-sh6).
- Telemetry **opt-in** (off by default) for Sentry + PII redaction (koe-3ai).
- Recording-consent + visible listening indicator + ToS (koe-n6s) — **koe-unique, no competitor reference**.
- OS-following theme + stay-on-top floating orb (koe-ios) — competitors confirm pop-out/stay-on-top is real.
- Auto-updater + code signing (koe-8h0).
- i18n (JA/EN), mic-permission UX + recovery (koe-8t2/8kw), onboarding tutorial (koe-30t).
- Per-tool Allow/Ask/Block (CD model) layered onto koe's SAFE/CAUTION/DANGER; "approve for this session" scope; Lockdown/paranoid mode.
- Usage stats surfacing ("minutes spoken / ¥ spent / cap state").
- "koe doctor" diagnostics (mic/key/budget/provider reachability).

**ADOPT-CAREFULLY (good idea, must not dilute thesis):**
- Personality/tone presets — but must not corrupt calibrated-confidence honesty.
- MCP client + tool marketplace — yes, but voice-announce every tool (transparency) and keep off the orb surface.
- Cached-vs-live web search toggle (cheaper/safer default).
- Remote approve-from-phone (M4, compelling for always-on but heavy infra).
- Cron/NL scheduling + review queue (idle curator koe-sua.6).

**SKIP / DILUTIVE (developer-coding-agent features that would erode a consumer voice secretary):**
- Parallel threads / Git worktree / multi-profile sessions / cross-profile @links.
- Working-directory file browser + Git diff/stage/revert panel.
- Multi-agent command center / subagent delegation / Python RPC pipelines.
- Container backends (Docker/SSH/Singularity/Modal/Daytona).
- Messaging gateways (Telegram/Discord/Slack/WhatsApp/Signal) — contradicts "talk like a person via voice".
- YOLO auto-approve toggle — directly contradicts fail-closed + "human judges when to intervene" thesis.
- 200-model provider catalog / enterprise MDM/RBAC/managed-policy / named permission profiles / config-file-first power config.
- In-app full browser embed.

**KEY DIFFERENTIATION CONFIRMED:** Hermes's "streaming responses with live tool activity" and Codex's "sidebar: plans/sources/task summaries" are the *closest* competitors to koe's transparency — but **both stop at showing raw tool output / sources**. **Neither shows a calibrated confidence label, and neither does it by voice in real time.** koe's thinking-window (verifiable action + source + calibrated confidence tier, spoken) is genuinely vacant in the surveyed field — consistent with koe's novelty research. The competitive risk is not "they have transparency too" but "koe accidentally copies their dev-tool chrome (threads/worktrees/gateways/palette) and buries the orb."

**Sources:** [Codex App](https://developers.openai.com/codex/app) · [Codex App Features](https://developers.openai.com/codex/app/features) · [Codex Computer Use](https://developers.openai.com/codex/app/computer-use) · [Codex Changelog](https://developers.openai.com/codex/changelog) · [Codex Plugins](https://developers.openai.com/codex/plugins) · [Codex Memories](https://developers.openai.com/codex/memories) · [Codex Skills](https://developers.openai.com/codex/skills) · [Introducing the Codex app](https://openai.com/index/introducing-the-codex-app/) · [Codex for almost everything](https://openai.com/index/codex-for-almost-everything/) · [Hermes Desktop user guide](https://hermes-agent.nousresearch.com/docs/user-guide/desktop) · [Hermes Agent GitHub](https://github.com/nousresearch/hermes-agent) · [MarkTechPost Hermes Desktop](https://www.marktechpost.com/2026/06/03/nous-research-releases-hermes-desktop-a-native-cross-platform-front-end-for-hermes-agent-v0-15-2-with-streaming-tool-output/) · [DigitalApplied Hermes Desktop](https://www.digitalapplied.com/blog/hermes-desktop-v0-15-2-nous-research-open-source-agent-2026) · [Claude Desktop MCP/enterprise (Claude Help Center)](https://support.claude.com/en/articles/12622667-enterprise-configuration-for-claude-desktop) · [Raycast AI Manual](https://manual.raycast.com/ai) · [Raycast AI Privacy](https://manual.raycast.com/ai/raycast-ai-privacy-security) · [ChatGPT features/memory/personalization](https://www.chat-power.com/blog/chatgpt-features-complete-guide-2026/) · [MacStories Codex superapp](https://www.macstories.net/news/openai-unveils-codex-superapp-update-with-computer-use-automations-built-in-browser-and-more/)