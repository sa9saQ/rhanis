This confirms koe's thesis: calibrated confidence is active 2026 *research* (MIT RLCR, TrustBench, conformal/ACI) but the search surfaces **zero shipping consumer products** that disclose calibrated confidence in a voice agent. Transparency-as-tool-streaming exists (Hermes, Comet, Manus) but that's "show the tools," not "show calibrated confidence so you know when to intervene." I have sufficient depth across the full competitor set. Compiling the structured map.

---

# COMPETITIVE MAP — Computer-Operating / Task-Doing AI Agents 2026 vs koe

Scoring legend per axis: ✅ strong / ◐ partial / ❌ absent. CONFIRMED = sourced to product/news page; INFERRED = my synthesis. koe's 4 moat axes: **(1) Turnkey-for-non-engineers** (no keys/config) · **(2) Voice-FIRST** (not bolt-on) · **(3) Calibrated transparency / glass-box** · **(4) Always-on PC-operating secretary**.

## Headline finding (answers the founder's worry directly)

**Voice IS becoming table-stakes — CONFIRMED.** Of the 11 products mapped, **9 added or shipped voice** by mid-2026 (Hermes, Copilot, Spark, Comet, Codex, Claude mobile, Rabbit DLAM, OpenClaw, ClickUp). 87.5% of builders are actively building voice agents (AssemblyAI 2026 Voice Agent Report). **BUT** — and this is the decisive nuance — **every one of those 9 is voice-BOLT-ON, not voice-FIRST.** They are chat/email/IDE/browser products with a push-to-talk or voice-mode toggle added (Codex = "Hold Ctrl+M"; Comet = "Shift+Alt+V"; Claude = push-to-talk in the *mobile* app; Hermes = voice listed 5th among capabilities, absent from the desktop product page). **None is architected so that talking like a person is the primary driver.** koe's voice-FIRST framing is NOT eroded by "everyone can talk now" — the moat was never "can it talk," it's "is voice the spine." That spine is still empty.

**Calibrated transparency is still VACANT — CONFIRMED.** Multiple products ship *tool-activity streaming* ("show what tools ran" — Hermes streaming pane, Comet, Manus, Spark approvals). **Zero ship calibrated confidence disclosure** ("here's how sure I am, calibrated to real accuracy, so you know when to step in"). This is live 2026 *research only*: MIT CSAIL RLCR (Apr 2026, −90% calibration error), TrustBench, the trust-calibration design literature explicitly says "transparency ≠ trust… systems must help people recognize ambiguity and know when intervention is necessary" — i.e., the academy is describing koe's exact thesis as an *unsolved, unshipped* problem. koe's novelty research holds.

---

## Per-product scorecard

### 1. Nous Research — Hermes Agent / Hermes Desktop (v0.15.2, desktop shipped Jun 2, 2026)
- **Type:** Developer-oriented. CONFIRMED — product page foregrounds terminals, Python RPC, Docker, SSH, Singularity, Modal; MIT-license "audit, self-host, modify."
- **Voice:** Bolt-on. CONFIRMED — "voice input and output" exists but is **listed 5th** among capabilities (web search, browser automation, vision, image-gen, TTS, multi-model) and is **absent from the desktop landing page entirely**. The founder's trigger ("Hermes just added 音声会話") is real but it's a checkbox feature, not the product's center.
- **Turnkey:** ◐ — native installers reduce friction, but Nous Portal credits / "works with any provider" / model selection / Linux-via-terminal = technical literacy assumed.
- **Transparency:** ◐ tool-streaming only — "The window shows streaming responses and live tool activity," right-hand preview pane. No confidence/calibration. CONFIRMED.
- **Always-on:** ✅ persistent memory, cross-surface sessions, "grows with you," gateway daemon. CONFIRMED.
- **vs koe:** (1) ◐ (2) ❌ (3) ❌ (4) ✅. **Closest "always-on agent" peer but a developer tool with bolt-on voice.** Its voice headline is the founder's scare, but it does not contest koe's non-engineer + voice-first + glass-box position.

### 2. OpenAI — ChatGPT Agent (ex-Operator / CUA) + Codex App
- **Type:** Operator shut down Aug 2025, folded into ChatGPT Agent (consumer-ish, in ChatGPT) + Codex app (developer). CONFIRMED.
- **Voice:** Bolt-on. CONFIRMED — Codex app: "Hold Ctrl+M… your voice will be transcribed" (dictation, not conversation). ChatGPT has Advanced Voice but the *agent/computer-use* surface is text-driven.
- **Turnkey:** ChatGPT Agent ◐ (subscription, no keys) but it's cloud-browser, $200/mo Pro for heavy use; Codex ❌ (developer, IDE/CLI/worktrees).
- **Computer use:** ✅ Codex now operates Windows desktop apps (see/click/type, foreground); ChatGPT Agent operates a cloud browser. CONFIRMED. "Keeps asking for permission" (reviewer) = approval friction, not calibrated confidence.
- **Transparency:** ◐ shows steps; no calibration.
- **Always-on:** ❌ per-session/per-task.
- **vs koe:** Codex (1)❌ (2)❌ (3)❌ (4)❌ — pure dev tool. ChatGPT Agent (1)◐ (2)❌ (3)❌ (4)❌ — consumer but text-first, per-session, cloud-browser not local-PC-resident.

### 3. Anthropic — Claude Cowork + Dispatch + computer use (Windows since Feb 10, 2026)
- **Type:** Cowork = "Claude Code without the code," explicit **non-developer** push. CONFIRMED — this is the most direct "consumer agentic desktop" threat.
- **Voice:** Bolt-on, and only on **mobile** — "chat with voice mode (5 voices, push-to-talk and continuous listening)" in the phone app; the desktop computer-use surface is text. CONFIRMED. **Not voice-first.**
- **Turnkey:** ◐ — Pro/Max subscription, no API keys, but setup-aware (must keep desktop awake + app open; "Dispatch stops if the machine sleeps").
- **Computer use:** ✅ operates local files/apps/browser, "always request permission before accessing new apps." CONFIRMED.
- **Transparency:** ◐ permission prompts + activity; no calibrated confidence.
- **Always-on:** ◐ Dispatch = "stays active… continues working even when you're not looking," persists across sessions — **but tethered** (dies on sleep/close). CONFIRMED. Closest to "resident secretary" of the big labs.
- **vs koe:** (1)◐ (2)❌ (3)❌ (4)◐. **The single most dangerous competitor on axes 1+4** (non-engineer + persistent + local PC). koe's defensible gap vs Claude = **voice-FIRST (2) and calibrated glass-box (3)**, where Claude is fully absent. Watch this one.

### 4. Microsoft — Windows Copilot / Copilot Vision / Recall / Copilot Studio CUA
- **Type:** Consumer (Windows Copilot, Vision, Recall, taskbar "Ask Copilot" mid-2026) + enterprise (Copilot Studio CUA, Dynamics real-time voice). CONFIRMED.
- **Voice:** Bolt-on consumer-side; **real-time voice is enterprise-only** (Dynamics 365 Contact Center, GA North America). CONFIRMED. Consumer Copilot is text+optional voice, not voice-first.
- **Turnkey:** ✅ for consumers (built into Windows, no keys) — **strongest turnkey distribution of anyone** (OS-bundled).
- **Computer use:** ✅ Copilot Studio CUA "interact directly with websites and desktop applications through the UI" (GA); Vision = opt-in screen-see ("Share with Copilot"); Recall = passive screen snapshots.
- **Transparency:** ◐ Vision opt-in, Agent Dashboard + local activity logs (privacy transparency), no confidence calibration.
- **Always-on:** ◐ Recall is ambient/passive; agents are task-invoked. Build 2026 pivot = local agents on any hardware.
- **vs koe:** (1)✅ (2)❌ (3)❌ (4)◐. **Biggest distribution threat (it's in the OS) but text-first, enterprise-gated voice, no glass-box.** koe competes on voice-first + calibration + a focused secretary persona vs. Microsoft's sprawling toolbox.

### 5. Google — Gemini Spark (I/O 2026, May 19) + Project Mariner (KILLED May 4, 2026, folded into Gemini)
- **Type:** Consumer, $100/mo Google AI Ultra, US-only beta. CONFIRMED.
- **Voice:** Bolt-on — "supports voice commands… can turn spoken requests into tasks," but primary intake is **chat/email** (dedicated Gmail address). CONFIRMED. Not voice-first.
- **Turnkey:** ✅ no keys, Workspace context "without requiring manual setup."
- **Computer use:** ◐ **cloud-VM, NOT local PC** — "runs on dedicated VMs on Google Cloud… executes long-running tasks even when your device is off." Acts across Workspace/partner apps via API, not by driving *your* screen. Mariner's browser-control DNA absorbed but the consumer surface is cloud-API, not local-desktop operation.
- **Transparency:** ◐ "proactively sends critical updates and requires explicit approval for high-risk actions." Approval gate (parallels koe's 3-tier!) but no calibrated confidence.
- **Always-on:** ✅ genuinely 24/7, device-off. CONFIRMED — the truest "always-on" of all, but cloud not local.
- **vs koe:** (1)✅ (2)❌ (3)❌ (4)◐ (always-on ✅ but cloud, not local-PC-operating). **Strongest on always-on + turnkey; orthogonal on "operates YOUR PC" (it operates Google's cloud).** koe's local-desktop + voice-first + calibration remain uncontested.

### 6. Manus (Butterfly Effect; Meta acquisition BLOCKED by China Apr 27, 2026)
- **Type:** Prosumer/autonomous-task. Manus Desktop launched Mar 2026.
- **Voice:** ❌ none found — text/prompt-driven ("single high-level instruction"). CONFIRMED-absent.
- **Turnkey:** ◐ subscription, cloud sandbox, no keys, but task-prompt UX aimed at power users.
- **Computer use:** ◐ **cloud virtual computer** (browser/shell/code via executable Python), not your local PC.
- **Transparency:** ◐ shows execution steps; no calibration.
- **Always-on:** ❌ per-task long-running sessions.
- **vs koe:** (1)◐ (2)❌ (3)❌ (4)❌. Cloud autonomous worker, no voice — **not a koe competitor** (different shape: fire-and-forget task runner vs. conversational resident).

### 7. Perplexity — Comet (browser + assistant; iOS/Android/Win/Mac)
- **Type:** Consumer AI browser. CONFIRMED.
- **Voice:** Bolt-on — "voice mode (powered by GPT Realtime 1.5)," invoked via **Shift+Alt+V**. CONFIRMED. Hotkey-gated = not voice-first.
- **Turnkey:** ✅ free browser, no keys.
- **Computer use:** ◐ **browser-scoped** — clicks links, fills forms, books flights *within tabs*; not full-OS PC operation. Agent runs Claude Sonnet/Opus 4.6 under the hood.
- **Transparency:** ◐ shows browsing steps; no calibration.
- **Always-on:** ❌ per-session/per-tab.
- **vs koe:** (1)✅ (2)❌ (3)❌ (4)❌ (browser-bound, not PC-wide, not resident). Closest "consumer voice + agentic" but **confined to the browser** — koe's whole-PC operation + residency + voice-first differentiate.

### 8. Rabbit — DLAM (Desktop LAM) + OpenClaw integration (early 2026)
- **Type:** Consumer, "users of all technical levels." CONFIRMED — rhetorically the closest to koe's turnkey pitch.
- **Voice:** ◐ — "talk to the agent through the browser or type." Voice present, not exclusive.
- **Turnkey:** ◐ "no setup required… don't need to install any software… no complex configuration… no virtual machines" — **BUT requires the r1 hardware dongle plugged via USB** + visit dlam.rabbit.tech + grant screen-share. CONFIRMED. Turnkey-but-hardware-gated.
- **Computer use:** ✅ "r1 can see your screen and operate your computer for you" (OS/browser/apps) via cloud screen-share.
- **Transparency:** ◐ candid about limits ("will still make mistakes," "not fast enough yet"); no calibration.
- **Always-on:** ❌ per-session, USB-tethered.
- **vs koe:** (1)◐ hardware-gated (2)◐ (3)❌ (4)❌. **Most similar *positioning* (turnkey + talk + operate PC) but crippled by needing a $200 dongle, per-session, slow, cloud screen-share.** koe = software-only, no hardware, resident. Differentiated.

### 9. OpenClaw (open-source self-hosted gateway; Rabbit r1 alpha integration)
- **Type:** Developer / self-hoster. CONFIRMED — "self-hosted gateway… run a single Gateway process on your own machine," launchd/systemd daemon.
- **Voice:** ✅ genuinely voice-capable (speak/listen macOS/iOS/Android, Voice Wake + ElevenLabs, Opus-OGG for WhatsApp PTT) — **but bolted to chat-app channels** (WhatsApp etc.), not a voice-first desktop secretary.
- **Turnkey:** ❌ — self-host, configure gateway, bring your own AI agent/keys.
- **Computer use:** ✅ via connected coding agents (e.g., the Railway-build-fix-over-voice demo).
- **Always-on:** ✅ daemon "stays running."
- **vs koe:** (1)❌ (2)◐ (channel-bound) (3)❌ (4)✅. Powerful but **for tinkerers** — antithesis of koe's no-config consumer turnkey.

### 10. (Reference) emerging consumer voice-first desktop startups
- **Logical** — "desktop AI… proactive and largely promptless copilot, reduce friction for everyday tasks." INFERRED-relevant; closest *philosophy* to koe (proactive, low-friction) but no evidence of voice-first + calibrated confidence + 3-tier safety. Worth a dedicated watch-search.
- **Caddy** — "OS that works proactively for you… learns how you work, acts on your behalf… your voice stays intact" (voice = writing style, not speech). Proactive resident, not voice-conversational.
- **ClickUp Brain MAX** — "hands-free, voice-first AI assistant" desktop companion, but productivity-suite-bound (search + voice-to-text), not PC-operating agentic.
- **Caveat:** none surfaced with koe's exact combination. The voice-first + calibrated-glass-box + always-on-local-PC-operator + non-engineer-turnkey quadruple remains a **0-product cell**.

---

## Synthesis grid (koe's 4 axes)

| Product | 1. Turnkey non-eng | 2. Voice-FIRST | 3. Calibrated glass-box | 4. Always-on local-PC secretary |
|---|---|---|---|---|
| **koe** | ✅ (OAuth+credit, M4) | ✅ | ✅ (only one) | ✅ |
| Hermes Desktop | ◐ | ❌ bolt-on | ❌ | ✅ |
| ChatGPT Agent / Codex | ◐ / ❌ | ❌ | ❌ | ❌ |
| Claude Cowork/Dispatch | ◐ | ❌ (mobile only) | ❌ | ◐ (tethered) |
| MS Copilot (consumer) | ✅ (OS-bundled) | ❌ | ❌ | ◐ (Recall passive) |
| Gemini Spark | ✅ | ❌ | ❌ | ✅ but **cloud, not local** |
| Manus | ◐ | ❌ none | ❌ | ❌ (cloud task) |
| Perplexity Comet | ✅ | ❌ (hotkey) | ❌ | ❌ (browser-bound) |
| Rabbit DLAM | ◐ hardware-gated | ◐ | ❌ | ❌ |
| OpenClaw | ❌ self-host | ◐ channel-bound | ❌ | ✅ |

**Axis-by-axis verdict for synthesis:**
- **Axis 1 (turnkey):** Contested — Copilot (OS), Spark, Comet all turnkey. koe is NOT differentiated here at parity; it's table-stakes for consumer reach.
- **Axis 2 (voice-FIRST):** **Uncontested. 0 competitors are voice-first.** All 9 voice-having products are bolt-on. This is koe's strongest live moat *despite* "everyone can talk now."
- **Axis 3 (calibrated glass-box):** **Uncontested. 0 products, confirmed by 2026 research framing it as unsolved.** koe's novelty research validated.
- **Axis 4 (always-on local-PC secretary):** Partially contested — Hermes (dev), OpenClaw (dev), Claude Dispatch (tethered), Spark (cloud-not-local). **No consumer + local + always-on + PC-operating peer.** koe holds the specific cell.

**Bottom line for the founder:** Voice-as-capability is now table-stakes (your worry is factually correct), but **voice-as-spine is not** — and your two real moats (voice-FIRST architecture + calibrated transparency) sit in cells with literally zero shipping competitors. Erosion risk is concentrated in **Claude Cowork/Dispatch** (non-engineer + persistent + local PC; only missing voice-first + calibration) and **Microsoft Copilot** (OS distribution). The differentiation is intact but the window is the calibrated-glass-box thesis — ship it before the labs read the same MIT/TrustBench papers.

## Sources
- Hermes Desktop: [hermes-agent.nousresearch.com/desktop](https://hermes-agent.nousresearch.com/desktop) · [MarkTechPost](https://www.marktechpost.com/2026/06/03/nous-research-releases-hermes-desktop-a-native-cross-platform-front-end-for-hermes-agent-v0-15-2-with-streaming-tool-output/)
- OpenAI: [ChatGPT Agent](https://openai.com/index/introducing-chatgpt-agent/) · [Codex app computer use](https://developers.openai.com/codex/app/computer-use) · [Operator tracker](https://presenc.ai/research/openai-operator-update-tracker-2026)
- Anthropic: [Claude Cowork](https://www.anthropic.com/product/claude-cowork) · [CNBC computer use](https://www.cnbc.com/2026/03/24/anthropic-claude-ai-agent-use-computer-finish-tasks.html) · [Dispatch guide](https://pasqualepillitteri.it/en/news/418/claude-cowork-dispatch-guide)
- Microsoft: [Copilot Studio CUA + real-time voice](https://www.microsoft.com/en-us/microsoft-copilot/blog/copilot-studio/new-and-improved-computer-using-agents-a-new-workflows-experience-and-real-time-voice-experiences/) · [Build 2026 local agents](https://windowsnews.ai/article/microsoft-build-2026-windows-ai-shifts-from-copilot-pcs-to-local-agents-on-any-hardware.423563) · [Copilot Vision/Recall](https://www.tomsguide.com/ai/microsoft-is-hiding-windows-11s-eyes-heres-how-to-find-copilot-vision-and-fully-delete-it)
- Google: [Gemini Spark (DataCamp)](https://www.datacamp.com/blog/gemini-spark) · [TechCrunch Spark](https://techcrunch.com/2026/05/19/google-introduces-gemini-spark-a-24-7-agentic-assistant-with-gmail-integration/) · [Mariner shutdown](https://nerova.ai/news/google-shuts-down-project-mariner-gemini-agent-browser-2026)
- Manus: [Wikipedia](https://en.wikipedia.org/wiki/Manus_(AI_agent))
- Perplexity Comet: [perplexity.ai/comet](https://www.perplexity.ai/comet)
- Rabbit DLAM: [rabbit.tech blog](https://www.rabbit.tech/blog/first-major-update-of-2026-dlam-openclaw-and-a-surprise)
- OpenClaw: [github.com/openclaw/openclaw](https://github.com/openclaw/openclaw) · [docs.openclaw.ai](https://docs.openclaw.ai/)
- Voice table-stakes: [AssemblyAI Voice AI 2026](https://www.assemblyai.com/blog/voice-ai-in-2026-series-1)
- Calibration vacancy: [MIT RLCR](https://news.mit.edu/2026/teaching-ai-models-to-say-im-not-sure-0422) · [TrustBench](https://arxiv.org/pdf/2603.09157) · [Trust-calibration design](https://www.designative.info/2026/05/21/trust-calibration-in-agentic-ai-designing-for-appropriate-reliance-not-blind-trust/)