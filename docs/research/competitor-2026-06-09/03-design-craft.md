I now have exhaustive primary-source data across all six dimensions. Compiling the dense research findings.

---

# RESEARCH DATA — Visual/Interaction Craft for koe's Immersive Voice-Orb Redesign

Status legend: **[C]** = confirmed in a primary source (URL cited) · **[I]** = inferred / synthesized by me from confirmed facts or general craft knowledge. All values are reusable design tokens for the downstream synthesis.

---

## 1. AMBIENT/ORB & VOICE-STATE VISUALIZATION

### 1.1 Apple Siri (iOS 18.1 → 26 "Liquid Glass") — the canonical voice-presence language

- **[C]** Apple **abandoned the centered orb** for an **edge-of-screen glow**: a multicolor light wraps the *entire screen border* during a Siri session, and a Liquid-Glass element "springs out of the Dynamic Island." The orb is no longer the surface — the *frame* is. Source: https://appleinsider.com/articles/26/05/28/this-is-what-the-siri-redesign-might-look-like-in-ios-26
- **[C]** The glow is **directional and tied to the activation gesture** — this is the key state-comm insight: motion *originates from where the user acted*. Side-button hold → colored lines radiate *from that physical edge*. Voice ("Hey Siri") → glow rises *from the bottom*. Source: https://www.slashgear.com/1865686/iphone-glowing-around-edges-reason/
- **[C]** During the active phase the border glow "**distorts everything briefly**" (refraction of underlying content), and "colorful lights dance around the edges." It is explicitly a **trust/affordance signal** ("a visual cue that Apple Intelligence has kicked in"), not decoration. Source: https://www.slashgear.com/1865686/iphone-glowing-around-edges-reason/
- **[C]** iOS 26 Liquid Glass = **translucent elements that reflect and refract surrounding content in real time** — the material *samples and bends* what's behind it rather than using flat fills. Source: https://www.engadget.com/mobile/smartphones/apple-reveals-new-ios-26-features-and-its-liquid-glass-redesign-at-wwdc-2025-171650243.html
- **[I] koe takeaway**: A *full-window rim/halo glow* is a stronger, less-generic state signal than a pulsing center orb alone. koe can do **both**: center orb = "where the AI lives," window-rim glow = "what state it's in." Rim color/intensity carries state; orb carries breathing/voice-reactivity. Directional origin (glow rising from the bottom thinking-window when koe starts speaking) is a free, premium-feeling detail to steal.

### 1.2 ChatGPT Advanced Voice Mode — the blue-orb state machine (most directly analogous to koe)

- **[C]** Core idle/ready state = an **animated blue orb, pulsing while it listens and responds**; on activation the center circle "morphs into a fluid sky-like blue-and-white animation." Source: https://help.openai.com/en/articles/8400625-voice-mode-faq and https://www.aiforwardmarketer.com/the-blue-orb-that-just-might-change-everything/
- **[C]** **State is encoded by color desaturation**: the blue-and-white orb **turns solid white when unresponsive/stalled** (connectivity/processing failure). Blue = advanced/live; black circle = the older standard mode. So: *full saturation = healthy live*, *washed to white/mono = degraded/stuck*. Source: https://community.openai.com/t/advanced-voice-mode-quickly-becomes-unresponsive/965704
- **[C]** Three packaging modes for the same orb: inline in chat, a **floating voice orb**, or a **separate full-screen voice mode**. Source: https://www.tomsguide.com/ai/chatgpt/openai-just-launched-chatgpt-advanced-voice-mode-for-the-web-heres-how-to-get-it
- **[I] koe takeaway**: koe's 6 states (idle / connecting / conversing / working / reconnecting / error) map cleanly onto an orb whose **saturation+motion** is the primary channel. Critically: ChatGPT proves *desaturation-to-mono = "something is wrong"* is an intuitive, learned convention — koe should reserve **mono/grey-white drain** for reconnecting, and **red-sink** for error, never use full chroma for a degraded state.

### 1.3 Concrete voice-reactive orb implementation values (production React/Web Audio)

From a working implementation — directly portable to koe's React layer:
- **[C]** Audio level normalization: `level = (avg − 16) / 90`, clamped 0–1, where `avg` is averaged FFT magnitude. **FFT size = 1024**. Source: https://medium.com/@therealmilesjackson/building-a-voice-reactive-orb-in-react-audio-visualization-for-voice-assistants-2bee12797b93
- **[C]** **Two-stage smoothing** (this is what makes it feel alive, not jittery): (a) raw→ref: `levelRef += (norm − levelRef) * 0.15`; (b) ref→render: `setLevel(prev => prev + (levelRef − prev) * 0.25)`. Source: same
- **[C]** Scale map: `scale = 1 + level * 0.35` (orb grows up to +35% on loud speech). Transition: `transform 0.12s ease-out`. Source: same
- **[C]** Glow opacity map: `0.25 + level * 2.45` (glow ramps hard with volume — glow is the loud channel, scale is the subtle channel). Source: same
- **[C]** Internal turbulence speed map: `0.75 + level * 0.5`. Base amplitude: `0.18 + level * 1.7`. Source: same
- **[C]** Static styling: orb `200px` square, **`blur-[130px]`** soft bloom, **`box-shadow: 0 0 90px rgba(58,108,255,0.45)`**, base RGB `[0.3, 0.6, 1]`. Source: same
- **[I] koe takeaway**: Use **two motion channels at different gains** — subtle scale (±35%) for "it hears you," aggressive glow (×~10 range) for "it's actively vocalizing." The `0.15`/`0.25` smoothing constants are the magic numbers; copy them. Swap the blue `[0.3,0.6,1]` for koe's calmer accent (see §3).

### 1.4 Breathing (idle) animation — concrete CSS

- **[C]** Canonical idle breathing: `@keyframes pulse { 0% { transform: scale(.15) rotate(180deg) } 100% { transform: scale(1) } }`, `animation: pulse 4s cubic-bezier(0.5,0,0.5,1) alternate infinite;` — note **`alternate`** (in-out reversal = natural inhale/exhale) and the **symmetric sine-like bezier** `(0.5,0,0.5,1)`. Source: https://css-tricks.com/recreating-apple-watch-breathe-app-animation/ + https://uiverse.io/challenges/voice-assistant-orb
- **[I]** For koe idle: a **~4–6s** breath cycle, scale range ~`0.97 → 1.03` (subtle — the Apple Watch Breathe app uses a slow 4s+ inhale specifically to feel calm/meditative), `cubic-bezier(0.37, 0, 0.63, 1)` (true ease-in-out-sine) on `alternate infinite`. Slower than ChatGPT's "ready pulse" → reads as *calm presence*, not *waiting impatiently*.

### 1.5 Mesh-gradient / glow orb craft — avoiding AI-template look

- **[C]** Mesh gradients = **multiple elliptical color orbs**, each with independent position/size/blur/opacity, composited via **CSS blend modes (`screen`, `multiply`, `overlay`)** — same as Figma/XD. Source: https://randomgen.eu/mesh-gradient-generator/
- **[C]** **The off-center rule** (this is the single biggest anti-flat tip): "a *centered* radial gradient looks flat; an *off-center* one looks like actual light." Shift the light source off-center to read as a real spotlight. Source: https://jaconir.online/blogs/css-radial-gradient
- **[C]** Layer a secondary **overlay pattern** (concentric rings, dot grid, fine bands) on top of the gradient to add depth — flat mesh alone reads generic. Source: https://randomgen.eu/mesh-gradient-generator/
- **[C] Grain-over-gradient (the premium de-flattener)**: `<feTurbulence type='fractalNoise' baseFrequency='0.65' numOctaves='3' stitchTiles='stitch'/>` layered *under* the gradient via `background: linear-gradient(...), url(noise.svg);` and pushed with `filter: contrast(170%) brightness(1000%)` to harden the speckle. This kills banding and the "smooth = cheap/AI" look. Source: https://css-tricks.com/grainy-gradients/
- **[C]** `numOctaves` above **3–4 rarely justifies the perf cost**; `baseFrequency` controls grain size. Source: https://tympanus.net/codrops/2019/02/19/svg-filter-effects-creating-texture-with-feturbulence/
- **[I]** koe project rule (`anti-ai-smell.md`) already mandates `feTurbulence baseFrequency=0.65, opacity 0.05–0.10`. This *matches* the confirmed CSS-Tricks technique — so koe's orb should be: 2–3 off-center radial color stops (calm palette, NOT purple→blue) + `screen` blend + a `0.06`-opacity grain overlay + a wide soft `blur(60–130px)` bloom ring. This is the recipe for an orb that looks hand-crafted, not Midjourney-generated.

### 1.6 Orb vs waveform vs particle field — when to use which (per-state)

- **[C]** Wispr Flow's pattern: **collapsed bubble (idle)** → **expands to a waveform + pulsing record indicator (active dictation)** → collapses back to screen edge. The waveform appears *only during active capture*; idle is a calm bubble. Source: https://wisprflow.ai/ and https://docs.wisprflow.ai/articles/6409258247-starting-your-first-dictation
- **[I] koe state→visual mapping** (synthesis of all above):
  - **idle** → slow-breathing orb, low saturation, no waveform. ("話しかけて" micro-label.)
  - **connecting** → particles/specks *converging inward* to form the orb (Siri's "spring out" + the brief's "収束していく粒子"). Convergence = "assembling."
  - **conversing** → orb surface **ripples/wobbles to live audio** (the §1.3 amplitude→scale/glow maps). This is the only state with voice-reactive deformation.
  - **working** → orb stops voice-reacting; instead a **concentric pulse / slow rotation** radiates outward (different motion *kind*, so it's distinguishable from conversing even at a glance). Tie ring color to the running tool's risk tier.
  - **reconnecting** → drain to **mono/grey-white** (ChatGPT's stall convention) + a slow, *lower-amplitude* breath. Recovery reads as "weak pulse," not "dead."
  - **error** → sink to **desaturated red**, motion nearly stops, short JP message + 再試行. (Color + icon + text, never color alone — colorblind rule.)
- **[I]** Keep ONE primary geometry (the orb) and vary **motion-kind + saturation** across states rather than swapping orb↔waveform↔particles wholesale. Swapping geometries per state is the thing that makes assistant UIs feel incoherent; Siri/ChatGPT both keep one identity and modulate it.

---

## 2. WINDOW BEHAVIOR for always-on apps

- **[C] Global hotkey is the spine of always-on AI.** Perplexity's redesigned Mac app summons a floating Command Bar with **double-tap of both ⌘ keys**, working over any app, "no app-switching." Source: https://winbuzzer.com/2026/05/08/perplexity-opens-personal-computer-to-all-mac-users-xcxwbn/ and https://www.facebook.com/9to5mac/posts/...1310083143818380/
- **[C]** Fazm/menubar-agent thesis: a **floating bar activated by a keyboard shortcut** beats a sidebar because it's "always available, requires no app-switching, never gets in the way, appears when you need it and disappears when you don't" — sidebars "steal screen space permanently." Source: https://fazm.ai/blog/native-swift-menu-bar-ai-agent
- **[C]** Granola's window = **"just a sliver of a window at the edge of a screen"** built on an **"invisible design"** philosophy — *"like an invisible handrail… stays out of the way while you live your life, but is instantly there when you need it."* Source: https://digitalfrontier.com/articles/granola-ai-note-taking-interview and https://medium.com/design-bootcamp/...ff72215b6553
- **[C] Chrome that hides until needed**: macOS Big Sur **blends title bar + toolbar into content until you scroll/hover** for a few seconds; iOS 15 nav bars are transparent until scroll. Reduces clutter, elevates content. Source: https://medium.com/design-bootcamp/ui-design-trends-of-today-...aa6cd66f9ccf
- **[C] Tauri-specific mechanics** (koe's stack):
  - Floating panel = `set_always_on_top(true)` + `set_decorations(false)` + `set_position()`. On macOS use Cocoa/NSWindow level to float above all. Source: https://github.com/orgs/tauri-apps/discussions/4452
  - **Use the native global-shortcut plugin** (`tauri-plugin-global-shortcut`) for system-wide summon — it "fires even when your app isn't focused." Use JS key listeners ONLY for in-window shortcuts. Source: https://v2.tauri.app/plugin/global-shortcut/ and https://dev.to/hiyoyok/global-keyboard-shortcuts-in-tauri-v2-...2h6d
  - macOS global shortcuts may trigger an **accessibility-permission prompt**; corporate-MDM users may be unable to grant it (plan a fallback). Source: same
  - Known caveat: `setAlwaysOnTop` has had reliability bugs — verify per-platform. Source: https://github.com/tauri-apps/tauri/issues/9439
- **[I] koe synthesis**:
  - koe's 440×680 tall-narrow window *is* the Granola "sliver" / Perplexity "floating bar" idea, just taller because it carries the orb + thinking-window. Default behavior should be **floating, frameless, pin-to-edge, always-on-top optional**, summoned by a **global hotkey** (e.g. a double-modifier, Perplexity-style) and a **tray/menubar icon** (koe already has tray/residency planned via `koe-944`).
  - **Push-to-talk**: orb-tap or Space toggles conversation (brief mandates Space). For an always-on secretary, also support a **global PTT hotkey** so the user can talk without focusing the window — the whole point of always-on.
  - **Chrome that recedes**: settings gear, cost line, window controls should fade to near-invisible when idle and surface on hover — mirrors Big Sur/Granola "invisible until needed." This protects the immersive orb space.
  - **Multi-monitor**: remember last position per display; summon on the display with the active cursor (standard for floating assistants). **[I]**

---

## 3. TYPOGRAPHY & COLOR for calm/premium (avoid Inter + purple→blue)

### 3.1 Two opposite reference palettes — pick koe's lane

**Claude (warm/editorial/calm — closest to "knowing presence")** — Source: https://github.com/VoltAgent/awesome-design-md/blob/main/design-md/claude/DESIGN.md
- **[C]** Canvas is **warm cream `#faf9f5`, deliberately NOT pure white** ("tinted cream differentiates from cool-gray competitors").
- **[C]** Light surface ladder: `#faf9f5` → `#f5f0e8` → `#efe9de` → `#e8e0d2`. Dark ladder: `#181715` → `#1f1e1b` → `#252320`. Hairlines `#e6dfd8` / `#ebe6df`.
- **[C]** Accent = **warm terracotta `#cc785c`** (active `#a9583e`) — "warm muted terracotta, NOT neon cyan or pure blue; counter-positions against OpenAI/Google/Microsoft." Used ONLY on primary CTA + full-bleed callouts, never scattered.
- **[C]** Text: ink `#141413` (warm off-black), body `#3d3d3a`, muted `#6c6a64`. Semantics: success `#5db872`, warning `#d4a017`, error `#c64545`, teal accent `#5db8a6`.
- **[C]** Type: serif display (Copernicus/Tiempos, weight **400 always, never bold**, negative tracking −0.3 to −1.5px) + humanist sans body (Styrene/Inter). Code = JetBrains Mono 14/1.6.
- **[C]** Spacing base 4px → 4/8/12/16/24/32/48/**96**(section). Radii **varied**: xs 4 / sm 6 / md 8 / lg 12 / xl 16 / pill 9999. Shadows: essentially none — *"surface color IS the depth signal,"* rare `0 1px 3px rgba(20,20,19,0.08)` on hover only.

**Linear (cool/precise/dark — operator-grade)** — Source: https://github.com/voltagent/awesome-design-md/blob/main/design-md/linear.app/DESIGN.md
- **[C]** Near-black canvas `#010102` (black with a *blue tint*), surface ladder `#0f1011`→`#141516`→`#18191a`→`#191a1b`, hairlines `#23252a`/`#34343a`.
- **[C]** Accent lavender-blue `#5e6ad2` (hover `#828fff`). Text ink `#f7f8f8`, muted `#d0d6e0`, subtle `#8a8f98`. Success `#27a644`.
- **[C]** Type: SF Pro Display-based, weight 600 for display with **aggressive negative tracking** (display-xl 80px/−3.0px, 56px/−1.8px, 40px/−1.0px). Body 16/1.5/−0.05px. Eyebrow 13/500/**+0.4px** (positive tracking on tiny caps).
- **[C]** **No shadows, no gradients, no glass** — depth purely via the surface color-ladder + 1px hairlines.

### 3.2 Raycast (dark monochrome HUD — most like koe's current "operator console" it's moving *away* from)
- **[C]** Surfaces `#07080a`→`#0d0d0d`→`#101111`→`#121212`; hairline `#242728` / `rgba(255,255,255,0.08–0.16)`. Accents are **single saturated splashes reserved for category tiles** (blue `#57c1ff`, red `#ff6161`, green `#59d499`, yellow `#ffc533`) — never ambient. Radii 4/6/8/10/16. **No drop shadows.** Source: https://github.com/VoltAgent/awesome-design-md/blob/main/design-md/raycast/DESIGN.md
- **[C]** Signature typographic trick: `font-feature-settings: "calt","kern","liga","ss03"` (the ss03 single-story `g`) + slightly *positive* letter-spacing (0.1–0.4px) to keep dark UI airy. Source: same

### 3.3 Japanese rendering — concrete font stacks
- **[C]** Best system JP faces: **Hiragino Kaku Gothic / Hiragino Sans (macOS/iOS — 9 weights, "Helvetica of Japanese")**; **Yu Gothic UI (Windows 8.1+)**. Web-font options: **Zen Kaku Gothic** (geometric, clean, corporate-trusted, the calm Noto alternative). For elegant/intimate: **Sawarabi Mincho** (serif). Source: https://www.az-loc.com/best-fonts-for-chinese-japanese-korean-websites/ and https://jstockmedia.com/blog/practical-japanese-web-fonts-on-google-fonts/
- **[C] JP typography rules**: line-height **185–200%** of font size (much looser than Latin), ~**35 chars/line**, and **reduce JP font size 10–15% vs Latin** to optically balance. Source: https://note.com/ababo/n/n6a8aae8b07eb and https://www.az-loc.com/...
- **[I] koe recommended stack** (OS-following, anti-AI-smell compliant, NO Inter as primary):
  - Latin display/UI: **Geist** or **Plus Jakarta Sans** (koe's anti-ai-smell.md whitelist) — NOT Inter.
  - JP: `"Hiragino Sans", "Hiragino Kaku Gothic ProN", "Yu Gothic UI", "Zen Kaku Gothic New", system-ui, sans-serif` — lets OS native face win (premium on each OS), with Zen Kaku Gothic as the web fallback so Windows/Linux still look intentional.
  - Set `font-feature-settings: "palt"` for JP (proportional kana spacing — the JP equivalent of Raycast's ss03 polish) and `line-height: 1.9` for JP body.

### 3.4 Color strategy for koe (synthesis)
- **[I]** koe is a *consumer voice secretary with a calm presence thesis*, not a dev console → lean **Claude-warm, not Linear-cool**. The current "dark operator console" is the Raycast/Linear lane the brief explicitly rejects.
- **[I]** OS-following light/dark with restrained accent:
  - **Light**: warm near-white canvas (Claude `#faf9f5` family, not `#fff`), warm-grey text, ONE low-chroma accent for the orb's living core + the active-state rim. Avoid cyan/electric-blue (the §1.2 ChatGPT default) — it reads "generic AI."
  - **Dark**: warm-tinted near-black (NOT Linear's blue-black `#010102`; use a neutral/warm `#16151400`-ish so the dark mode still feels *calm* not *cold*), surface ladder, same single accent.
  - Per koe's `quality.md`: body text must hit `#767676`/4.54:1 min on white; CTA white-text safe colors `#c94400`/`blue-600 #2563eb`/`green-700`. Terracotta `#cc785c` at 4.6 ish is borderline on white → use the darker `#a9583e` for text-bearing CTAs.
- **[I]** **Reserve red exclusively for DANGER/error** (the 3-tier gate). Accent must NOT be red, or the safety signal loses meaning. CAUTION = amber/warning `#d4a017`. SAFE = the calm accent/neutral. This makes the orb's working-state ring double as a risk-tier indicator for free.

---

## 4. MOTION — easing/duration systems, breathing, reduced-motion, micro-interactions

### 4.1 Duration & easing tokens (use different curves per role — brief mandates this)
- **[I/C]** koe's own `anti-ai-smell.md` already defines the right tokens (and they match Material/industry norms):
  - `--ease-standard: cubic-bezier(0.4, 0, 0.2, 1)` (general)
  - `--ease-decel: cubic-bezier(0, 0, 0.2, 1)` (entrances)
  - `--ease-accel: cubic-bezier(0.4, 0, 1, 1)` (exits)
  - `--ease-spring: cubic-bezier(0.2, 0.8, 0.2, 1)` (interactions)
  - Durations: toggle **100ms**, hover **150ms**, modal **200–300ms**, page/route **250–400ms**. *Never* one global 300ms-on-everything.
- **[C] Material 3 Expressive** has moved from duration-based to **physics/spring-based** motion, with two schemes: **Expressive** (low damping → overshoot/bounce, for "hero moments") and **Standard** (high damping → minimal bounce, "calmer, utilitarian"). Spring tokens split into **spatial** (position/size/rotation, *allowed to overshoot*) and **effects** (color/opacity, **no overshoot**). Source: https://m3.material.io/styles/motion/easing-and-duration/tokens-specs and https://supercharge.design/blog/material-3-expressive
- **[I] koe mapping**: koe is a *calm* secretary → use the **Standard (high-damping)** scheme as default; reserve **Expressive overshoot** for ONE hero moment (onboarding world-ignition, §6). Button taps = light spring (`--ease-spring`); orb breathing = slow ease-in-out-sine; state cross-fades = `effects` (opacity, no overshoot).

### 4.2 Breathing specifics (idle presence)
- **[C/I]** 4s+ slow `alternate infinite` cycle, symmetric sine bezier `cubic-bezier(0.5,0,0.5,1)` ≈ `(0.37,0,0.63,1)`, scale `~0.97↔1.03`, paired glow-opacity breath slightly out of phase with scale for organic feel. (Apple Watch Breathe uses long inhales specifically to induce calm.) Source: https://css-tricks.com/recreating-apple-watch-breathe-app-animation/

### 4.3 Reduced-motion (koe must ship a calm fallback)
- **[C]** WCAG defines "motion animation" as *the illusion of movement* and **explicitly excludes color, blur, and opacity changes** — these are the *safe* substitutes. So under `prefers-reduced-motion: reduce`: replace scale/translate/rotate with **opacity cross-fades + color transitions**, keep durations short. A modal that *fades* instead of *flying up* still signals state without vestibular risk. WCAG 2.3.3 requires a way to disable non-essential motion. Source: https://www.w3.org/WAI/WCAG22/Techniques/css/C39 and https://css-tricks.com/almanac/rules/m/media/prefers-reduced-motion/ and https://www.smashingmagazine.com/2020/09/design-reduced-motion-sensitivities/
- **[I] koe reduced-motion orb**: kill the scale-breath + audio-reactive deformation; keep a **slow opacity/saturation pulse only** (legal under WCAG since opacity/color aren't "motion"). State changes become **color/saturation cross-fades**, no convergence-particles, no rotation. The orb still "lives" via brightness breathing — just doesn't move. This satisfies the brief's "抑制版" requirement cleanly.

### 4.4 Micro-interactions (premium polish details to steal)
- **[C]** Codex's computer-use cursor: **wiggles to show the model is thinking**, takes **playful (non-straight) paths**, and **derives its color from the system wallpaper** — reviewer rated its design quality as matching "only one other major tech company." Source: https://www.macstories.net/notes/openais-new-codex-app-...  and https://developers.openai.com/codex/app
- **[I] koe steals**: (a) a **"thinking wiggle"** — when koe is mid-reasoning, a tiny irregular jitter/shimmer on the orb (not a clean spin) reads as *organic thought*, not a loading spinner. (b) **Wallpaper/OS-accent color sampling** — koe's "OS-following color" can go beyond light/dark to *sample the user's OS accent color* for the orb core, making it feel native and personal on every machine (Tauri can read the Windows/macOS accent color). This is a concrete, high-craft way to satisfy "OS追従配色" beyond just light/dark.

---

## 5. DENSITY & INFORMATION LAYERING — status + thinking-window + cost + approvals around one hero orb

- **[C]** Granola's whole thesis is **"invisible design"** — UI present only when needed, otherwise out of the way. Source: https://digitalfrontier.com/articles/granola-ai-note-taking-interview
- **[C]** Chrome should **recede into content until interaction** (Big Sur toolbar blend, iOS transparent-until-scroll). Source: https://medium.com/design-bootcamp/...aa6cd66f9ccf
- **[I] koe layout system** (440×680, brief's 2/3 orb : 1/3 thinking-window):
  - **Layer 0 — orb stage (top ~440px)**: the orb + its bloom + the optional window-rim state-glow. Nothing else competes here. State micro-label ("話しかけて") floats *under* the orb at very low contrast, fades when conversing.
  - **Layer 1 — thinking window (bottom ~240px)**: the 3 disclosures, but with **strict density limits** — only the *current* thought line (💭 1 line) + the *active/last* tool row (icon + name + target + status glyph ✓/spinner/✕) + a small confidence badge. *Max ~3 faint recent rows*, full history lives behind a "履歴" tap (brief). This is the koe-specific application of Granola "invisible until needed."
  - **Layer 2 — ambient meta (edges, persistent but quiet)**: cost line ("今月 $0.42 / 上限 $20.00") in caption-size muted text bottom-corner; settings gear top-corner. Both at low contrast, hover-surface (Big Sur pattern). Cost is *always visible but never loud* — it's a koe trust feature, like the thinking window.
  - **Layer 3 — modal overlays (front of orb, on demand)**: DANGER approval overlay **dims the orb stage behind a scrim** (Linear uses `#000000` overlay scrim) and presents "◯◯を実行してもいいですか? [許可][拒否]" + 30s countdown ring. Fail-closed = no-action defaults to 拒否. This is the ONLY time something covers the orb.
- **[C]** Approval/permission UX matters enormously: Codex's permission-grant flow was rated "the best I've ever seen in a third-party Mac app." A calm, legible single-decision modal beats a checklist. Source: https://www.macstories.net/notes/openais-new-codex-app-...
- **[I]** **Information-layering rule for koe**: only ONE layer may be "loud" at a time. Idle → orb loud, everything else faint. Conversing → orb loud + thinking-window mid. Working → thinking-window loud (tool row), orb is ambient. Approval → modal loud, all else dimmed. This single rule prevents the clutter the brief fears (ログをびっしり並べない).

---

## 6. ONBOARDING that feels like "a world igniting" (immersive)

- **[C]** Siri's "spring out of the Dynamic Island" + glow-rising-from-bottom = a precedent for **birth-from-a-point** motion. The brief explicitly wants "最初に世界が立ち上がる演出." Source: https://appleinsider.com/articles/26/05/28/...
- **[C]** Material 3 Expressive **overshoot/bounce springs are explicitly "for hero moments and key interactions where a more spirited feel is desired"** — i.e. onboarding is exactly where koe is *allowed* to break its calm-default and use a bouncy spring. Source: https://supercharge.design/blog/material-3-expressive
- **[C]** The convergence-particle motion ("収束していく粒子") the brief wants for connecting maps to Siri's assembly metaphor + mesh-orb construction (§1.5). Source: brief + https://randomgen.eu/mesh-gradient-generator/
- **[I] koe onboarding choreography** (2 steps, immersive):
  1. **Cold open**: dark/empty stage, a single dim point of light. As the user completes step ① (budget) and ② (BYOK key), **particles converge into the point → the orb ignites and takes its first breath** (use the Expressive overshoot spring here, once). The orb literally "comes alive" as setup completes — setup *is* the ignition, not a separate splash.
  2. **Trust-forward copy at the BYOK step**: the encrypted-on-device message (brief) shown calmly *as the orb forms* — the safety message and the world-ignition happen together, so security feels like part of the magic, not a legal speed-bump.
  3. After ignition, the orb settles into idle breathing → seamless handoff into the main screen (no separate "done" screen). The window doesn't *load*; it *wakes up*.
- **[I]** Keep onboarding to the brief's 2 steps; resist adding a feature tour. The orb igniting *is* the tour. (Granola's "zero learning curve" philosophy.)

---

## KEY ANTI-DILUTION GUARDRAILS (the discerning-synthesis note)

- **[I]** Codex App / Hermes / Perplexity PC / Raycast are **dev/command-center surfaces** — their *multi-panel, multi-session, file-browser, profiles, MCP-server-list* chrome is exactly what would **dilute** koe. Steal their **micro-craft** (cursor wiggle, wallpaper color, permission-flow polish, global-hotkey summon, invisible chrome) — do **NOT** steal their **information architecture** (sidebars, tabs, thread lists). koe = one orb, one thinking-window, one decision at a time.
- **[I]** koe's differentiator (calibrated transparency) must be **expressed in the visual language**, not bolted on: the confidence label and the tool-row are the *only* persistently-visible "data," and they should feel as crafted as the orb (premium typography, restrained color, ✓/spinner/✕ glyphs + text, never raw %). The orb is *presence*; the thinking-window is *the product's soul made legible*. Don't let it look like a debug log (the §3.2 Raycast-console trap the brief is escaping).
- **[I]** Avoid every confirmed AI-template tell: NO Inter primary, NO purple→blue gradient, NO uniform border-radius/spacing/duration, NO centered-flat radial glow (use off-center + grain), NO pure-white or blue-black canvas (use warm-tinted neutrals per Claude). These are all both koe-rule mandates *and* confirmed by the design-system sources above.

---

### Primary sources cited
Siri/Liquid-Glass: appleinsider.com (siri-redesign-ios-26), slashgear.com (iphone-glowing-edges), engadget.com (ios-26-wwdc). ChatGPT voice orb: help.openai.com (voice-mode-faq), aiforwardmarketer.com (blue-orb), community.openai.com (unresponsive thread), tomsguide.com (web voice mode). Voice-reactive orb impl: medium.com/@therealmilesjackson (react orb). Breathing CSS: css-tricks.com (apple-watch-breathe), uiverse.io (voice-assistant-orb). Mesh/grain: randomgen.eu (mesh-gradient-generator), jaconir.online (css-radial-gradient), css-tricks.com (grainy-gradients), tympanus.net/codrops (feTurbulence). Window/always-on: fazm.ai (native-swift-menu-bar), digitalfrontier.com + design-bootcamp Medium (Granola), winbuzzer.com + 9to5mac (Perplexity PC), tauri-apps GitHub discussion #4452 + issue #9439, v2.tauri.app (global-shortcut), dev.to/hiyoyok. Design systems: github.com/VoltAgent/awesome-design-md (claude, linear.app, raycast DESIGN.md). Codex craft: macstories.net (computer-use), developers.openai.com/codex/app. Typography JP: az-loc.com, jstockmedia.com, note.com/ababo. Motion: m3.material.io (easing-and-duration), supercharge.design (m3-expressive). Reduced-motion: w3.org/WAI/WCAG22 (C39), css-tricks.com (prefers-reduced-motion almanac), smashingmagazine.com (reduced-motion sensitivities).