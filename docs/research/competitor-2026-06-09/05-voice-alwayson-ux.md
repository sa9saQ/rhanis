I have comprehensive coverage across all six dimensions. Compiling the research data now.

---

# RESEARCH DATA: Voice-and-Always-On UX Dimensions (for koe)

Framing legend: **[CONFIRMED]** = stated in a fetched/searched primary source (URL cited). **[INFERRED]** = my synthesis/extrapolation for koe, not directly in a source. **[koe-IMPLICATION]** = direct design consequence for koe's orb / glass-box thesis.

Critical context for downstream synthesis: koe is **VOICE-FIRST + ALWAYS-ON + glass-box-transparency**. Codex App / Hermes Desktop are **TEXT/CHAT-first developer agents**. The dimensions below are where text-first apps structurally cannot lead, because they have no continuous-audio surface, no turn-taking, no recording-consent burden, and no "speak vs stay silent" decision.

---

## 1. VOICE INTERACTION UX (activation, barge-in, state, turn-taking, noise)

### 1.1 Activation model: push-to-talk vs continuous/open-mic vs wake-word

- **[CONFIRMED]** Three models with distinct tradeoffs. Push-to-talk = simplest impl (physical/graphical button), keeps user "in control of when transcription happens." Wake word = hands-free but needs local detection. Continuous wake-word listening "does not incur per-utterance cloud compute fees." (https://sensory.com/custom-wake-words-branded-voice-ux-guide-2026/, https://picovoice.ai/blog/complete-guide-to-wake-word/)
- **[CONFIRMED]** Dictation tools (Wispr Flow, Superwhisper) standardized on **hold-to-talk** ("press hotkey, speak, release") precisely because it gives explicit user control over when audio is captured — a deliberate retreat from open-mic. (https://www.getvoibe.com/resources/wispr-flow-vs-superwhisper/)
- **[CONFIRMED]** Gemini "Continued Conversation" keeps mic active "for a few seconds after replies with listening cues" so the user need not repeat the wake word — a hybrid: wake-word to enter, open-mic window inside the turn. (https://letsdatascience.com/news/gemini-for-home-enables-seamless-continued-conversations-5f88b640)
- **[koe-IMPLICATION]** koe is "always-on / talk continuously like to a person," which is the **open-mic** end. But open-mic is exactly where false-activation lawsuits live (§4). The discerning synthesis: koe should offer a **three-position mic control** — (a) full open-mic continuous, (b) wake-word-gated continuous (default for always-on), (c) push-to-talk / hold-hotkey for sensitive contexts (meetings, shared space). The orb's center is the natural home for this state; tray gives the global toggle. Codex/Hermes have none of this because they're typed.

### 1.2 Barge-in / interruption handling (hard numbers)

- **[CONFIRMED]** Barge-in is "the failure mode users notice" — "users will remember that the agent kept talking over them" even if content is good. Target **false-barge-in rate <2%** (above 5% "feels broken"); **barge-in success rate >96%**. (https://futureagi.com/blog/voice-ai-barge-in-turn-taking-2026/)
- **[CONFIRMED]** Five components of barge-in: (1) VAD continuously scores incoming audio *while the agent is speaking*; (2) barge-in trigger fires when VAD confidence sustains above threshold; (3) **TTS flush <60ms** (10ms dispatch + 20ms buffer + 20ms WebSocket + 10ms device release); (4) **LLM cancel <40ms** via AbortController; (5) context preservation (record interrupted utterance + tool state). **Total barge-in handle target <150ms.** (https://futureagi.com/blog/voice-ai-barge-in-turn-taking-2026/)
- **[CONFIRMED]** Mid-tool interruption is nuanced: "let it finish in background" for idempotent reads, but cancel mutations "if the new intent contradicts the in-flight tool." (https://futureagi.com/blog/voice-ai-barge-in-turn-taking-2026/)
- **[CONFIRMED]** Keep "turn detection layer active even while the agent is playing back audio"; requires client-side echo cancellation. Echo cancellation "has to be handled on the client side where the speaker is"; devices without built-in AEC "need a fallback like push-to-talk." (https://livekit.com/blog/turn-detection-voice-agents-vad-endpointing-model-based-detection, https://futureagi.com/blog/voice-ai-barge-in-turn-taking-2026/)
- **[CONFIRMED]** ChatGPT Advanced Voice supports barge-in via native speech-to-speech model, interruptible mid-sentence; design rule: "Support barge-in. If the user talks, pause narration and re-route." Gemini Live also supports barge-in to "cut off the assistant... and issue a new prompt." (https://completeaitraining.com/news/chatgpt-voice-breaks-free-no-mode-switching-live/, https://www.gend.co/blog/enhanced-gemini-models-boost-voice-interactions)
- **[koe-IMPLICATION]** koe's Rust audio_bridge + WebSocket session_manager must keep VAD live during rodio playback and abort the OpenAI `response` on user speech. The <150ms total / <60ms TTS-flush / <40ms cancel budgets are concrete acceptance criteria for an E2E test. **Glass-box twist:** when the human barges in *to correct* a low-confidence action, that is the exact moment the transparency thesis pays off — koe should treat "barge-in during a CAUTION/DANGER disclosure" as a first-class intervention signal, not just a TTS cancel.

### 1.3 Turn-taking gaps and "listening vs processing vs speaking"

- **[CONFIRMED]** Humans take turns at **200–300ms gaps**; most voice agents lag at **800–1500ms** because they wait for VAD silence. Semantic turn detection (VAD + lightweight text classifier on partial transcript) closes the gap to **~300ms** without cutting users off; incomplete utterances get a longer wait, complete phrases cut to **200ms**. (https://altersquare.medium.com/why-vad-end-of-speech-detection-is-the-hardest-problem-in-production-voice-agents-fee308e38cfc, search synthesis)
- **[CONFIRMED]** A naive **800ms silence timeout** "adds nearly a full second to every single response before the pipeline even starts." (https://livekit.com/blog/turn-detection-voice-agents-vad-endpointing-model-based-detection)
- **[CONFIRMED]** Recommended **turn-taking gap P95 by use case**: sales 250–350ms; support 350–450ms; clinical/financial 500–700ms (slower because stakes are higher, paired with a "visible thinking signal" at 500–800ms gaps). (https://futureagi.com/blog/voice-ai-barge-in-turn-taking-2026/)
- **[CONFIRMED]** State signaling via earcons: "subtle ping when the system starts listening, a different sound when it begins processing, a chime when a task completes, and a discordant tone when something fails." Critical threshold: **silence past 3 seconds and the user assumes the system crashed.** (https://fuselabcreative.com/voice-user-interface-design-guide-2026/)
- **[CONFIRMED]** Predictive turn-taking is multimodal: listeners plan their next turn while the speaker is still speaking, supported by "syntax, prosody and gaze." Visual cues enhance predictive turn-taking. (https://arxiv.org/pdf/2505.21043)
- **[CONFIRMED]** ChatGPT's animated orb "pulses while the model listens and responds" — a single visual element doubles for listening and speaking. (search synthesis; https://medium.com/@vuongngo/exploring-chatgpts-advanced-voice-mode-transforming-ai-interaction-538189e6fa3d)
- **[koe-IMPLICATION]** koe's secretary is closer to the **clinical/financial 500–800ms band** because it does *consequential PC operations*, not chit-chat — and the glass-box thesis *requires* a visible thinking window during that gap. So the "slow" gap that hurts a sales bot is **on-thesis for koe**: the 300–700ms "thinking window" the central thesis already names is the perceptual cover for koe doing tool-routing + confidence-calibration. **The orb must encode 4 distinct states, not 2**: idle/ambient, listening, thinking/processing (this is where the thinking-window text streams), speaking. A 2-state pulse (ChatGPT) is insufficient for a transparency product. Color-independent encoding required (§5).

### 1.4 Background noise & accidental activation

- **[CONFIRMED]** False-barge-in prevention uses 3 signals: energy threshold (**-45 to -35 dBFS**), voice classifier (Silero VAD target **false-positive rate ~5–8%**), and a **minimum-duration guard (200–300ms sustained voice)** that "cuts false-barge-in by 60%." WebRTC VAD "degrades sharply in real noise"; Silero/neural perform "significantly better." (https://futureagi.com/blog/voice-ai-barge-in-turn-taking-2026/, https://livekit.com/blog/...)
- **[CONFIRMED]** Documented false-trigger sources: background noise, side conversations, codec artifacts, and "echo wake" (assistant re-triggers on its own response). Mic arrays (3–4 mics) + front-facing speech improve SNR up to 14 dB. (https://futureagi.com/blog/..., https://www.kunalganglani.com/blog/self-hosted-voice-assistant-home-assistant-2026-guide)
- **[CONFIRMED]** Wake-word phonetic design reduces false accepts: favor plosives/affricates and distinct vowels/diphthongs for "sharp formant shifts"; test that domain co-occurring words don't trigger. (https://sensory.com/custom-wake-words-branded-voice-ux-guide-2026/)
- **[koe-IMPLICATION]** A desktop always-on app sitting next to speakers/video has a severe **echo-wake** risk. koe needs AEC (Granola/LiveKit pattern) or a self-speech gate so its own rodio output never counts as a user turn or barge-in. The minimum-duration guard (200–300ms) is cheap and high-value; koe should ship it. Accidental DANGER-tool triggering from misheard speech is the catastrophic version — the 3-tier gate (§ product) is the mitigation, but the *detection* side (don't act on a misfire) needs VAD + semantic-completion gating before any tool dispatch.

### 1.5 Self-hosted voice (koe's Qwen3.5-Omni path) and the transparency edge

- **[CONFIRMED]** 2026 voice stacks migrate to "dedicated turn-taking models that classify backchannel vs. barge-in vs. continued silence as a learned signal" (Pipecat SmartTurnAnalyzer, LiveKit TurnDetector, Vapi endpointing). (https://futureagi.com/blog/...)
- **[koe-IMPLICATION / from koe memory]** koe's own research already concluded Qwen3.5-Omni (semantic interruption / barge-in native) for self-hosted. Self-hosted is *also* where koe can read hidden-state confidence signals (SEP) that a BYOK API can't expose — i.e. self-hosting is not just cost, it's a **transparency-thesis enabler**. This is a moat Codex/Hermes (text agents) have no reason to build.

---

## 2. ALWAYS-ON RESIDENCY (tray, hotkey, DND, timeout, minimized, battery, notifications)

### 2.1 Tray / global hotkey baseline

- **[CONFIRMED]** Standard 2026 desktop-assistant traits: "launch instantly, respond to system-level prompts, support voice or typed input." Examples: ChatGPT Desktop appears in system tray + configurable global shortcut; Flowly summons "from the menubar, or through a notch overlay — one global hotkey away" on Mac/Win/Linux and supports **persistent sessions** ("long-running agent tasks can be resumed after walking away"); Copilot guidance: "system tray presence, a global hotkey, or OS-level permissions." (https://www.producthunt.com/products/flowly-6, https://www.revoyant.com/blog/best-ai-assistants-for-windows-in-2026, https://www.microsoft.com/.../for-individuals/)
- **[koe-IMPLICATION]** koe's tall narrow orb window (~440×680) is the *foreground* surface; the **tray icon is the always-on anchor** (it persists when the orb window is closed/minimized). Global hotkey should do two things: (a) summon/dismiss the orb window, (b) **instant mic mute toggle** (privacy panic button). This maps to koe's planned "tray/always-on residency" product-layer gap.

### 2.2 Do-not-disturb / pause / session timeout

- **[CONFIRMED]** koe's plan already sets a **30-minute session timeout** as a cost-protection backstop. (koe CLAUDE.md / cost_tracker context) — this is consistent with the industry pattern of bounding always-on sessions.
- **[INFERRED]** DND for an always-on *voice* app has two axes text apps lack: (1) **mic DND** (stop listening) and (2) **output DND** (stop speaking — e.g. user is on a call, koe should go silent but may keep working). These are independent and both need one-tap toggles. No source treats them separately; this is a koe-specific design opportunity.
- **[koe-IMPLICATION]** "What happens when minimized" must be explicit: koe keeps the WebSocket + cost gate alive across minimize and across auto-reconnect (already built: reconnect preserves cost/budget). The orb-closed state should still show *something* in the tray (listening / muted / working / needs-approval) — a 4-state tray icon mirroring the orb.

### 2.3 Battery / CPU for continuous audio

- **[CONFIRMED]** Optimized low-power wake-word front-ends draw "as low as about 1 mA." (https://sensory.com/...)
- **[CONFIRMED]** Always-on audio processing has measurable cost: idle audio CPU "should be below 2%, memory under 50 MB"; "audio processing has a really negative effect on battery given the high CPU usage"; enhancements/effects are the main culprit. Wispr Flow's cloud dictation is "resource-heavy at ~800MB RAM, ~8% CPU." (search synthesis: OBS/Dell/MS forums; https://www.getvoibe.com/...)
- **[koe-IMPLICATION]** koe should **gate the heavy pipeline behind a cheap local VAD/wake stage** so that when no one is talking, only a ~1mA-class detector runs, not the full Realtime WebSocket stream. This is the difference between a laptop-killer and a residency-viable app. Streaming continuous audio to OpenAI Realtime 24/7 is both a cost-tracker nightmare and a battery one — the cost gate and the battery gate are the *same* gate (only open the expensive stream when VAD says someone is addressing koe).

### 2.4 Notifications when the AI needs attention (approval / done / error)

- **[CONFIRMED]** Agentic HITL pattern: notify an approver out-of-band (SNS/Slack with approve/reject buttons) and let the session continue *without blocking* while approval proceeds. High-confidence auto-sends; low-confidence escalates. (https://aws.amazon.com/blogs/machine-learning/human-in-the-loop-constructs-..., https://medium.com/@AlignX_AI/...)
- **[CONFIRMED]** Named UI patterns: Agent Status & Activity UI (ASP) for "real-time agent activities, thinking states, operational status"; Confidence Visualization UI Patterns (CVP) for "displaying AI confidence levels, uncertainty, prediction reliability in user-friendly formats"; Progressive Disclosure UI Patterns (PDP) to "prevent cognitive overload"; Human-on-the-Loop (HOTL) for supervisory oversight + intervene/take control. (https://agentic-design.ai/patterns/ui-ux-patterns)
- **[CONFIRMED]** Regulatory pressure: EU AI Act Art. 14 (effective Aug 2, 2026) mandates high-risk systems have "human-machine interface tools enabling effective oversight"; California SB-833 adds state requirements by July 1, 2026. (search synthesis)
- **[koe-IMPLICATION]** koe's DANGER approval modal (30s, fail-closed) is the *foreground* case. But koe is always-on, so the user may be **looking away**. koe needs OS-level notifications (Windows toast / mac notification) for: (a) approval-needed (with the 30s timeout — and the fail-closed default means timeout = deny, which must be visible), (b) task-done, (c) error/budget-hit. The *voice channel* is koe's unique escalation path text apps don't have — koe can **speak** "I need your OK to delete this file" — but see §3 for when speaking is annoying vs. right. The CVP/ASP/PDP/HOTL pattern vocabulary maps almost 1:1 onto koe's thinking-window + approval-gate, which is a validation that koe's architecture is the *named best-practice shape* for agentic oversight — koe just renders it in voice+orb instead of a dev dashboard.

---

## 3. TRANSPARENCY-DURING-VOICE (disclose tool/confidence/thinking via VOICE without annoyance)

### 3.1 Verbal confidence calibration — the core risk

- **[CONFIRMED]** LLMs are "systematically overconfident across models, domains, and elicitation strategies" and "adopt an assertive language style also when making false claims," creating "overconfident hallucinations" that "mislead users and erode trust." (https://www.researchgate.net/publication/388821876_..., https://arxiv.org/abs/2306.13063)
- **[CONFIRMED, CRITICAL FOR koe]** User-trust is **non-monotonic** in expressed uncertainty: "When participants perceived LLMs as speaking with 100% confidence despite known limitations, excessive certainty undermined trust. Interestingly, in groups where high verbalized uncertainty was used, participants often distrusted the LLM due to its hesitant language" — overt verbal uncertainty "led participants to question the LLM's reliability." I.e. *both* over-confidence and over-hedging destroy trust. (https://www.researchgate.net/publication/388821876_Confronting_verbalized_uncertainty_...)
- **[CONFIRMED]** LLMs "may encode verbal uncertainty with confidence levels that differ substantially from those of humans" — the mapping from words ("likely", "possible") to actual probability is mis-calibrated vs. human expectation. (https://openreview.net/forum?id=uZ2A0k5liR)
- **[CONFIRMED]** Calibration is fixable in training (calibration-aware fine-tuning / ConfTuner — "training LLMs to express their confidence verbally"); the alignment-vs-calibration tradeoff is "an artifact of current training procedures," not fundamental. (https://arxiv.org/pdf/2508.18847, ICML 2025 CFT)
- **[koe-IMPLICATION — this is the thesis's sharpest edge]** koe's central thesis is **calibrated** transparency with **3–4 natural-language tiers, NOT raw %**. The research *validates this exact choice*: raw % (100% confident) erodes trust, and naive hedging ("I'm not sure...") *also* erodes trust. The winning path is a **small set of calibrated tiers whose words are tuned to real accuracy** — which is precisely koe's plan (calibration-memory layers, conformal/ACI). koe must NOT speak raw probabilities and must NOT free-form hedge. It should speak from a fixed, accuracy-calibrated vocabulary. koe's own E2 experiment already found "raw confidence direct-output scored below the work-log baseline (6.5% < 7.1%)" — this is the same finding. **Codex/Hermes show confidence (if at all) as text labels in a transcript; koe must make it a calibrated spoken+visual signal — and the literature says getting the *words* right is the whole game.**

### 3.2 When to speak vs. stay silent (avoid annoyance)

- **[CONFIRMED]** Earcons/non-verbal cues > verbal for low-stakes state changes (ping=listening, chime=done, discordant=fail) — i.e. *don't narrate everything in words*. (https://fuselabcreative.com/...)
- **[CONFIRMED]** Visuals should lead voice for detail: "Start visual updates before the assistant finishes speaking. Show partials fast"; "let users tap any transcript line to jump to the related visual state." The transcript is "part of comprehension, audit, and accessibility." (https://completeaitraining.com/news/chatgpt-voice-breaks-free-...)
- **[CONFIRMED]** Contextual re-prompting on failure: "don't ask them to repeat the whole sentence. Ask for the specific missing piece." After two no-matches, "the third step must be an escape or a solution." (https://fuselabcreative.com/...)
- **[INFERRED / koe-IMPLICATION]** koe's disclosure has 3 payloads (thinking 1-liner, tool+source, confidence tier). Annoyance avoidance rule of thumb: **route by stakes**. SAFE tools → *silent* screen disclosure only (orb thinking-window text + earcon), don't speak it. CAUTION → brief spoken confidence tier + screen detail ("Opening the URL — pretty sure that's the right one"). DANGER → *must* speak (this is the approval ask) + modal. Confidence is spoken only when it changes the human's decision; otherwise it's screen-only. This makes the voice channel *sparse and meaningful* — the opposite of a chatbot reading its whole chain-of-thought aloud. **This silence-discipline is impossible to even pose as a question in a text app**; it is koe-native.

### 3.3 No raw chain-of-thought aloud

- **[CONFIRMED, from koe research/memory]** Glass-box discloses *verifiable actions* (which tool, which source) + *calibrated confidence*, not raw CoT. koe's E2 found raw-confidence direct output underperforms; raw CoT narration would be worse (cognitive overload, PDP violation). (koe plan §中心思想; https://agentic-design.ai/... PDP pattern)

---

## 4. PERMISSION & CONSENT for an always-listening recorder

### 4.1 Recording-consent law (the always-on landmine)

- **[CONFIRMED]** 11 strict all-party (two-party) consent states (CA, DE, FL, IL, MD, MA, MT, NV, NH, PA, WA). One-party consent "fails to protect always-on wearables" because federal law requires the recorder to be an *active participant*; **passive ambient capture of others' conversations when you're not present is a federal felony — up to 5 years + $250,000 fine.** (https://keku.com/blog/call-recording-laws-by-state, https://consultantlm.com/...)
- **[CONFIRMED]** Aug 2025 *Brewer v. Otter.ai*: a call was allegedly recorded without one participant's consent because *another attendee* ran Otter; challenged the design where "only the meeting host is asked for permission." Crystallized that "an always-joining bot that records by default is a liability." (https://www.granola.ai/blog/ai-notetaker-participant-privacy-consent)
- **[CONFIRMED]** Aug 2025–Feb 2026: class actions vs Otter.ai, Fireflies.ai, Microsoft Teams under **BIPA** for "extracting voiceprints without written consent" — "even if you delete the audio... the biometric processing has already occurred." (https://consultantlm.com/...)
- **[CONFIRMED]** Apple *Lopez v. Apple* Siri settlement: users compensated for "unintended Siri activation" recordings during "confidential or private" conversations, 2014–2024 window. (https://www.axios.com/2025/07/01/apple-settlement-siri-lopez-voice-assistant-claim)
- **[koe-IMPLICATION — existential]** koe is *exactly* the always-on ambient recorder these suits target. koe already plans "terms + recording consent" as a product-layer gap; this research says it is **P0, not polish**. Concrete musts: (a) koe must not silently capture *bystanders/other parties* — its recorder must be scoped to the user's intended interactions; (b) the SQLite recorder's *biometric* exposure (voiceprints) is a BIPA surface even if audio is deleted; (c) per-state consent posture matters for a US launch. Granola's pattern (below) is the proven mitigation.

### 4.2 Granola as the privacy gold-standard pattern

- **[CONFIRMED]** Granola: **captures device audio locally, transcribes in real time, deletes audio immediately, stores no recording anywhere, no bot joins the call, no audio for 3rd-party training**; contractually prohibits providers from training on data; transcript auto-deletion configurable. (https://www.granola.ai/blog/ai-notetaker-participant-privacy-consent, https://www.granola.ai/security)
- **[CONFIRMED]** Disclosure timing matrix: before the call (calendar), at start (verbal script), when transcribing (automatic in-chat notice = timestamped consent artifact); consent captured *in the transcript itself*. On decline: "stop... acknowledge the choice without friction, proceed with manual notes." (https://www.granola.ai/blog/...)
- **[koe-IMPLICATION]** koe's RecorderAdapter trait should default to **transcribe-then-delete-audio** (Granola pattern) rather than store-audio. "What's stored" must be explicit and user-visible (transcripts/notes, not raw audio). koe's planned "data deletion" gap should be a first-class, discoverable control, not buried. **The glass-box thesis extends naturally to data: koe can show, live, "I'm storing this note / I deleted the audio" as part of its transparency window** — turning a compliance burden into a thesis demonstration.

### 4.3 OS mic/screen permission flows + privacy indicator expectations

- **[CONFIRMED]** Windows 11: mic icon appears in taskbar notification area while an app uses the mic; hover reveals which app; Privacy & Security > App permissions shows a "Recent activity" log of which apps used a sensor and when. (https://support.microsoft.com/.../windows-camera-microphone-and-privacy-..., https://windowsforum.com/threads/windows-sensor-icons-...)
- **[CONFIRMED]** macOS (Monterey+): **orange dot** in menu bar when mic active; Control Center shows the mic symbol + the app name using it; "Recording Indicator" in Control Center shows current/recent use. (https://support.apple.com/guide/mac-help/control-access-to-the-microphone-on-mac-..., search synthesis)
- **[CONFIRMED]** Cross-platform user expectation: "that little orange or white dot... means an app is using the microphone right now" — users are *trained* to read OS indicators as ground truth. (https://www.makeuseof.com/important-windows-taskbar-privacy-icons-meaning/)
- **[koe-IMPLICATION]** Users will see the OS mic indicator lit *whenever koe is open-mic*, which for an always-on app is *constantly* — this reads as creepy unless koe's own in-app indicator is **more granular and more trustworthy than the OS dot.** koe's orb should make "listening now" vs "muted" vs "VAD-idle (mic open but not capturing)" unmistakable and color-independent, so the user trusts koe's state over the always-lit OS dot. The gate-behind-VAD design (§2.3) *also* helps here: if koe only opens the full mic stream on VAD/wake, the OS dot is lit less often, reducing the creep factor — privacy, battery, and cost align again.

---

## 5. ACCESSIBILITY for voice-first

- **[CONFIRMED]** WCAG 1.2.1 (Text Alternatives / captions & transcripts for voice content) applies to conversational interfaces: "if chatbots use audio... captions, transcripts, or alt text should be available." (https://www.accesify.io/blog/voice-conversational-accessibility-chatbots-vui/)
- **[CONFIRMED]** ChatGPT voice design rule: "Always provide transcripts and captions; support keyboard-only control"; live transcription ensures "non-audio comprehension," keyboard support covers motor accessibility. Transcript is explicitly "part of... accessibility." (https://completeaitraining.com/news/chatgpt-voice-breaks-free-...)
- **[CONFIRMED]** AI-speech transparency duty: "website visitors should be aware when speech is AI-generated." (https://www.accesify.io/...)
- **[CONFIRMED]** Voice-first AI is "an interface shift" for blind/low-vision users — from "navigating screen-based obstacles to expressing intent through natural conversation." (https://toptechtidbits.com/how-voice-first-ai-could-soon-change-everything-for-blv-people/)
- **[CONFIRMED — color independence]** WCAG-aligned guidance emphasizes states must not rely on color alone; contrast + readable typography for multimodal systems. (https://www.accesify.io/..., general WCAG 1.4.1 Use of Color)
- **[koe-IMPLICATION]** Two accessibility tensions unique to koe: (1) **Screen-reader vs koe both speaking** — a blind user's screen reader and koe's TTS will collide; koe needs a mode that yields the audio channel to the screen reader and surfaces everything as accessible text/captions instead. (2) **Color-coded orb states fail for color-blind users** — koe's 4 orb states (idle/listening/thinking/speaking) and the safety tiers (SAFE/CAUTION/DANGER) must each have a **shape/motion/icon/text** encoding, not just color (e.g. breathing-amplitude, ring style, an explicit text label in the thinking window). This is mandatory for the glass-box thesis: a transparency signal that's invisible to a color-blind user isn't transparent. Captions of koe's *own speech* (live transcript under the orb) double as the accessibility layer **and** the glass-box audit log — one feature, two wins.

---

## 6. COMPETITOR REFERENCES (always-on voice: who does what well/badly)

| Product | Activation | State display | Barge-in | Privacy posture | Does well | Does badly (for always-on) | Source |
|---|---|---|---|---|---|---|---|
| **Siri** | "Hey Siri" wake + side-button | Orb/waveform | Limited | On-device wake; cloud after | Ubiquity, on-device wake | **False activation → class-action settlement (Lopez v. Apple), confidential recordings** | axios.com/2025/07/01/apple-settlement-siri-lopez-... |
| **Google Assistant / Gemini Live** | Wake + Continued-Conversation open window | Listening cues; **transcript button** top-right; new screen for Live | **Yes, native barge-in** | Temporary Chat = no stored transcript; noise filtering | Low latency (~1s first token), barge-in, continued conversation w/o re-wake, tool access mid-session | Live "switches to a completely new screen" (modal, breaks ambient feel); some features disabled in voice mode | gemini.google/overview/gemini-live/, blog.google/.../gemini-audio-model-updates/, letsdatascience.com/... |
| **ChatGPT Advanced Voice** | Tap to enter; in-chat (no mode switch) | **Pulsing orb** (listen+speak shared); **live transcript in thread** | **Yes, mid-sentence** | Mic permission; transcripts | No mode-switch, partials-fast visuals, barge-in, captions+keyboard a11y | Single orb conflates listening/speaking (2 states only); historically a separate full-screen mode (now fixed) | completeaitraining.com/..., help.openai.com/.../8400625 |
| **Superwhisper** | **Hold-to-talk hotkey** | Minimal | n/a (dictation) | **100% on-device, audio never leaves Mac, offline** | Privacy gold standard, constant latency, custom modes | Mac-only; steep setup ("configuring a server"); manual transcript cleanup | getvoibe.com/resources/wispr-flow-vs-superwhisper/ |
| **Wispr Flow** | **Hold-to-talk hotkey** | "Best UI" per reviewers | n/a (dictation) | **Cloud (OpenAI/Meta servers), audio sent off-device** | Polished UX, auto-edit/format, 100+ langs, Whisper Mode (quiet) | **~800MB RAM / ~8% CPU (heavy for always-on)**; no offline; 1–2s cloud round-trip | getvoibe.com/..., tldv.io/blog/wisprflow/ |
| **Granola** | App captures device audio (meetings) | n/a | n/a | **Local capture, audio deleted immediately, no bot, auto consent message** | **Privacy/consent gold standard for ambient recording**; no-bot trust | Meeting-scoped, not a general voice agent | granola.ai/blog/ai-notetaker-participant-privacy-consent |
| **Alexa/smart speakers** | Wake + mic array | Light ring | Echo-wake issues | Cloud after wake | Mic array (+14 dB SNR), firmware acoustic refinements | "Echo wake" self-retrigger; vulnerable to silent commands | kunalganglani.com/.../self-hosted-voice-assistant-..., neowin.net/.../silent-commands/ |

**Synthesized competitor lessons for koe:**
- **[CONFIRMED]** *No mode switching* (ChatGPT) and *don't jump to a new screen* (Gemini Live's weakness) — keep voice ambient and in-context. koe's persistent orb window already does this; **do not make voice a separate full-screen mode.** (completeaitraining.com/..., search synthesis)
- **[CONFIRMED]** *Transcript-as-UI* (ChatGPT, Gemini transcript button) is now table-stakes for audit + accessibility. koe's thinking-window/conversation-log should be the persistent transcript. (completeaitraining.com/...)
- **[CONFIRMED]** *On-device = trust* (Superwhisper, Granola). koe is BYOK-on-device for keys but streams audio to OpenAI Realtime — that's a *cloud* audio path, a privacy gap vs Superwhisper/Granola. koe's self-hosted Qwen3.5-Omni path closes this and is a differentiator. (getvoibe.com/..., granola.ai/...)
- **[CONFIRMED]** *Always-on resource cost is real* (Wispr 800MB/8% CPU). koe must beat this via VAD-gating, or residency fails. (getvoibe.com/...)
- **[CONFIRMED]** *Ambient recording = legal liability* (Otter/Apple/BIPA suits). The Granola no-store/no-bot/consent pattern is the only safe template. (granola.ai/..., consultantlm.com/...)

---

## 7. WHERE TEXT-FIRST APPS (Codex App / Hermes Desktop) STRUCTURALLY CANNOT LEAD — koe's lane

**[INFERRED — synthesis, this is the strategic payload]:** Each item below is a UX dimension that only *exists* for a voice-first always-on product. A text/chat developer agent has no continuous audio, so it has nothing to design here — meaning koe isn't "catching up," it's in an empty category.

1. **Turn-taking & barge-in latency budgets** (<150ms barge-in, 200–700ms gap bands) — text apps have no turn-taking; the user just stops typing.
2. **4-state real-time presence** (idle/listening/thinking/speaking) rendered as an ambient orb — text apps show a spinner at most.
3. **Speak-vs-silence discipline** for disclosing tool/confidence/thinking — text apps print everything; koe must *choose* what to voice, which is a genuine design surface and a thesis differentiator.
4. **Calibrated *spoken* confidence vocabulary** (3–4 tiers, not %, tuned to accuracy) — validated by the over/under-confidence trust research; text apps can append a "(low confidence)" tag cheaply, but koe must make it *hearable without annoying*, which is the hard, ownable problem.
5. **Always-listening consent & recording law** (BIPA/two-party/ambient-capture felony) — text apps record nothing ambient; this is koe's burden *and* its trust moat (Granola proved consent-done-right is a feature).
6. **OS mic-indicator trust management** — text apps never light the mic dot; koe's always-on lights it constantly and must out-trust the OS dot with its own granular indicator.
7. **VAD-gated battery/cost residency** — text apps idle near-free; koe must engineer the cheap-local-stage → expensive-stream-on-demand pipeline (and it doubles as the cost gate and the privacy gate).
8. **Voice↔screen-reader coexistence & captions-of-AI-speech** — text apps are already screen-reader native; koe must solve the dual-audio collision, which forces it to build the captions/transcript layer that *also* serves the glass-box audit thesis.

**Single sharpest synthesis line for downstream:** koe's central thesis (calibrated glass-box) and the always-on-voice constraints are not in tension — they *converge*. The "thinking window" that the thesis needs for transparency is the same 300–700ms turn-gap the voice pipeline needs anyway; the calibrated-tier (not raw-%) choice is exactly what the trust research demands; the transcribe-then-delete recorder that the law demands is also a live glass-box "what I stored" demonstration; and VAD-gating that battery/cost needs also shrinks the creepy always-lit OS mic dot. Every always-on-voice hard constraint, handled koe's way, *reinforces* the transparency thesis rather than competing with it — which is precisely why a text-first feature-copy would dilute koe and why this is koe's category to own.

---

### Source list (primary URLs cited above)
- Voice UI / state / earcons: https://fuselabcreative.com/voice-user-interface-design-guide-2026/
- Wake word metrics / on-device: https://picovoice.ai/blog/complete-guide-to-wake-word/ · https://sensory.com/custom-wake-words-branded-voice-ux-guide-2026/
- Turn detection / VAD / barge-in numbers: https://livekit.com/blog/turn-detection-voice-agents-vad-endpointing-model-based-detection · https://futureagi.com/blog/voice-ai-barge-in-turn-taking-2026/ · https://altersquare.medium.com/why-vad-end-of-speech-detection-is-the-hardest-problem-in-production-voice-agents-fee308e38cfc
- Turn-taking visual cues (academic): https://arxiv.org/pdf/2505.21043
- Dictation competitors: https://www.getvoibe.com/resources/wispr-flow-vs-superwhisper/ · https://tldv.io/blog/wisprflow/
- ChatGPT voice / no-mode-switch / captions: https://completeaitraining.com/news/chatgpt-voice-breaks-free-no-mode-switching-live/ · https://help.openai.com/en/articles/8400625-voice-mode-faq (403, used search excerpts)
- Gemini Live: https://gemini.google/overview/gemini-live/ · https://blog.google/products/gemini/gemini-audio-model-updates/ · https://www.gend.co/blog/enhanced-gemini-models-boost-voice-interactions · https://letsdatascience.com/news/gemini-for-home-enables-seamless-continued-conversations-5f88b640
- Confidence calibration / trust: https://www.researchgate.net/publication/388821876_Confronting_verbalized_uncertainty_... · https://arxiv.org/abs/2306.13063 · https://openreview.net/forum?id=uZ2A0k5liR · https://arxiv.org/pdf/2508.18847
- OS mic indicators: https://support.microsoft.com/en-us/windows/windows-camera-microphone-and-privacy-... · https://support.apple.com/guide/mac-help/control-access-to-the-microphone-on-mac-... · https://www.makeuseof.com/important-windows-taskbar-privacy-icons-meaning/
- Consent law / ambient recording / BIPA: https://keku.com/blog/call-recording-laws-by-state · https://consultantlm.com/consultant-article/voice-data-privacy-laws-2025-... · https://www.axios.com/2025/07/01/apple-settlement-siri-lopez-voice-assistant-claim
- Granola privacy/consent pattern: https://www.granola.ai/blog/ai-notetaker-participant-privacy-consent · https://www.granola.ai/security
- Agentic HITL / notification / UI patterns: https://agentic-design.ai/patterns/ui-ux-patterns · https://aws.amazon.com/blogs/machine-learning/human-in-the-loop-constructs-... · https://medium.com/@AlignX_AI/designing-human-in-the-loop-for-agentic-workflows-079faec737ed
- Residency / tray / hotkey: https://www.producthunt.com/products/flowly-6 · https://www.revoyant.com/blog/best-ai-assistants-for-windows-in-2026
- Accessibility: https://www.accesify.io/blog/voice-conversational-accessibility-chatbots-vui/ · https://toptechtidbits.com/how-voice-first-ai-could-soon-change-everything-for-blv-people/