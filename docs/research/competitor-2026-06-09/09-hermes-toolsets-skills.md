I now have the complete canonical catalogs from both authoritative source files. I have enough to produce the exhaustive structured data output.

---

# HERMES AGENT — FULL TOOLSET + SKILLS CATALOG (DATA)

Scope: Official Hermes Agent (NousResearch/hermes-agent, v0.16.0, MIT) + as surfaced in unofficial desktop (fathah/hermes-desktop). Canonical sources = `tools-reference.md` (built-in tools) and `skills-catalog.md` (bundled skills, 76 skills / 21 categories). CONFIRMED = appears verbatim in a fetched source. INFERRED = derived. Consumer = a normal person would voice-use it; Dev = developer-only.

## PART A — TOOLSETS (BUILT-IN TOOLS)

Canonical toolset keys (CONFIRMED, full registry list): `browser, clarify, code_execution, cronjob, debugging, delegation, discord, discord_admin, feishu_doc, feishu_drive, file, homeassistant, image_gen, kanban, memory, messaging, moa, rl, safe, search, session_search, skills, spotify, terminal, tts, video, vision, web, yuanbao` — src: WebSearch of configuration.md / toolsets.py [github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/configuration.md]. ~71 tools total — src: [hermes-agent.nousresearch.com/docs/reference/tools-reference].

### A1. User's observed list — verification

| User saw | Status | Canonical name / tools | Consumer/Dev |
|---|---|---|---|
| Browser Automation | CONFIRMED | toolset `browser`: `browser_navigate, browser_click, browser_type, browser_scroll, browser_back, browser_press, browser_snapshot, browser_console, browser_get_images, browser_vision` (10 core) + CDP-gated `browser_cdp, browser_dialog`. Activates when a Chrome DevTools Protocol endpoint is reachable (`/browser connect`, `browser.cdp_url`, Browserbase, Camofox). | Consumer (book/search/fill a site by voice) |
| Computer Use | CONFIRMED | `computer_use` standalone — "macOS desktop control (requires `cua-driver`)". | Consumer (drive the Mac), but macOS-gated |
| macOS | CONFIRMED (as skills, not one toolset) | The "macOS" surface = `computer_use` tool + Apple **skills**: `apple-notes, apple-reminders, findmy, imessage, macos-computer-use`. | Consumer |
| Context Engine | CONFIRMED (as plugin subsystem, NOT a tool) | Not a tool — it's a single-select **plugin** type: `hermes plugins → Provider Plugins → Context Engine`; set `context.engine`. Never auto-activated. Lives in `plugins/context_engine/`. | Dev/power-user (config) |
| Cron Jobs | CONFIRMED | toolset `cronjob`, tool `cronjob`; "natural language cron scheduling for reports, backups, briefings — running unattended through the gateway". | Consumer (voice: "every morning at 8 send me…") |
| Cross-Platform Messaging | CONFIRMED | toolset `messaging`, tool `send_message`. Desktop README: "16 messaging gateways": Telegram, Discord, Slack, WhatsApp, Signal, Matrix, Mattermost, Email (IMAP/SMTP), SMS (Twilio/Vonage), iMessage (BlueBubbles), DingTalk, Feishu/Lark, WeCom, WeChat (iLink), Webhooks, Home Assistant. Official site lists "Telegram, Discord, Slack, WhatsApp, Signal, Email, CLI". | Consumer (delivery of replies/reports) |
| Discord | CONFIRMED | toolset `discord`, tool `discord` — "read/participate". Requires `DISCORD_BOT_TOKEN`. | Consumer-ish |
| Discord Server Admin | CONFIRMED | toolset `discord_admin`, tool `discord_admin` — "server moderation (requires bot permissions)": list guilds/channels/roles, create/edit/delete channels, role grants, timeouts, kicks, bans. | Dev/admin |
| Home Assistant | CONFIRMED | toolset `homeassistant`: `ha_list_entities, ha_get_state, ha_list_services, ha_call_service`. Auto-activates when `HASS_TOKEN` set. | Consumer (smart home by voice) |
| Image Generation | CONFIRMED | toolset `image_gen`, tool `image_generate`. Requires `FAL_KEY` (FAL.ai) or Nous Portal Tool Gateway. | Consumer |
| Memory | CONFIRMED | toolset `memory`, tool `memory` — "persistent cross-session storage". Pluggable backends (Honcho, Mem0, Supermemory, OpenViking, Hindsight, Holographic, RetainDB, ByteRover). Suppressible via `agent.disabled_toolsets`. | Consumer |
| Session Search | CONFIRMED | toolset `session_search`, tool `session_search` — "FTS5-backed session queries". | Consumer ("what did we decide last week?") |
| Spotify | CONFIRMED | toolset `spotify` (7 tools): `spotify_playback, spotify_devices, spotify_queue, spotify_search, spotify_playlists, spotify_albums, spotify_library`. Requires OAuth (`hermes spotify setup`). | Consumer |
| Text-to-Speech | CONFIRMED | toolset `tts`, tool `text_to_speech`. Via Nous Portal Gateway or API key. | Consumer |
| Video Analysis | CONFIRMED | toolset `video`, tool `video_analyze` — opt-in `--toolsets video`. (Separate `video_generate` exists: opt-in, requires plugin + credential.) | Consumer |
| X (Twitter) | CONFIRMED | tool `x_search` — requires `XAI_API_KEY` or xAI OAuth. | Consumer |
| Web Search | CONFIRMED | toolset `web`/`search`, tool `web_search` — requires API key (Exa/Tavily/Parallel) or Nous Portal Gateway. | Consumer |
| Web Scraping | CONFIRMED | tool `web_extract` — "markdown extraction from URLs/PDFs" (Firecrawl-class). Part of `web` toolset. | Consumer-ish |

All 18 of the user's observed toolsets = CONFIRMED. None refuted.

### A2. Additional toolsets / tools the user did NOT list (CONFIRMED, exhaustive)

- `file` toolset — `read_file, write_file, patch, search_files` (ripgrep). Dev.
- `terminal` toolset — `terminal` (shell exec), `process` (background process mgmt). Dev.
- `code_execution` toolset — `execute_code`. Dev.
- `vision` toolset — `vision_analyze` (image understanding); `browser_vision` overlaps. Consumer.
- `skills` toolset — `skill_manage, skill_view, skills_list`. Dev/power-user.
- `todo` tool — session task list. Mixed.
- `clarify` toolset — `clarify` (asks user for a decision; surfaces in desktop as inline "clarify.request" card). Consumer-facing UX.
- `delegation` toolset — `delegate_task` (spawn isolated subagents). Dev.
- `moa` toolset — `mixture_of_agents` (multi-model reasoning; requires `OPENROUTER_API_KEY`). Dev.
- `kanban` toolset (9 tools, orchestration) — `kanban_show, kanban_list*, kanban_complete, kanban_block, kanban_heartbeat, kanban_comment, kanban_create, kanban_link, kanban_unblock*` (* = orchestrator-only). Activated by `HERMES_KANBAN_TASK` / kanban profile. Persists to `~/.hermes/kanban.db`. Requires bundled skills `kanban-worker` + `kanban-orchestrator`. Mixed (board is consumer-visible, tools are dev).
- `feishu_doc` toolset — `feishu_doc_read`. Dev/enterprise.
- `feishu_drive` toolset — `feishu_drive_add_comment, feishu_drive_list_comments, feishu_drive_list_comment_replies, feishu_drive_reply_comment`. Dev/enterprise.
- `yuanbao` toolset (Tencent 元宝, `hermes-yuanbao` platform) — `yb_query_group_info, yb_query_group_members, yb_send_dm, yb_search_sticker, yb_send_sticker`. Consumer (CN market).
- `debugging` toolset — present in registry. Dev.
- `rl` toolset — RL/trajectory tooling (batch trajectory generation, trajectory compression for training tool-calling models). Dev/research.
- `safe` toolset — present in registry (safety/guardrail tool group). Dev.
- MCP dynamic tools — appear as `mcp_<server>_<tool>` (e.g., `mcp_github_create_issue`). Dev. Desktop has an MCP server management UI.

### A3. Calendar / Email / Weather / Maps — user asked, findings

- Email — CONFIRMED but as **skill** (`himalaya`, IMAP/SMTP) + messaging gateway (Email IMAP/SMTP). Not a standalone built-in tool. Consumer.
- Calendar — CONFIRMED via skill `google-workspace` ("Gmail, Calendar, Drive, Docs, Sheets via gws CLI"). No dedicated calendar toolset. Consumer.
- Maps — CONFIRMED via skill `maps` ("Geocode, POIs, routes, timezones via OpenStreetMap/OSRM"). Consumer.
- Weather — NOT FOUND as a built-in toolset or bundled skill. INFERRED: reachable only via generic `web_search`/`web_extract` or a user-added MCP server. Treat as absent from official catalog.

## PART B — SKILLS (BUNDLED CATALOG: 76 skills, 21 categories) — CONFIRMED

Source = official `skills-catalog.md`. Format = agentskills.io open SKILL.md standard (shared with Claude Code, Codex CLI, OpenCode). Install via `python scripts/sync-hermes-skills.py`.

### B1. RISKIEST CLAIMS — verdicts

- "Drive Claude Code CLI" — CONFIRMED. Skill `claude-code`: "Delegate coding to Claude Code CLI (features, PRs)." Dir: `skills/autonomous-ai-agents/claude-code`. Dev.
- "Drive Codex CLI" — CONFIRMED. Skill `codex`: "Delegate coding to OpenAI Codex CLI (features, PRs)." Dev.
- "Drive OpenCode CLI" — CONFIRMED. Skill `opencode`: "Delegate coding to OpenCode CLI (features, PR review)." Dev.
- "Drive Hermes CLI" — PARTIALLY REFUTED / corrected. The 4th sibling skill is `hermes-agent` ("Configure, extend, or contribute to Hermes Agent"), NOT a "hermes-cli" delegation skill. The user's "Hermes CLI" label is INACCURATE — it's the self-config skill. (Note: `hermes-cli` exists separately as a platform **toolset preset**, not a delegation skill.) Dev.
- "Claude Design" — CONFIRMED. Skill `claude-design`: "Design one-off HTML artifacts (landing, deck, prototype)." Consumer-ish/creative. (This is a creative HTML-artifact skill, NOT an Anthropic product.)
- "design.md / DESIGN.md" — CONFIRMED. Skill `design-md`: "Author/validate/export Google's DESIGN.md token spec files." Dev.
- "Architecture diagram / ASCII-art (creative)" — CONFIRMED. `architecture-diagram` (dark SVG infra diagrams as HTML), `ascii-art` (pyfiglet/cowsay/boxes/image-to-ascii), `ascii-video` (video→colored ASCII MP4/GIF). Consumer/creative.
- "Video" — CONFIRMED (multiple): `ascii-video`, `manim-video` (3Blue1Brown-style math animations), media `youtube-content`, plus built-in `video_analyze`/`video_generate` tools. Mixed.
- "Email" — CONFIRMED: skill `himalaya`. Consumer.
- "GitHub" — CONFIRMED (6 skills, full category): `github-auth, github-code-review, github-issues, github-pr-workflow, github-repo-management, codebase-inspection`. Dev.
- "Autonomous AI Agents" — CONFIRMED as a category name (the 4 delegation skills above).

### B2. FULL skills catalog (all 76, verbatim names + relevance)

apple (5) — `apple-notes` C, `apple-reminders` C, `findmy` C, `imessage` C, `macos-computer-use` C.
autonomous-ai-agents (4) — `claude-code` D, `codex` D, `opencode` D, `hermes-agent` D.
creative (16) — `architecture-diagram` D, `ascii-art` C, `ascii-video` C, `baoyu-infographic` C, `claude-design` C, `comfyui` C, `design-md` D, `excalidraw` C, `humanizer` C, `manim-video` C, `p5js` D, `popular-web-designs` D, `pretext` D, `sketch` D, `songwriting-and-ai-music` C, `touchdesigner-mcp` D.
data-science (1) — `jupyter-live-kernel` D.
devops (2) — `kanban-orchestrator` D, `kanban-worker` D.
dogfood (1) — `dogfood` (QA web apps, bug reports) D.
email (1) — `himalaya` C.
github (6) — `codebase-inspection` D, `github-auth` D, `github-code-review` D, `github-issues` D, `github-pr-workflow` D, `github-repo-management` D.
media (4) — `gif-search` C, `heartmula` (Suno-like song gen) C, `songsee` (audio spectrograms) D, `youtube-content` C.
mlops (8) — `audiocraft-audio-generation` D, `huggingface-hub` D, `llama-cpp` D, `evaluating-llms-harness` D, `obliteratus` (abliterate refusals) D, `segment-anything-model` D, `serving-llms-vllm` D, `weights-and-biases` D.
note-taking (1) — `obsidian` C.
productivity (8) — `airtable` C, `google-workspace` (Gmail/Calendar/Drive/Docs/Sheets) C, `maps` C, `nano-pdf` C, `notion` C, `ocr-and-documents` C, `powerpoint` C, `teams-meeting-pipeline` C.
red-teaming (1) — `godmode` (LLM jailbreak: Parseltongue/GODMODE/ULTRAPLINIAN) D.
research (5) — `arxiv` C, `blogwatcher` C, `llm-wiki` D, `polymarket` C, `research-paper-writing` D.
smart-home (1) — `openhue` (Philips Hue) C.
social-media (1) — `xurl` (X/Twitter: post, search, DM, media) C.
software-development (10) — `hermes-agent-skill-authoring` D, `node-inspect-debugger` D, `plan` D, `python-debugpy` D, `requesting-code-review` D, `simplify-code` D, `spike` D, `systematic-debugging` D, `test-driven-development` D.
yuanbao (1) — `yuanbao` C.

(C = consumer-voice-usable; D = developer-only. Total 76.)

## PART C — DESKTOP-SPECIFIC SURFACES (fathah/hermes-desktop, unofficial)

- Major screens (CONFIRMED): Chat, Sessions, Agents, Skills, Models, Memory, Soul, Tools, Schedules, Gateway, Office (Claw3d), Settings — src [github.com/fathah/hermes-desktop].
- "Skills & Tools view" the user saw = the desktop **Skills** screen (browse/install/manage skills) + **Tools** screen (toolset config) + 14-toolset list. Desktop README groups built-ins as "14 toolsets": web, browser, terminal, file, code execution, vision, image generation, text-to-speech, skills, memory, session search, clarify, delegation, MoA/method-of-action. (Note: official registry is broader, ~29 toolset keys; the "14" is the desktop's curated grouping.)
- Kanban (CONFIRMED): "Claw3D HQ read-only board" + standard board; works in SSH tunnel mode. Backed by `kanban_*` tools + CLI `hermes kanban` / `/kanban`.
- Artifacts (CONFIRMED, the user's "Artifacts"): right-hand **preview rail** — "render web pages, files, and tool outputs side by side"; "Agent-generated media" rendering in chat. src [hermes-agent.nousresearch.com/docs/user-guide/desktop].
- 22 slash commands (CONFIRMED): `/new /clear /fast /web /image /browse /code /shell /usage /help /tools /skills /model /memory /persona /version /compact /compress /undo /retry /debug /status`.
- 16 messaging gateways (CONFIRMED) — see A1 Cross-Platform Messaging.
- Multi-provider (CONFIRMED): OpenRouter, Anthropic, OpenAI, Google Gemini, xAI Grok, Nous Portal, Qwen, MiniMax, Hugging Face, Groq + local OpenAI-compatible (LM Studio, Ollama, vLLM, llama.cpp, NVIDIA NIM, Atlas Cloud, etc.). In-app OAuth: ChatGPT Codex, xAI Grok, Qwen, Gemini CLI, MiniMax.
- Tool integrations exposed (CONFIRMED): Exa, Parallel, Tavily, Firecrawl, FAL.ai, Honcho, Browserbase, W&B, Tinker.
- Settings tree (CONFIRMED, partial): Providers/API keys, Models, Toolset config, MCP servers, Gateway, Sessions, Cron, Profiles, Messaging, Agents/Command Center, Privacy (analytics), Interface locale, per-session YOLO toggle, Voice mode (mic). "Office" = Claw3d visual interface.

## KEY CORRECTIONS / FLAGS FOR CURATION

1. "Hermes CLI" delegation skill does NOT exist — the 4th autonomous-agents skill is `hermes-agent` (self-config). User's label inaccurate.
2. "Context Engine" is NOT a tool — it's a single-select provider-plugin subsystem (config-time), distinct from the `memory` toolset and from Memory Providers.
3. "Claude Design" = creative HTML-artifact skill (`claude-design`), unrelated to any Anthropic product — likely to confuse end users.
4. Weather has NO official toolset or bundled skill (web-search-only / MCP).
5. macOS isn't a single toolset — it's `computer_use` + 5 Apple skills.
6. "Computer Use" (`computer_use`) is macOS-only and requires `cua-driver`.
7. Desktop's "14 toolsets" ≠ official ~29-key registry; the desktop presents a curated subset. ~71 tools total officially.

SOURCES: [tools-reference.md](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/reference/tools-reference.md) · [skills-catalog.md](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/reference/skills-catalog.md) · [docs/user-guide/desktop](https://hermes-agent.nousresearch.com/docs/user-guide/desktop) · [features/kanban](https://hermes-agent.nousresearch.com/docs/user-guide/features/kanban) · [features/tools](https://hermes-agent.nousresearch.com/docs/user-guide/features/tools) · [configuration.md](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/configuration.md) · [autonomous-ai-agents dir](https://github.com/NousResearch/hermes-agent/tree/main/skills/autonomous-ai-agents) · [hermes-agent.nousresearch.com](https://hermes-agent.nousresearch.com/) · [github.com/nousresearch/hermes-agent](https://github.com/nousresearch/hermes-agent) · [github.com/fathah/hermes-desktop](https://github.com/fathah/hermes-desktop) · [hermes-desktop/releases](https://github.com/fathah/hermes-desktop/releases)