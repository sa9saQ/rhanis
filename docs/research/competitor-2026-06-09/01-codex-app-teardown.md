I have exhaustive coverage from primary and authoritative secondary sources. Compiling the final teardown now.

---

# CODEX DESKTOP APP vs HERMES AGENT DESKTOP — EXHAUSTIVE TEARDOWN (2026)

Legend: **[C]** = CONFIRMED in a primary/official source · **[C2]** = confirmed in authoritative secondary (quotes the announcement/docs) · **[INF]** = inferred. Source URL after each claim.

NOTE ON SOURCING: `openai.com/index/introducing-the-codex-app/` and `openai.com/index/codex-for-almost-everything/` returned **HTTP 403** to the fetcher; their content is recovered via OpenAI's own developer docs (developers.openai.com/codex/app/*), the official changelog, and secondary outlets that quote the announcements (MacStories, digitalapplied, danielvaughan Codex Knowledge Base, releasebot, Verdent, getpushtoprod).

═══════════════════════════════════════════════════════════════
## PART A — OPENAI CODEX DESKTOP APP
═══════════════════════════════════════════════════════════════

### A1. WINDOW & LAYOUT

- **Tech base**: Electron app. [C2] codex.danielvaughan.com/2026/04/17/...; getpushtoprod.substack.com/p/complete-beginners-guide-to-openais
- **Three primary regions**: "a project sidebar, active thread, and review pane." [C] developers.openai.com/codex/app — framed as "a focused desktop experience for working on Codex threads in parallel."
- **Project sidebar (left)**: "Every top-level item is a project. Inside each project, you create threads." Each thread = one agent instance. Visual indicators distinguish **local / cloud / worktree** executions. [C2] getpushtoprod.substack.com/...; verdent.ai/guides/what-is-codex-app
- **One window, many projects**: "Use one Codex app window to run tasks across projects. Add a project for each codebase and switch between them as needed." [C] developers.openai.com/codex/app/features
- **Sidebar = collapsible sections**: "Added collapsible sidebar sections" (v26.417, Apr 20 2026). [C2] releasebot.io/updates/openai/codex
- **Toggle sidebar**: `Cmd+B`. [C] developers.openai.com/codex/app/commands
- **Skills in sidebar**: "click Skills in the sidebar" to view/explore team-created skills across projects. [C2] WebSearch snippet of developers.openai.com features; intuitionlabs.ai
- **Task sidebar (live agent panel)** — four sections, real-time: [C2] codex.danielvaughan.com/2026/04/17/...
  - **Plan** — "see the agent's decomposed steps and current progress"
  - **Sources** — "view which files, URLs, or context the agent has consulted"
  - **Artifacts** — "open generated files directly from the sidebar"
  - **Summary / Task summary** — "a concise overview of the task's state, useful when returning to a thread after a break"
  - Stated design intent: "Rather than watching a stream of tool calls scroll past, you get a structured view of what the agent is doing, what it has produced, and what it intends to do next." [C2] same
- **Artifact viewer**: previews non-code generated files inline — "PDFs, spreadsheets, documents, and presentations." Revisions requestable in-thread. [C] developers.openai.com/codex/app/features ("PDF, spreadsheet, document, and presentation viewing"); [C2] codex.danielvaughan.com
- **Review pane (diff)**: shows uncommitted changes / all branch changes / last-turn modifications. Scope toggle: **"Unstaged," "Staged,"** and branch-comparison views. Files can show staged AND unstaged simultaneously. [C] developers.openai.com/codex/app/review
- **Toggle diff panel**: `Cmd+Option+B`. [C] developers.openai.com/codex/app/commands
- **Integrated terminal**: per-thread, "scoped to current project/worktree." Multiple terminal tabs — "run builds, tests, or Git commands in parallel terminals scoped to the current project." Codex reads terminal output for context. [C] developers.openai.com/codex/app/features; [C2] codex.danielvaughan.com
  - **Default terminal location** setting (bottom vs right panel), added in General settings v26.601 Jun 1 2026. [C] developers.openai.com/codex/changelog
- **In-app browser**: "open rendered pages, leave comments, or let Codex operate local browser flows." Derived from OpenAI's **Atlas** browser. For localhost / file-backed previews; "Browse and comment on pages to instruct the agent." Browser comments "mark specific page elements." Not compatible with authentication flows at launch. [C] developers.openai.com/codex/app; developers.openai.com/codex/app/features; [C2] macstories.net; digitalapplied.com/blog/openai-codex-desktop-computer-use-plugins-guide
- **Multi-window**: "you can run parallel projects across separate windows — useful for monorepo workflows" (added v26.415). [C2] codex.danielvaughan.com/2026/04/17/...
- **Floating window**: "Pop-out separate window with 'stay on top' toggle… Ideal for front-end iteration workflows." [C] developers.openai.com/codex/app/features
- **Tray/menubar**: "tray usage-limit surfacing" added v26.417 (Apr 20). The system tray surfaces usage-limit status. [C2] releasebot.io; WebSearch snippet
- **Window chrome / opacity**: theme supports `theme.opaqueWindows` boolean (translucent vs opaque chrome). [C2] codex.danielvaughan.com/2026/03/30/...
- **Status surfacing**: completion + approval **Notifications** (customizable timing). **"Prevent sleep while running"** toggle. [C] developers.openai.com/codex/app/features + /settings

### A2. NAVIGATION & SHORTCUTS (all [C] developers.openai.com/codex/app/commands unless noted)

Command menu / palette:
- `Cmd+Shift+P` or `Cmd+K` — **Command menu** (command palette)
- A **command-palette theme switcher** lives in the palette (v26.417). [C2] releasebot.io

Window / view:
- `Cmd+,` — Settings · `Cmd+/` — Keyboard shortcuts · `Cmd+O` — Open folder
- `Cmd+[` — Navigate back · `Cmd+]` — Navigate forward
- `Cmd++`/`Cmd+=` — Increase font · `Cmd+-`/`Cmd+_` — Decrease font
- `Cmd+B` — Toggle sidebar · `Cmd+Option+B` — Toggle diff panel
- `Cmd+J` — Toggle terminal · `Ctrl+L` — Clear terminal

Threads:
- `Cmd+N` or `Cmd+Shift+O` — New thread
- `Cmd+G` — Search threads ("Cmd/Ctrl+G to reopen a past conversation"; expanded match covers **conversation content + Git branch names**). [C2] WebSearch; developers.openai.com/codex/changelog (v26.527)
- `Cmd+F` — Find in thread
- `Cmd+Shift+[` — Previous thread · `Cmd+Shift+]` — Next thread
- `Ctrl+M` — Dictation (hold `Ctrl+M` while composer visible and talk → transcription with editing). [C] /features + /commands

Slash commands (in-app): `/feedback`, `/goal` (persistent goal), `/mcp` (MCP status), `/plan` (plan mode toggle), `/review` (code review of uncommitted changes), `/status` (thread ID + context usage + rate limits). Also `/plugins` (terminal) for plugin install, `/theme` (CLI-side picker). [C] /commands; [C2] digitalapplied (plugins); danielvaughan (theme)

Shortcut settings: keypress search ("keystroke search functionality") + "Reset to defaults." (v26.527 added shortcut keypress search + reset-all.) [C] /settings; /changelog

### A3. FEATURES

**Parallel threads / multi-agent (the headline)**:
- "Manage multiple agents at once, run work in parallel… collaborate with agents over long-running tasks." "Command center for agents." [C2] WebSearch of openai.com/index/introducing-the-codex-app
- "Agents run in separate threads organized by projects… switch between tasks without losing context." [C2] same. Threads can be pinned, archived, managed across worktrees. [C] /features
- "Multi-agent v2 runtime persistence per thread"; "thread coordination for local projects and worktrees with background thread support" (v26.527). [C] /changelog

**Three execution modes per project**: **Local** (direct project dir), **Worktree** (isolated git worktree), **Cloud** (remote configured env). [C] /features
- Worktree: "Codex clones your current branch into a separate Git worktree, giving you two parallel local environments… run two agents on the same codebase without them stepping on each other's files." "Zero merge conflicts." Worktrees inherit only git-tracked files; may need setup scripts for deps. Automations run in dedicated background worktrees; "frequent automations can create many worktrees over time" (archive recommended). [C2] codex.danielvaughan; verdent; [C] /troubleshooting

**Git / PR review**:
- Diff pane with inline comments; commit / push / PR creation; stage/revert at **full-diff ("Stage all"/"Revert all"), per-file, and per-hunk** levels. [C] /review
- Inline feedback: hover line → click **+** → submit; Codex treats comments as "review guidance." `/review` shows comments "directly inline in the review pane." [C] /review
- **GitHub PR context** pulled into sidebar when `gh` CLI is authenticated: "reviewer comments, changed files, and diffs." "Ask Codex to fix the specific comments," then "Stage, commit, and push." [C] /review; [C2] codex.danielvaughan
- Settings: branch naming standardization, force-push prefs, prompts for "Commit messages and pull request descriptions." [C] /settings

**Computer use** (full breakdown in A-CU below): see/click/type with its own cursor; parallel agents on macOS in background; macOS + Windows; install via plugin. [C] developers.openai.com/codex/app/computer-use

**Automations**:
- "Schedule recurring tasks, or wake up the same thread for ongoing checks." [C] /features
- Two flavors: **Thread automations** = "recurring wake-up calls preserving conversation context" ("Heartbeat automations stay in the same Codex thread"); **Project-level automations** = "start fresh recurring tasks" in background worktrees. [C] /features; [C2] macstories
- "Codex can now schedule future work for itself and wake up automatically to continue on a long-term task, potentially across days or weeks." Cloud-based triggers so it runs "not just when your computer is open." Model can modify its own automations. [C2] openai.com via WebSearch; macstories
- "Combine skills with automations to perform routine tasks such as evaluating errors in your telemetry and submitting fixes." [C] /features

**SSH to remote devboxes**: "Remote devbox SSH (alpha feature)." [C2] macstories; digitalapplied ("SSH remote devbox: Alpha-stage remote development environments"). Note: NOT mentioned in the Feb macOS-launch guide (verdent lists it as ❌ "not mentioned" at launch) → **added post-launch, alpha**. [C2] verdent

**In-app browsing**: localhost + file-backed previews; comment on rendered elements; "let Codex operate local browser flows." Browser-use improvements (v26.519) added "asset extraction and structured data" across in-app browser & Chrome. "Improved Chrome context capture for Google Docs, Sheets, and Slides" (v26.527). [C] /features; /changelog

**Image generation**: "Ask Codex to generate or edit images directly in a thread." Model identified variously as **gpt-image-2** (official /features) and **gpt-image-1.5** (secondary). Counts toward general usage limits. "No ChatGPT round-trip." Use cases: presentations, website mockups, product concepts, frontend mocks, game art. Standalone image-generation extension also exists (CLI 0.136.0). [C] /features (gpt-image-2); [C2] digitalapplied/macstories (gpt-image-1.5)

**Memory ("Memories")**: "Carry context from past tasks into future threads… project conventions and recurring patterns." Preview feature; "Persists preferences, tech stacks, recurring workflows across threads"; "remembers useful context, preferences, and corrections"; proactive next-action suggestions. Rollout to Enterprise/Edu/EU "soon." Toggle in Settings → **Memories** "where available." [C] /features + /settings; [C2] digitalapplied/macstories

**Plugins (marketplace, 90+/100+)**: each plugin = bundle of **skills + app integrations + MCP servers**. Install via `/plugins` (terminal); self-published marketplaces supported; plugin sharing via marketplace sources for ChatGPT Business (v26.519). [C] /features; [C2] digitalapplied; macstories; /changelog
- Named plugins/categories: Design (Figma, Adobe Creative Cloud); Docs/Knowledge (Notion, Box, Google Drive, Google Workspace); Dev (GitHub, GitLab, CircleCI, Jira, Linear, Sentry, CodeRabbit); Comms (Slack, Microsoft Teams/Suite); PM (Trello, Jira, Linear); Data (SQL connectors, Neon); Scheduling (Google Calendar); plus Atlassian Rovo, Render, Remotion, Superpowers, **Sites** (preview — create/save/deploy/inspect websites, manage hosted env vars & secrets via Sites sidebar). [C2] digitalapplied; macstories; [C] /changelog (Sites)
- **Computer Use** and **Browser Use** are themselves installable plugins. [C] /computer-use; /settings

**Skills**: reusable instruction bundles shared across **app, CLI, and IDE Extension**. Browse open-source skill packs (GitHub-hosted), install, or author your own. View team skills via Skills sidebar. [C] /features; [C2] intuitionlabs; WebSearch

**Local branch search + file pasting**: "Added local branch search and non-image file pasting in the composer" (v26.417, Apr 20). [C2] releasebot.io

**Usage-limit tray surfacing**: tray surfaces usage limits (v26.417). `/status` shows rate limits; "Monthly credit limits display" (CLI 0.137). [C2] releasebot; [C] /commands; /changelog

**IDE sync**: "If you have the Codex IDE Extension installed… your Codex app and IDE Extension automatically sync" — threads, file context, agent state across app + VS Code. "IDE context" option with auto-context file tracking. [C] /features

**Chats (project-independent threads)**: use Codex-managed `~/.codex/threads`; for research/triage/plugin-heavy workflows. [C] /features

**MCP**: shared "Model Context Protocol (MCP) settings" across app/CLI/IDE; recommended server enablement; custom config; OAuth credential refresh for MCP (CLI 0.138). [C] /features; /changelog

**Web search**: first-party tool, enabled by default for local tasks; cache vs live depending on sandbox config; parallel web search (CLI 0.137). [C] /features; /changelog

**Voice input**: hold `Ctrl+M`, talk, transcribe + edit. [C] /features

**Appshots**: "send the frontmost Mac app window to Codex with a screenshot and available text" (macOS, v26.519). [C] /features; /changelog

**Profile section**: activity insights, share cards, profile card generation/sharing, usage stats, token activity, "peak tokens, streaks, longest task." (v26.527 → v26.602.) [C] /settings; /changelog

**Goal mode**: `/goal` persistent goal; "no longer experimental" across app/IDE/CLI (v26.519). [C] /commands; /changelog

**Remote control**: connect via ChatGPT mobile (iOS/Android) or Mac Codex to "start new threads, continue existing work, send follow-up instructions, answer questions, approve actions." Windows can be controlled remotely but cannot yet control another computer. [C] /features; [C2] kingy.ai

### A-CU. COMPUTER USE (deep)

- Definition: "see and operate graphical user interfaces on macOS or Windows" — view screen, take screenshots, interact with windows/menus/keyboard/clipboard "with its own cursor." [C] /computer-use; [C2] openai via WebSearch
- **macOS = background + parallel**: "Multiple agents can work on your Mac in parallel, without interfering with your own work in other apps" — doesn't steal foreground focus. **"Locked use"**: operates after the Mac locks via an Apple authorization plug-in with safeguards ("Remote computer use… after your Mac locks," v26.519). [C] /computer-use; [C2] openai/WebSearch; macstories
- **Windows = foreground only**: "runs on the active desktop and cannot operate in the background while you continue using the same Windows session"; "will move the pointer, type, and take over foreground input." (v26.527.) [C2] kingy.ai
- **Enable**: install "Computer Use plugin" from Codex settings. macOS prompts for **Screen Recording + Accessibility** permissions. Windows requires keeping target app visible; install Codex via `winget install Codex -s msstore`. [C] /computer-use; [C2] kingy.ai
- **Permission model**: per-app permission prompt before access; **"Always allow"** option + a dedicated "Always allow" list in Computer Use settings (revocable). May "ask before taking sensitive or disruptive actions." [C] /computer-use
- **Hard limits**: cannot automate terminal apps or Codex itself; cannot authenticate as administrator or approve security/privacy permission prompts. [C] /computer-use
- **Region**: unavailable in **EEA, UK, Switzerland at launch**. [C] /computer-use; /features
- Use cases: testing desktop/iOS-simulator apps, browser tasks, reproducing UI bugs, adjusting app settings, inspecting non-plugin data sources, multi-app workflows. [C] /computer-use
- v26.602 "Improved Computer Use startup readiness and appshot error reporting." [C] /changelog

### A4. SETTINGS / CONFIGURATION (all [C] developers.openai.com/codex/app/settings unless noted)

Settings categories and notable options:
- **General**: file-opening location; command-output display in threads; **Default terminal location** (bottom/right); "require Cmd+Enter for multiline prompts"; **Prevent sleep during thread execution**.
- **Profile**: activity insights, token metrics (peak tokens, streaks, longest task), profile picture + display name, profile card generation/sharing.
- **Keyboard Shortcuts**: review + rebind; keystroke search; reset-to-defaults.
- **Notifications**: turn-completion timing; permission-prompt prefs.
- **Agent Configuration**: inherits IDE/CLI settings; common controls in-app; advanced via `config.toml`.
- **Appearance**: Base theme; Accent / Background / Foreground(ink) colors; **Opacity**; UI font; Code font; Semantic colors (diffAdded, diffRemoved, skill callout); custom-theme sharing (Import/Export `codex-theme-v1:…`); **Codex Pets** subsection (built-in + custom). [C] /settings; [C2] danielvaughan
- **Git**: branch naming, force-push prefs, commit-message + PR-description prompts.
- **Integrations & MCP**: connect external tools via MCP; recommended server enablement; OAuth.
- **Browser Use**: install/enable browser plugin; Chrome-extension setup; website allowlist/blocklist.
- **Computer Use**: review desktop-app access; system-level permission mgmt; "Always allow" list.
- **Personalization**: Personality modes **"Friendly," "Pragmatic," "None"**; custom instructions linked to `AGENTS.md`.
- **Context-Aware Suggestions**: follow-up + task-surface resumption.
- **Memories**: enable where available.
- **Archived Threads**: list with dates; unarchive.
- **Accounts/auth/billing**: sign in via ChatGPT account; "Monthly credit limits display"; usage limits surfaced in tray + `/status`; Amazon Bedrock auth/billing path (CLI). [INF for exact UI labels; C2 for existence] releasebot; /changelog

### A5. ONBOARDING & FIRST-RUN

- Sign in with ChatGPT account (plan-gated; e.g. Plus "30–150 messages per 5-hour window"). [C2] verdent
- **Role-based onboarding**: "Expanded onboarding with more role choices for tailored first-run suggestions" (v26.602). [C] /changelog
- First action = Open folder / Add a project, then create threads; sessions persist across restarts. [C2] getpushtoprod
- IDE-extension auto-pairs if present. [C] /features

### A6. VISUAL DESIGN LANGUAGE

- **Highly themeable**: `codex-theme-v1` JSON wire format (URI-prefixed, shareable as `codex-theme-v1:…`). Fields: `codeThemeId` (e.g. "one","catppuccin","matrix"), `variant` (dark/light), `theme.accent`, `theme.ink` (primary text), `theme.surface` (background), `theme.contrast` (int ratio), `theme.fonts.code`, `theme.fonts.ui`, `theme.opaqueWindows` (translucent/opaque), `theme.semanticColors` (diffAdded/diffRemoved/skill). [C2] danielvaughan
- **Built-in themes**: Catppuccin, Monokai, Solarized (light/dark), "one," "matrix." **Partner themes**: Linear, Notion, OpenClaw (dark high-contrast). [C2] danielvaughan
- Configurable UI font + code font; per-theme custom fonts. [C] /settings
- **Playful signature**: **Codex Pets** (built-in + custom desktop "pet" companions in Appearance). [C] /settings
- Density: IDE-grade, multi-pane, structured "live panel" over raw tool-call stream — positioned as "the kind of stuff that's impossible in a terminal UI" (rich rendering, image previews, animated diff stats, hex color swatches). Polish signals from changelog: "animated diff stat alignment," "hex color swatches," "terminal scrollbar alignment," "fullscreen browser composer controls." [C2] getpushtoprod; [C] /changelog
- Subagent visual identity: "stable identicons for background subagents." [C] /changelog

### A7. WHAT CODEX DOES NOT DO (gaps/limits)

- **No offline/local-only mode** — "Requires internet." [C2] verdent
- **Computer Use blocked in EEA/UK/Switzerland**; cannot automate terminals or Codex itself; cannot do admin auth or approve OS security prompts. [C] /computer-use
- **Windows Computer Use is foreground-only** (no background/parallel like macOS); Windows can't remote-control other machines (only be controlled). [C2] kingy.ai
- In-app browser: **no authentication flows**; "limited to running sites and apps via a local server setup" at launch (broader internet interaction "future"). [C] /features; [C2] macstories
- SSH remote devbox = **alpha**, not at original launch. [C2] macstories/verdent
- App vs CLI parity gap: "might rely on different versions of the agent… some experimental features land in the CLI first." [C] /troubleshooting
- Worktrees inherit only git-tracked files; deps need setup scripts; worktree accumulation from automations. [C] /troubleshooting
- Review panel shows ALL git-state changes (even non-Codex), can be noisy. Terminal panels can get stuck (workaround: reopen `Cmd+J`). [C] /troubleshooting
- Memory/EU rollout incomplete ("coming soon"). [C2] macstories
- **Linux**: app availability "pending" / not shipped. [C] /codex/app

═══════════════════════════════════════════════════════════════
## PART B — HERMES AGENT DESKTOP (Nous Research)
═══════════════════════════════════════════════════════════════
Versions: public preview **v0.15.2** (Jun 3 2026); agent core at **v0.16.0**. License **MIT**. [C] hermes-agent.nousresearch.com; [C2] marktechpost; digitalapplied

### B1. WINDOW & LAYOUT

- **Tech stack**: **Electron + React shell over a Python backend**, reusing the shared agent core. "The packaged app ships only the Electron shell. On first launch it installs the Hermes Agent runtime into `HERMES_HOME`… The React renderer talks to a `hermes dashboard` backend over the standard gateway APIs." [C] /docs/user-guide/desktop; [C2] digitalapplied
- **"Chat-first window with a left sidebar for navigation."** [C] /desktop
- **Main chat area**: streaming responses with live tool activity + structured tool-call summaries; drag-and-drop file attach anywhere in chat. [C] /desktop
- **Right-hand preview rail**: "render web pages, files, and tool outputs side by side while you keep chatting." Called "the clearest single thing the desktop adds over a terminal" and noted to serve a security/audit function for non-technical users. [C] /desktop; [C2] digitalapplied
- **Bottom status bar**: "live session state and… quick controls" — **inline model picker** (switch model for active session) + **per-session YOLO toggle** (bypass dangerous-command approval prompts, matching the TUI). [C] /desktop
- **Composer**: history + queue editing via up/down arrows in empty composer. [C] /desktop

### B2. LEFT SIDEBAR / MANAGEMENT PANES

Sidebar gives access to: Chat sessions, File browser, Voice mode, Settings, and broader management panes. [C] /desktop
- **File browser**: "Explore and preview the working directory without leaving the app." Configurable via `hermes desktop --cwd <path>` or `HERMES_DESKTOP_CWD`. [C] /desktop
- **Skills**: "browse, install, and manage skills" (compatible with agentskills.io open standard). [C] /desktop; github
- **Cron**: "view and manage scheduled jobs" (natural-language cron scheduler). [C] /desktop
- **Profiles**: "switch between Hermes profiles (isolated config/skills/sessions)"; concurrent multi-profile sessions; cross-profile `@session` links. [C] /desktop
- **Messaging**: "set up gateway channels" (Telegram/Discord/Slack/WhatsApp/Signal/Email). [C] /desktop
- **Agents** + **Command Center**: "orchestration surfaces for multi-agent work" (spawn isolated subagents w/ separate conversations, terminals, Python RPC scripts). UI specifics not documented. [C] /desktop
- **Sessions**: "Session-list overhaul" with archiving + "general session hygiene"; **search sessions by id**; sessions resume across CLI/TUI/desktop ("started here resume in the CLI/TUI and vice versa"). [C] /desktop

### B3. NAVIGATION & SHORTCUTS

- **Command palette**: `Cmd+K` (macOS) / `Ctrl+K` (Win/Linux) — "jump to actions and navigate the app from the keyboard." [C] /desktop
- **Rebindable shortcuts** panel in Settings. **Custom zoom shortcuts** (half-step increments). [C] /desktop
- **UI language switcher** in-app (incl. Simplified Chinese zh-Hans). [C] /desktop

### B4. FEATURES

- **Voice mode**: "Talk to Hermes and hear it back" (TTS + transcription); macOS prompts once for mic. [C] /desktop
- **Multi-model / model-agnostic**: GUI surfaces "every provider and model that `hermes model` knows about." 200+ models via Nous Portal, OpenRouter, NovitaAI, NVIDIA NIM, Xiaomi MiMo, z.ai/GLM, Kimi/Moonshot, MiniMax, Hugging Face, OpenAI, local vLLM, or any OpenAI-compatible endpoint. **No model lock-in.** [C] /desktop; github; digitalapplied
- **xAI Grok OAuth** = first-class OAuth provider in launcher (browser flow). [C] /desktop
- **Auxiliary-model split warning**: warns if main model's provider differs from where auxiliary tasks (titling/summarization) are pinned. [C] /desktop
- **Tool-backend installs from GUI**: run a tool backend's post-setup install steps in-app instead of dropping to terminal. [C] /desktop
- **MCP** tool integration. [C2] digitalapplied
- **Sandboxing backends** (inherited from CLI): **Local, Docker, SSH, Singularity, Modal** (digitalapplied/marktechpost) — github README also lists **Daytona** (six backends) — with container hardening (read-only root FS, dropped Linux capabilities, namespace isolation). [C2] digitalapplied; marktechpost; [C] github
- **Built-in tools**: web search, browser automation, vision, image generation, text-to-speech, multi-model reasoning. [C2] digitalapplied
- **Memory**: agent-curated memory w/ periodic nudges; autonomous skill creation after complex tasks; FTS5 session search + LLM summarization for cross-session recall; Honcho dialectic user modeling. [C] github
- **Cross-surface state**: desktop = "the 8th surface over one unified gateway." Config, API keys, sessions, skills, memory all shared. "If you have used `hermes` in a terminal, everything you set up there is already here, and anything you do here shows up there." [C] /desktop; [C2] digitalapplied
- **Remote backend**: app starts/manages its own **local** backend by default, or point at a remote Hermes (`Settings → Gateway → Remote gateway`: Remote URL e.g. `http://<host>:9119`, provider sign-in, auto-reconnect, per-profile remote host). Remote requires a separately-running `hermes dashboard` ("does not start it for you… you or a `systemd` service keep it running"). Auth: **OAuth (Nous Portal)** for anything beyond local; **username/password** for local/trusted-network only. [C] /desktop
- **Auto-update**: background check + one-click update. [C] /desktop

### B5. SETTINGS / CONFIG

- **Providers pane**: manage inference providers w/ "Accounts / API-keys UX for signing in and storing credentials per provider." [C] /desktop
- **Gateway**: remote-gateway config (above). [C] /desktop
- **Shortcuts** panel (rebindable). **UI language**. **Zoom**. [C] /desktop
- **About → Danger zone** uninstall options: "Uninstall Chat GUI only," "Uninstall GUI + agent, keep my data," "Uninstall everything." [C] /desktop

### B6. ONBOARDING & FIRST-RUN

- First launch installs Hermes runtime into `HERMES_HOME` (same layout as CLI install). [C] /desktop
- **First-run onboarding redesigned on a "unified overlay design system"**; option **"Choose provider later"** to skip provider setup and enter the app first. [C] /desktop
- Installers: macOS 12+ & Windows 10/11 = direct/native installers; **Linux = terminal script only** (`--include-desktop` flag) — "partially undermining the 'no terminal needed' promise on that platform." [C] /desktop; [C2] digitalapplied; marktechpost

### B7. VISUAL DESIGN LANGUAGE

- Self-described: "a modern & thoughtfully designed UI"; onboarding on a "unified overlay design system." No color/typography/component-library specifics published; **no official screenshots in the fetched sources.** [C] /desktop (qualitative only). Density skews chat-centric (single chat column + right preview rail + slim left nav + bottom status bar), lighter-weight than Codex's IDE-grade multi-pane workspace. [INF from layout description]

### B8. WHAT HERMES DESKTOP DOES NOT DO (gaps/limits)

- **"Not a separate product or a lightweight clone"** — thin Electron front-end over the shared agent core. [C] /desktop
- **Does not manage messaging independently** — gateway is a separate process you keep running. [C] /desktop
- **Does not auto-start a remote backend** — requires pre-existing `hermes dashboard`. [C] /desktop
- **Linux has no native installer** (terminal-only). [C2] digitalapplied
- No computer-use / OS-GUI control documented (unlike Codex); capabilities are web/browser-automation + tools, not native-app driving. [INF — absent from all fetched Hermes sources]
- No Git/PR review pane, no worktree workflow, no integrated code-review/diff surface (it's an autonomous-agent chat front-end, not an IDE). [INF — absent from sources]
- Command Center / Agents orchestration UIs are surfaced but undocumented (no workflow detail, no scalability/session-count limits published). [C] /desktop (existence) + [INF gaps]

═══════════════════════════════════════════════════════════════
## PART C — HEAD-TO-HEAD DELTAS (for synthesis)
═══════════════════════════════════════════════════════════════

- **Identity**: Codex = IDE-grade "command center for parallel coding agents" (project/thread/review tri-pane, git-native). Hermes = chat-first autonomous-agent front-end (one of 8 surfaces over a unified gateway), MIT-licensed, model-agnostic.
- **Layout**: Codex = project sidebar + active thread + review/diff pane + task sidebar (plan/sources/artifacts/summary) + integrated terminal + in-app browser, multi-window. Hermes = left nav + chat column + right preview rail + bottom status bar.
- **Live transparency**: Codex = structured **task sidebar** (plan/sources/artifacts) replacing tool-call scroll. Hermes = **right preview rail** + structured tool-call summaries (its single biggest add over terminal).
- **Quick controls**: Hermes exposes **inline model picker + per-session YOLO** in status bar; Codex hides model choice deeper, uses approval prompts + "Always allow" lists.
- **Parallelism**: Codex = git worktrees + parallel threads + background computer-use agents on macOS. Hermes = spawned isolated subagents (Agents/Command Center) + concurrent multi-profile sessions.
- **Computer use**: Codex has native OS-GUI control (macOS background/locked, Windows foreground; region-restricted). Hermes has browser automation + vision but no documented native-app driving.
- **Themeability**: Codex deeply themeable (`codex-theme-v1`, partner themes, Codex Pets, opacity, fonts). Hermes only "thoughtfully designed UI" + language switcher + zoom (no theming system published).
- **Cross-surface**: Codex syncs app↔IDE↔CLI via `~/.codex`; Hermes syncs desktop↔CLI↔TUI↔chat-platforms via one gateway/`HERMES_HOME`.
- **Command palette**: both have one (Codex `Cmd+K`/`Cmd+Shift+P`; Hermes `Cmd/Ctrl+K`). Codex's is shortcut-rich (full IDE shortcut map + theme switcher in palette); Hermes's is action-navigation.
- **Platform**: Codex = macOS (Apple Silicon + Intel/macOS13+) & Windows (msstore/winget); Linux pending. Hermes = native macOS 12+ & Windows 10/11; Linux terminal-only.
- **Openness**: Hermes = MIT, self-hostable, any endpoint incl. local vLLM, six sandbox backends. Codex = closed, ChatGPT-account-gated, cloud-required (no offline mode), Bedrock billing option.

**KEY SOURCE GAP**: the two `openai.com/index/*` announcement posts were 403-blocked to the fetcher — all Codex announcement-level claims above are reconstructed from official `developers.openai.com/codex/*` docs + the official changelog + secondary outlets quoting those posts; treat announcement-only framing ("command center for agents," gpt-image version discrepancy 1.5 vs 2) as [C2] not [C].