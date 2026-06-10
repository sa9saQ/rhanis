I have comprehensive, current data across all three questions. Synthesizing the raw findings now.

---

# COMPETITIVE INTELLIGENCE: koe — RAW FINDINGS (2026-06, cited)

**Moat-axis scoring legend** (per product): ✅ = meets, ⚠️ = partial/bolt-on, ❌ = absent, ? = unknown.
Axes: **(1) Turnkey** (no keys/config for non-engineers) · **(2) Voice-FIRST** (not a mic bolted onto chat) · **(3) Calibrated transparency / glass-box** · **(4) Always-on PC-operating secretary**.

---

## Q1 — DOES koe'S EXACT THING EXIST? CLOSEST COMPETITORS

**Verdict: No product hits all 4 axes. The single closest is Simular Sai. koe's exact combination — consumer + turnkey + voice-FIRST + calibrated glass-box + always-on PC operator — is NOT shipped by anyone. The vacancy is specifically axes (2)+(3) combined.** CONFIRMED.

### Simular Sai — CLOSEST DIRECT COMPETITOR (the one to watch)
- Always-on agentic AI "coworker" that uses the GUI like a human (apps, websites, desktop), runs on your own Mac/Windows/Mac mini (BYOD) **or** a provisioned remote VM. Plain-English tasks, "full transparency and built-in safety guardrails," "always double-checking with you before executing critical actions." Pricing $20/mo Plus, $500/mo Pro, 7-day trial. Backed by Agent S framework (first to beat humans on OSWorld benchmark). CONFIRMED.
- **Voice**: "Introducing voice commands for hands-free use" appears on the macOS page, but it is a **newly-added, secondary input** — the product is described/marketed task-text-first; I could not find a launch announcement framing it as voice-first. ⚠️ INFERRED (voice = bolt-on, recent).
- **Calibrated confidence**: ❌ — has approval gates ("double-checking before critical actions") but no calibrated confidence signal.
- Scores: **(1) ✅ turnkey · (2) ⚠️ voice bolt-on · (3) ❌ · (4) ✅**
- This is koe's most dangerous direct analog: same always-on + turnkey + real PC operation. koe's wedge against it = voice-FIRST + calibrated glass-box.
- Sources: https://www.simular.ai/sai · https://www.simular.ai/simular-for-macos · https://www.sai.work/

### Hermes Desktop (Nous Research) — the founder's "voice now table-stakes" example, contextualized
- Open-source autonomous agent, native desktop app (Mac/Win/Linux), public preview **June 2026**. Has voice mode (Whisper STT + spoken replies) across all surfaces (CLI, Discord, desktop). CONFIRMED.
- **But**: "shares the same agent core, **configuration, API keys**, sessions, skills, and memory as every other surface" → **BYOK / dev-oriented, NOT turnkey**. Voice is one I/O mode bolted across surfaces, not voice-first. No calibrated confidence.
- Scores: **(1) ❌ (API keys) · (2) ⚠️ bolt-on · (3) ❌ · (4) ⚠️ (agent, not always-on residency)**
- **Strategic read**: Hermes adding voice ≠ Hermes becoming koe. It validates the founder's worry that *voice is becoming table-stakes*, but Hermes is BYOK + developer-flavored, missing axes (1)(2)(3). It does NOT erode koe's actual differentiation.
- Sources: https://www.everydev.ai/tools/hermes-desktop-nous-research · https://hermes-agent.nousresearch.com/docs/user-guide/features/voice-mode

### Braina — voice-first PC operator, but legacy/macro-based
- 10+ yr Windows voice assistant: voice control, dictation, custom voice commands to open files/programs/websites, keyboard/mouse macros, runs local LLMs. Voice-FIRST and turnkey-ish. CONFIRMED.
- **But**: command/macro paradigm, not conversational LLM-agentic computer-use; no calibrated confidence; not "talk like a person" continuous secretary.
- Scores: **(1) ✅ · (2) ✅ · (3) ❌ · (4) ⚠️ (command-driven, not agentic always-on)**
- Closest on **voice-first + turnkey** but weakest on the agentic/transparency thesis. Proves voice-first PC control alone is a commodity — koe's defensibility must rest on glass-box + modern agentic depth.
- Sources: https://www.brainasoft.com/braina/ · https://en.wikipedia.org/wiki/Braina

### Pika Voice Assistant — "Jarvis-style," consumer, free tier
- "Hey Pika" wake word, opens/runs apps, system commands (shutdown/lock), free version. Voice-first, turnkey, consumer. CONFIRMED.
- **But**: command-execution tool, no agentic web/reasoning depth, no transparency/confidence layer.
- Scores: **(1) ✅ · (2) ✅ · (3) ❌ · (4) ⚠️**
- Same lesson as Braina: voice-first PC control is already a crowded commodity layer; the moat is NOT "voice that controls the PC."
- Sources: https://pikaai.vercel.app/ · https://pikavoice.com/blog/ai_assistant_for_pc/

### Others scanned (lower threat): QwenPaw (self-host personal assistant, dev-flavored, BYOK), ClickUp Brain MAX (voice-to-text companion, not a PC operator), Speechify Voice AI Assistant (web-answer/dictation, not PC agent), macos26/Agent (open-source Mac harness, BYOK 18+ providers, dev tool).

**Pattern across Q1**: The market splits into (A) **turnkey voice-first PC *command* tools** (Braina, Pika) with no agentic depth or transparency, and (B) **agentic always-on computer-use coworkers** (Sai, Hermes) that are text-first and/or BYOK. **No one occupies koe's intersection, and no one in either camp does calibrated transparency.** koe's defensible wedge = (2)+(3) together, layered on (1)+(4).

---

## Q2 — BIG-PLAYER THREAT (plan risk R6)

**Verdict: The platform threat is REAL and intensifying on axes (1)(2)(4), but NONE of the big four is shipping calibrated glass-box transparency (3), and OpenAI has actively RETREATED from desktop voice. The squeeze is on koe's "always-on voice PC agent" framing, not on its transparency thesis.**

### Microsoft — THE BIGGEST THREAT (direct, on-platform, consumer, turnkey)
- Windows 11 explicitly rebranded "the computer you can talk to." Ships: **"Hey Copilot"** wake word (always-listening, hands-free) → launches Copilot Vision (sees your screen); **Copilot Actions** = "first general-purpose agentic AI experience on Windows" that controls apps/files and completes tasks "right in front of your eyes or in the background"; **Agent Workspace** = contained runtime + per-agent accounts for UI-level actions (opt-in); powered by GPT-5.2; also opens the platform to **third-party agents**. CONFIRMED.
- Scores: **(1) ✅ turnkey (built into OS) · (2) ✅ voice-first ("talk to") · (3) ❌ (opt-in approval, not calibrated confidence) · (4) ✅ always-on PC operator**
- **This is the existential platform risk**: Microsoft is shipping koe's axes (1)(2)(4) natively in Windows, free, for every PC. koe **cannot** out-distribute this. koe's ONLY durable wedge vs Copilot = **axis (3) calibrated glass-box** + cross-provider/BYOK neutrality + "your secretary, not Microsoft's." If koe's transparency thesis is weak, Copilot subsumes it.
- Sources: https://www.windowscentral.com/microsoft/windows-11/microsoft-dubs-windows-11-pcs-the-computer-you-can-talk-to... · https://venturebeat.com/ai/microsoft-launches-hey-copilot-voice-assistant-and-autonomous-agents-for-all · https://windowsforum.com/threads/windows-11-agent-workspace-and-copilot-actions-preview-explained.397580/

### Apple — adjacent threat, OS-level, but not a full PC operator yet
- WWDC 2026: "Siri AI," Gemini-powered (~1.2T-param custom Gemini, ~$1B/yr to Google) + on-device models. On-screen awareness, personal-context access (email/messages/files/photos), cross-app actions, "Call them"/"How far is this" contextual deixis. iOS 27. CONFIRMED.
- Scores: **(1) ✅ · (2) ✅ voice-first · (3) ❌ · (4) ⚠️ (cross-app actions + on-screen awareness, but iPhone-centric; not yet a Mac "operate everything + run commands" agent)**
- Threat is mostly mobile/iPhone-first; Mac PC-operation depth (run commands, file ops, web agent) unproven. Still, raises the consumer baseline for "voice assistant that acts."
- Sources: https://appleinsider.com/articles/26/04/22/google-confirms-context-aware-siri-built-from-gemini-will-debut-in-2026 · https://www.business-standard.com/.../wwdc-2026-apple-unveils-siri-ai...

### Google — agentic + voice, but cloud/browser-centric, not desktop-resident
- Gemini Agent (built on Project Mariner, Gemini 3): decomposes tasks via Deep Research, Canvas, Workspace (Gmail/Calendar), live web browsing; "seeks confirmation before critical actions like purchases/messages, take over anytime" (= approval gate). Gemini 3.1 Flash Live = real-time voice + screen-share. CONFIRMED.
- Scores: **(1) ✅ · (2) ⚠️ (voice is a Live mode, not the primary agent surface) · (3) ❌ (confirmation gate, not calibrated confidence) · (4) ⚠️ (browser/cloud agent, not always-on local PC operator)**
- Plus the Apple-Siri deal puts Gemini *inside* the consumer voice layer regardless.
- Sources: https://blog.google/products-and-platforms/products/gemini/gemini-3-gemini-app/ · https://blog.google/innovation-and-ai/technology/developers-tools/build-with-gemini-3-1-flash-live/

### OpenAI — RETREATING from desktop voice (R6 RELIEF, partial)
- **Retiring ChatGPT native Voice on macOS by January 2026**; consolidating voice to **mobile**; stated direction: "the next-gen AI assistant is an **ambient, mobile-native entity, not a PC utility**." Building toward an **audio-first hardware device** (~1yr out), smart glasses/pins/recorders 2026–27. ChatGPT Pulse = proactive/ambient briefings (mobile, Pro). ChatGPT Agent absorbed Operator (computer-use), but that's cloud-browser, Plus/Pro tiers. CONFIRMED.
- Scores: **(1) ✅ · (2) ✅ but mobile/hardware, NOT desktop · (3) ❌ · (4) ❌ on desktop (deprioritized)**
- **R6 read**: OpenAI is the LEAST likely of the four to ship a turnkey always-on *desktop PC-operating* voice agent near-term — they've explicitly vacated desktop voice for mobile + ambient hardware. This narrows the "big-player subsumes koe on the desktop" risk to **Microsoft (primary) and Apple (Mac, later)**.
- Sources: https://i10x.ai/news/openai-sunsets-chatgpt-voice-macos-advanced-voice-mode · https://techcrunch.com/2026/01/01/openai-bets-big-on-audio-as-silicon-valley-declares-war-on-screens/ · https://openai.com/index/introducing-chatgpt-pulse/

**R6 net**: Voice + agentic + always-on + turnkey is going **table-stakes at the OS level** (Microsoft now, Apple Mac later). The founder's worry is correct on axes (1)(2)(4). **But all four big players use simple approval/confirmation gates, NOT calibrated confidence disclosure.** Axis (3) remains uncontested even by the giants.

---

## Q3 — IS THE "CALIBRATED CONFIDENCE / GLASS-BOX" MOAT REALLY VACANT? (falsification attempt)

**Verdict: The "0 products" claim SURVIVES for koe's niche, but with a nuance the founder must own: calibrated/tiered confidence IS shipping — exclusively in ENTERPRISE CUSTOMER-SUPPORT voice agents, never end-user-facing, never in a consumer PC assistant. The vacancy is real for "consumer + end-user-visible + voice-PC-secretary." It is NOT a vacancy for "calibrated confidence exists anywhere."** CONFIRMED with caveat.

### Closest falsifiers found (all enterprise CX, none consumer):
- **Maven AGI "Thinks Out Loud"** — tiered confidence (high → answer / medium → caveat+escalate / low → human) driving response behavior. **BUT Maven explicitly states: "Generally no — raw scores aren't meaningful to customers"** — the confidence signal is **internal-facing to support teams, NOT shown to end users.** Enterprise CX, not consumer, not a PC operator. → This is the strongest near-match and it confirms koe's gap: *nobody surfaces calibrated confidence TO the end user.* Source: https://www.mavenagi.com/glossary/ai-confidence-score
- **PolyAI Agent Studio** (enterprise voice transparency/governance), **Retell Assure** (monitors 100% of calls, scores failures) — both observability/ops tooling for enterprises, not user-facing confidence disclosure. Source: telnyx/assemblyai roundups above.
- **"Confidence UI" / "Confidence Visualization Patterns (CVP)"** — a **2026 design-pattern movement** (Medium essays, agentic-design.ai). Crucially, when probed for **named shipped products implementing it**, the pattern catalog cites **only guidelines (Google PAIR, MS, IBM, Apple) and libraries — ZERO shipped products.** → The concept is in the air, but **no product has shipped it**, which both (a) confirms the vacancy and (b) warns koe the idea is becoming "obvious" and will attract entrants. Sources: https://agentic-design.ai/patterns/ui-ux-patterns/confidence-visualization-patterns · https://medium.com/@Modexa/the-confidence-ui-pattern-that-users-actually-trust-...

### Distinction koe should hold:
- **Approval/confirmation gates** (Sai "double-checks," Copilot opt-in, Gemini "seeks confirmation") are **everywhere** — this is NOT the same as calibrated confidence and koe should stop treating its approval gate as a differentiator (it's table-stakes; see Q2).
- **Calibrated confidence shown to the END USER in real time, in a consumer voice PC agent** = found in **0 products**. The claim holds for koe's exact framing.

---

## SYNTHESIS FOR THE FOUNDER (raw, for your own judgment)

1. **Voice IS going table-stakes** (Hermes, Sai, Copilot, Siri all added it). The founder's instinct is right. **Axes (1)(2)(4) are NOT a defensible moat by mid-2026** — Microsoft ships them free in Windows. Stop leaning on "voice + operates PC + always-on" as the pitch.
2. **The single surviving differentiator is axis (3) calibrated glass-box transparency, shown to the end user.** It is genuinely vacant in koe's niche — even the giants only do approval gates, and the only calibrated-confidence implementations (Maven et al.) are enterprise CX and explicitly *hidden from end users*. koe's novelty-research "0 products" claim is **validated** for its niche.
3. **Caveat / erosion risk**: The "Confidence UI" design pattern is trending in 2026 with zero shipped products — meaning koe has a **first-mover window that is closing**. Someone will ship it. koe's edge must be *execution* of calibrated (accuracy-matched, not raw-%) confidence + the Calibration Memory loop, not the mere idea.
4. **Primary platform threat = Microsoft Copilot Actions** (same axes 1/2/4, OS-native, free). koe's only viable positioning vs Copilot: provider-neutral + privacy-local + **calibrated transparency Microsoft won't ship** (giants have no incentive to surface "how unsure am I" to consumers — it undercuts the magic-assistant UX). **R6 partial relief**: OpenAI has *exited* desktop voice for mobile/hardware, so the desktop-voice big-player threat is narrower than feared — concentrated in Microsoft (now) and Apple-on-Mac (later).
5. **Closest startup to displace koe = Simular Sai.** If Sai makes voice first-class AND adds user-facing confidence, it becomes a true head-to-head. Watch it.