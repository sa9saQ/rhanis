//! Voice connection layer (koe-zv3): the `RealtimeProvider` trait that lets the
//! session loop drive either OpenAI Realtime or (PR2) Google Gemini Live.
//!
//! PR1 abstracts the existing OpenAI path into [`OpenAiRealtime`] with
//! **identical behaviour** — every `session_manager` test stays green. The trait
//! methods are all synchronous + pure: the socket, the WS config (512 KiB DoS
//! cap), the read loop, cost tracking and dispatch all stay in `session_manager`
//! (transport / loop concerns), so the trait needs no `async`
//! (`BoxFuture`/`async-trait` not required — simpler than `DispatcherSeam`).
//!
//! ## Key discipline
//! [`RealtimeAuth`] lives here because it is the OpenAI Bearer credential shape
//! (Gemini's query-param key is a different form = PR2). The BYOK key is exposed
//! ONLY inside [`RealtimeAuth::bearer_header`] to build the `Authorization`
//! header — never stored, logged, or emitted — and `RealtimeAuth` is not
//! `Serialize`/`Clone` with a redacted `Debug`, so the key cannot leak through a
//! derived format or an event payload.
//!
//! transaction N/A · idempotency_key N/A (connection/credential types, not billing).

use std::fmt;
use std::sync::Arc;

use serde_json::Value;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::Request;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use crate::audio_bridge::MAX_ARGS_LEN;
use crate::cost_tracker::Usage;
use crate::realtime_types::ToolSchema;
use crate::secret_store::SecretString;

/// OpenAI Realtime WebSocket endpoint (gpt-realtime-2 GA model).
const REALTIME_URL: &str = "wss://api.openai.com/v1/realtime?model=gpt-realtime-2";

/// The ASR (input-transcription) model enabled in [`OpenAiRealtime::initial_frames`].
/// Token-billed (so its usage is meterable as tokens, not an opaque `duration`
/// shape — see [`parse_asr_usage`]), low-cost, and supports Japanese. It is billed
/// SEPARATELY from the realtime model; its usage is metered via [`parse_asr_usage`]
/// onto the same cost ledger (koe-pbe).
const ASR_MODEL: &str = "gpt-4o-mini-transcribe";

// ---- RealtimeAuth ------------------------------------------------------------

/// The connection credential. `Byok` is M1; `ManagedCredit` is a stub for M4
/// (operator-key prepaid). NOT `Serialize`/`Clone`; `Debug` is redacted so the
/// key cannot leak through a derived format.
pub enum RealtimeAuth {
    Byok(SecretString),
    /// M4 operator-key path; unused in M1.
    #[allow(dead_code)]
    ManagedCredit { token: SecretString },
}

impl RealtimeAuth {
    /// Builds the `Authorization: Bearer …` header. `expose()` is called ONLY
    /// here; the formatted string lives in this frame and drops immediately.
    fn bearer_header(&self) -> Result<HeaderValue, &'static str> {
        let secret = match self {
            RealtimeAuth::Byok(s) => s,
            RealtimeAuth::ManagedCredit { token } => token,
        };
        // Zeroize the intermediate "Bearer …" string so the key does not linger
        // in a second heap allocation after HeaderValue copies it. (The
        // HeaderValue's own bytes + the TLS write buffer remain an unavoidable
        // minimum exposure window for a BYOK desktop client.)
        let mut bearer = zeroize::Zeroizing::new(String::with_capacity(7 + secret.expose().len()));
        bearer.push_str("Bearer ");
        bearer.push_str(secret.expose());
        HeaderValue::from_str(&bearer).map_err(|_| "invalid credential")
    }
}

impl fmt::Debug for RealtimeAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RealtimeAuth::Byok(_) => f.write_str("RealtimeAuth::Byok(***)"),
            RealtimeAuth::ManagedCredit { .. } => f.write_str("RealtimeAuth::ManagedCredit(***)"),
        }
    }
}

// ---- normalized events -------------------------------------------------------

/// A function call whose arguments are size-capped but NOT yet JSON-parsed. The
/// read loop parses `args_raw` only AFTER the in-flight dispatch cap admits the
/// call, so a saturated burst is rejected without paying the per-frame parse —
/// the pre-trait code checked the cap before parsing arguments (koe-wj2 DoS
/// guard). Carrying the raw string keeps the size cap (applied in `parse_frame`)
/// while deferring the parse to the loop.
pub struct PendingCall {
    pub call_id: String,
    pub name: String,
    pub args_raw: String,
}

/// Which side of the conversation a [`ProviderEvent::Transcript`] turn came
/// from. Maps to the `role` column of a stored `ConversationEvent` (koe-emd) via
/// [`TranscriptRole::as_role_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptRole {
    /// The user's spoken input, transcribed by the server.
    User,
    /// The assistant's spoken output, transcribed by the server.
    Assistant,
}

impl TranscriptRole {
    /// The `role` string persisted by `RecorderAdapter::log_conversation_event`.
    /// Matches the values the existing sqlite tests already use ("user" /
    /// "assistant"), so the conversation log stays consistent across callers.
    pub fn as_role_str(self) -> &'static str {
        match self {
            TranscriptRole::User => "user",
            TranscriptRole::Assistant => "assistant",
        }
    }
}

/// One server frame normalized across providers. [`RealtimeProvider::parse_frame`]
/// maps a provider's wire event (OpenAI `response.*`, later Gemini
/// `BidiGenerateContent`) to one of these so the session loop stays
/// provider-agnostic.
pub enum ProviderEvent {
    /// A tool call to dispatch (OpenAI: `response.function_call_arguments.done`).
    /// Carries raw, size-capped, UNPARSED arguments — see [`PendingCall`].
    FunctionCall(PendingCall),
    /// A usage report to add to the cost tracker. Realtime-model usage arrives on
    /// `response.done`; the SEPARATELY-BILLED ASR (input-transcription) usage rides
    /// on `conversation.item.input_audio_transcription.completed` and is metered
    /// through this SAME variant (and the same `add_month_cost` ledger), so there is
    /// no second cost path to keep in sync (koe-pbe).
    Usage(Usage),
    /// A **finalized** speech transcript turn — user input (OpenAI:
    /// `conversation.item.input_audio_transcription.completed`) or assistant
    /// output (OpenAI GA: `response.output_audio_transcript.done`). Carries the
    /// finalized text only; streaming `.delta` events map to [`Ignored`] so each
    /// turn is journalled exactly once (no fragmented / double rows). The session
    /// loop persists it via the recorder (koe-emd). The user `.completed` frame also
    /// yields a [`Usage`] for its ASR cost (koe-pbe) — surfaced after this transcript
    /// so the turn is journalled before any budget gate.
    ///
    /// [`Ignored`]: ProviderEvent::Ignored
    /// [`Usage`]: ProviderEvent::Usage
    Transcript { role: TranscriptRole, text: String },
    /// Server audio output. PR1's OpenAI impl never emits this — the
    /// `audio_handler` closure in `run_read_loop` already consumes the
    /// audio-delta frames (GA `response.output_audio.delta` / beta
    /// `response.audio.delta`, see `audio_bridge::is_audio_delta_type`).
    /// Declared as the forward type contract for PR2 (Gemini audio + 16 kHz
    /// integration).
    #[allow(dead_code)]
    AudioDelta,
    /// The user started speaking (OpenAI: `input_audio_buffer.speech_started`,
    /// server VAD) — the barge-in trigger (koe-bx7). Two reactions, two seams:
    /// the audio side (cut local playback + suppress stale deltas) happens in
    /// [`PlaybackHandle::handle_server_audio`], which sees the same frame via the
    /// read loop's `audio_handler`; the protocol side — sending the provider's
    /// [`cancel_frame`] so the in-flight response stops generating server-side —
    /// is the session loop's job when it receives this event.
    ///
    /// Emitted on EVERY user speech start, not only mid-playback: the audio cut
    /// is a no-op on an empty sink, and a cancel without an active response is
    /// answered by a benign error frame (see [`cancel_frame`]).
    ///
    /// [`PlaybackHandle::handle_server_audio`]: crate::audio_bridge::PlaybackHandle::handle_server_audio
    /// [`cancel_frame`]: RealtimeProvider::cancel_frame
    SpeechStarted,
    /// Streaming transcript delta / ack / blank transcript / unknown — the loop
    /// continues without action (and records nothing).
    Ignored,
}

// ---- provider trait ----------------------------------------------------------

/// A realtime voice provider. All methods are synchronous + pure: the socket,
/// the WS config, the read loop, cost and dispatch stay in `session_manager`.
/// The provider only supplies the handshake `Request`, the initial setup frames,
/// and a normalizer from one wire event to zero or more [`ProviderEvent`]s.
/// Stateless behind `Arc<dyn RealtimeProvider>` → trivially object-safe (no
/// `BoxFuture`).
pub trait RealtimeProvider: Send + Sync + 'static {
    /// Builds the WS upgrade request (URL + auth + any provider headers).
    fn build_request(&self, auth: &RealtimeAuth) -> Result<Request, &'static str>;
    /// Frames sent immediately after connect (tool advertisement + session config).
    fn initial_frames(&self, tools: &[ToolSchema]) -> Vec<Message>;
    /// Normalizes one decoded server frame into zero or more [`ProviderEvent`]s.
    ///
    /// A single wire event can map to MORE THAN ONE normalized event: the user
    /// input-transcription `.completed` frame carries BOTH the finalized transcript
    /// AND a separately-billed ASR usage report, so it yields a `Transcript` *and* a
    /// `Usage` (koe-pbe). Every other frame yields a 0- or 1-element `Vec` — a
    /// negligible allocation next to the per-frame `serde_json` parse the read loop
    /// already paid, and it does NOT weaken the koe-wj2 function-call DoS guard: the
    /// in-flight cap check still runs in the loop AFTER this returns a 1-element
    /// `Vec`, and an over-cap `arguments` blob is still dropped here without parsing.
    fn parse_frame(&self, event: &Value) -> Vec<ProviderEvent>;
    /// The client frame that cancels the in-flight response (barge-in, koe-bx7).
    /// The session loop sends it when [`ProviderEvent::SpeechStarted`] arrives.
    ///
    /// Default `None`: a provider whose server handles interruption entirely on
    /// its own (or that has no cancel control) needs no frame. OpenAI overrides
    /// with `response.cancel` — sent UNGATED on every speech start because the
    /// client does not track response lifecycle; when no response is active
    /// (or the server's VAD `interrupt_response` already cancelled it) the
    /// server answers with a benign `error` frame that `parse_frame` maps to
    /// [`ProviderEvent::Ignored`]. koe-nal (error surfacing) must keep that
    /// duplicate-cancel error classified as benign.
    fn cancel_frame(&self) -> Option<Message> {
        None
    }
}

// ---- OpenAI Realtime ---------------------------------------------------------

/// OpenAI Realtime (gpt-realtime-2) provider. Zero-field: all per-session state
/// lives in `session_manager`; the impl is a stateless strategy object behind
/// `Arc<dyn RealtimeProvider>`.
#[derive(Default)]
pub struct OpenAiRealtime;

impl OpenAiRealtime {
    pub fn new() -> Self {
        Self
    }
}

impl RealtimeProvider for OpenAiRealtime {
    fn build_request(&self, auth: &RealtimeAuth) -> Result<Request, &'static str> {
        let mut request = REALTIME_URL
            .into_client_request()
            .map_err(|_| "invalid realtime url")?;
        let headers = request.headers_mut();
        headers.insert("Authorization", auth.bearer_header()?);
        // gpt-realtime-2 is a GA model; the current Realtime WebSocket docs drop
        // the `OpenAI-Beta: realtime=v1` header (it selected the now-superseded
        // beta interface). The exact handshake headers + server event shapes are
        // verified against the live API in koe-ef8 (Windows E2E).
        Ok(request)
    }

    fn initial_frames(&self, tools: &[ToolSchema]) -> Vec<Message> {
        // `session.update` advertising the dispatcher's tools so the model can issue
        // function calls (else the dispatch loop is permanently idle), AND enabling
        // user-speech transcription so the user half of the conversation log fills.
        //
        // koe-pbe: USER-speech transcripts require enabling input audio
        // transcription. The GA Realtime session config nests it at
        // `session.audio.input.transcription` (this is a PARTIAL `session.update`:
        // the server merges it, leaving other audio defaults — format /
        // turn_detection — untouched). That turns on a SEPARATELY-BILLED ASR model,
        // so it is COUPLED with metering the ASR usage that rides on
        // `conversation.item.input_audio_transcription.completed.usage` — see
        // [`parse_asr_usage`] and the `.completed` arm in [`parse_frame`]. Without
        // that metering the fail-closed monthly budget would leak (koe's core
        // invariant), which is exactly why this enable + the ASR metering ship in
        // ONE change. [`ASR_MODEL`] is token-billed so the usage is meterable; the
        // exact live usage field shape is pinned in koe-ef8 (Windows E2E).
        let session_update = serde_json::json!({
            "type": "session.update",
            "session": {
                "tools": tools,
                "tool_choice": "auto",
                "audio": { "input": { "transcription": { "model": ASR_MODEL } } }
            }
        });
        vec![Message::Text(session_update.to_string().into())]
    }

    fn cancel_frame(&self) -> Option<Message> {
        // `response.cancel` stops the in-flight response. Sent ungated on every
        // speech start (see the trait doc): with the GA default server VAD the
        // server has usually interrupted already, and a cancel with no active
        // response yields a benign `error` frame (Ignored today; koe-nal keeps
        // it benign). Static shape — no per-call state, mirrors initial_frames.
        Some(Message::Text(
            serde_json::json!({ "type": "response.cancel" }).to_string().into(),
        ))
    }

    fn parse_frame(&self, event: &Value) -> Vec<ProviderEvent> {
        match event.get("type").and_then(Value::as_str) {
            Some("response.function_call_arguments.done") => {
                let call_id = event
                    .get("call_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let name = event
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                // `arguments` arrives as a JSON-encoded string; enforce a size cap
                // on its raw length BEFORE keeping it so a crafted oversized blob
                // cannot consume unbounded allocator memory (DoS guard). Over-cap
                // frames are dropped (Ignored) — the model's call is intentionally
                // left unanswered, matching the pre-trait inline behaviour.
                let args_raw = event
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if args_raw.len() > MAX_ARGS_LEN {
                    eprintln!("[session] function-call arguments too large, dropping call");
                    return vec![ProviderEvent::Ignored];
                }
                // Carry the arguments UNPARSED. `session_manager::handle_text`
                // parses them only after the MAX_INFLIGHT_DISPATCHES cap admits the
                // call, so a saturated burst is dropped without paying the per-frame
                // JSON parse (the pre-trait code checked the cap before parsing the
                // arguments — koe-wj2 DoS guard).
                vec![ProviderEvent::FunctionCall(PendingCall {
                    call_id,
                    name,
                    args_raw: args_raw.to_string(),
                })]
            }
            // Realtime-model usage. A `response.done` WITHOUT a parseable `usage`
            // is currently Ignored (continue) — a known fail-OPEN gap tracked by
            // koe-2br, deliberately NOT hardened here: a `response.done` for a
            // cancelled/empty turn legitimately carries no usage ($0), so naively
            // fail-closing every usage-less `response.done` would spuriously stop
            // normal sessions. Distinguishing "absent = normal" from "malformed =
            // suspicious" needs the live payload shape, which koe-ef8 (Windows E2E)
            // pins; koe-2br does the fail-closed hardening once that data exists.
            // Out of scope for koe-pbe (ASR transcription/metering); behaviour here
            // is unchanged from before this PR.
            Some("response.done") => match parse_usage(event) {
                Some(usage) => vec![ProviderEvent::Usage(usage)],
                None => vec![ProviderEvent::Ignored],
            },
            // Audio deltas are consumed by the `audio_handler` seam in the read
            // loop, so the normalized path ignores them (PR1). This arm uses
            // the SAME `audio_bridge::is_audio_delta_type` predicate as that
            // seam, so it matches both wire names (GA
            // `response.output_audio.delta` / superseded beta
            // `response.audio.delta`, koe-bd7) and cannot drift from the
            // seam's match. Today this equals the `_ => Ignored` catch-all,
            // but pinning the names keeps audio frames out of any future
            // non-Ignored catch-all (e.g. unknown-frame logging). PR2 will
            // route Gemini audio through `ProviderEvent::AudioDelta`.
            Some(t) if crate::audio_bridge::is_audio_delta_type(t) => {
                vec![ProviderEvent::Ignored]
            }
            // Barge-in trigger (koe-bx7): the user began speaking (server VAD).
            // The audio cut happens in the `audio_handler` seam (the bridge sees
            // this same frame); the normalized event tells the session loop to
            // send `cancel_frame()`.
            Some("input_audio_buffer.speech_started") => vec![ProviderEvent::SpeechStarted],
            // Finalized user-speech transcription (koe-emd / koe-pbe). The matching
            // `.delta` stream falls through to `_ => Ignored`, so only the completed
            // turn is journalled. This frame carries BOTH the transcript AND a
            // SEPARATELY-BILLED ASR usage (OpenAI: "billed according to the ASR
            // model's pricing rather than the realtime model's"), so it normalizes to
            // up to TWO events: the transcript FIRST (so the turn is journalled
            // before a `Usage` budget gate could stop the loop), then the ASR usage —
            // metered through the SAME `ProviderEvent::Usage` / `add_month_cost`
            // ledger as realtime usage (no second cost path). Each is surfaced at
            // most once: the turn is recorded once and the ASR cost counted once (no
            // double-record / double-count).
            //
            // C-P2b (koe-ef8): ASR runs asynchronously, so `.completed` may arrive
            // BEFORE or AFTER the response it belongs to. The journal orders by row
            // id (arrival order), which can diverge from strict conversation order;
            // whether an item_id/response_id-based ordering (a `ConversationEvent`
            // schema migration) is warranted is decided from live traffic in koe-ef8,
            // NOT in this PR. `item_id` is intentionally not consumed here yet.
            Some("conversation.item.input_audio_transcription.completed") => {
                let mut events = Vec::new();
                if let Some(text) = transcript_text(event) {
                    events.push(ProviderEvent::Transcript {
                        role: TranscriptRole::User,
                        text,
                    });
                }
                if let Some(usage) = parse_asr_usage(event) {
                    events.push(ProviderEvent::Usage(usage));
                }
                events
            }
            // Finalized assistant-speech transcript (koe-emd). The GA Realtime
            // event is `response.output_audio_transcript.done`; the superseded
            // beta interface used `response.audio_transcript.done`. Match both so
            // the log fills regardless of which the live handshake selects (the
            // exact server event shape is confirmed in koe-ef8 Windows E2E). Both
            // carry the final text in `transcript`; the `.delta` stream is
            // ignored below so the turn is recorded once.
            Some("response.output_audio_transcript.done")
            | Some("response.audio_transcript.done") => match transcript_text(event) {
                Some(text) => vec![ProviderEvent::Transcript {
                    role: TranscriptRole::Assistant,
                    text,
                }],
                None => vec![],
            },
            _ => vec![ProviderEvent::Ignored],
        }
    }
}

/// Best-effort parse of an OpenAI Realtime usage payload into [`Usage`].
///
/// NOTE: the exact token-detail field names are confirmed against live traffic
/// in koe-ef8 (Windows E2E). Unknown fields default to 0, so an unexpected shape
/// under-counts rather than panicking — the session timeout is the backstop.
fn parse_usage(event: &Value) -> Option<Usage> {
    let u = event.get("response")?.get("usage")?;
    let input = u.get("input_token_details");
    let output = u.get("output_token_details");
    let get = |d: Option<&Value>, k: &str| -> u64 {
        d.and_then(|d| d.get(k)).and_then(Value::as_u64).unwrap_or(0)
    };
    Some(Usage {
        audio_input_tokens: get(input, "audio_tokens"),
        text_input_tokens: get(input, "text_tokens"),
        cached_input_tokens: get(input, "cached_tokens"),
        audio_output_tokens: get(output, "audio_tokens"),
        text_output_tokens: get(output, "text_tokens"),
    })
}

/// Best-effort parse of the ASR (input-transcription) usage carried on a
/// `conversation.item.input_audio_transcription.completed` frame into a [`Usage`],
/// or `None` when no token usage is present/parseable (absent block, a `duration`
/// usage we never select, or junk). Returning `None` records the transcript without
/// fabricating a no-op cost event, and this never panics — every field read is a
/// fallible `as_u64` that defaults out, so a malformed shape can only UNDER-surface,
/// never crash.
///
/// ## Conservative (fail-closed) rate mapping
/// ASR is billed at the ASR model's pricing, which is CHEAPER than the realtime
/// model. We deliberately bill ASR tokens at the REALTIME rates (audio_input /
/// text_input / text_output via [`Usage::cost_nanodollars`]) so we OVER-count,
/// never under-count: under-counting would leak the fail-closed monthly budget
/// (fail-open = real BYOK money), while over-counting can only trip a budget EARLY.
/// A dedicated (lower) ASR per-token rate is intentionally NOT introduced here;
/// pinning the live usage shape + a precise ASR rate is a koe-ef8 (Windows E2E)
/// follow-up. Mapping of the GA "tokens" usage shape:
///   - `input_token_details.audio_tokens` -> `audio_input_tokens` (realtime audio rate)
///   - `input_token_details.text_tokens`  -> `text_input_tokens`
///   - `output_tokens`                    -> `text_output_tokens`
///
/// The token counts are reconciled UP against BOTH coarse totals (`input_tokens`
/// and `total_tokens`) so a partial, mis-named, or coarse-only usage OVER-counts
/// rather than dropping cost: any token the server reports in a coarse field but
/// that the per-modality breakdown did not account for is billed at the audio rate.
/// So the metered total is always `>= total_tokens` (and `>= input_tokens`) = never
/// under-count (fail-closed) — this is the koe-pbe R-C hardening. Only a usage with
/// NO numeric token field anywhere (absent block, a `duration` usage we never
/// select, or junk) meters nothing; that residual is backstopped by the realtime
/// model's own audio input on `response.done` + the session timeout, and the live
/// shape is pinned in koe-ef8 (which also confirms integer typing on the wire).
fn parse_asr_usage(event: &Value) -> Option<Usage> {
    let u = event.get("usage")?;
    let as_u64 = |v: Option<&Value>| v.and_then(Value::as_u64).unwrap_or(0);
    let details = u.get("input_token_details");
    let audio_in = as_u64(details.and_then(|d| d.get("audio_tokens")));
    let text_in = as_u64(details.and_then(|d| d.get("text_tokens")));
    let coarse_in = as_u64(u.get("input_tokens"));
    let output = as_u64(u.get("output_tokens"));
    let total = as_u64(u.get("total_tokens"));

    // Reconcile UP twice so a PARTIAL / mis-named / coarse-only usage OVER-counts
    // rather than dropping ASR cost (fail-closed; never bill less than what the
    // server reported in ANY field — koe-pbe R-C / Codex HIGH):
    //   1. coarse `input_tokens`: a remainder beyond the per-modality breakdown
    //      (a partial breakdown with only one sub-field, or an absent one) is billed
    //      at the audio input rate.
    //   2. `total_tokens`: any token in the coarse total NOT yet accounted for by the
    //      breakdown + output (e.g. a missing `output_tokens`, or our field-name
    //      guesses turn out wrong — koe-ef8 pins the live shape) is also billed at the
    //      audio input rate. ASR is far cheaper than the realtime audio rate, so
    //      bucketing any unclassified remainder there is a safe over-count.
    // (Token fields are read as integers; a non-integer field reads as 0 — the live
    // wire is integer-typed per the GA `tokens` shape, pinned in koe-ef8.)
    let text_input_tokens = text_in;
    let text_output_tokens = output;
    let input_remainder = coarse_in.saturating_sub(audio_in.saturating_add(text_in));
    let mut audio_input_tokens = audio_in.saturating_add(input_remainder);
    let accounted = audio_input_tokens
        .saturating_add(text_input_tokens)
        .saturating_add(text_output_tokens);
    audio_input_tokens = audio_input_tokens.saturating_add(total.saturating_sub(accounted));

    let usage = Usage {
        audio_input_tokens,
        text_input_tokens,
        text_output_tokens,
        ..Default::default()
    };
    // No meterable token counts (e.g. a `duration` usage — only emitted for models
    // we never select — or junk, or an all-zero turn) → record the transcript and
    // meter nothing. The realtime model's own audio input (response.done) + the
    // 30-min session timeout are the backstops; the live shape is pinned in koe-ef8.
    // Skipping an all-zero usage also avoids a no-op cost-update (new sequence, zero
    // spend) on the UI.
    if usage == Usage::default() {
        return None;
    }
    Some(usage)
}

/// The finalized `transcript` text of a transcript-bearing frame, or `None` when
/// it is missing or blank — so a silent / empty turn never becomes an empty
/// conversation-log row (koe-emd). The text is kept verbatim (only its
/// non-blankness is checked); its size is already bounded by the
/// `MAX_WS_TEXT_BYTES` frame cap applied in `run_read_loop` before parsing.
fn transcript_text(event: &Value) -> Option<String> {
    match event.get("transcript").and_then(Value::as_str) {
        Some(text) if !text.trim().is_empty() => Some(text.to_string()),
        _ => None,
    }
}

// ---- provider selection ------------------------------------------------------

/// Resolves the persisted `voice_provider_model` to a provider impl by matching
/// the FULL `"provider/model"` string. `"openai/gpt-realtime-2"` is the only model
/// PR1 wires; `"google/gemini-2.5-flash-live"` is a typed "not yet supported"
/// error (PR2); everything else is rejected — including an `"openai/<other>"`
/// that would otherwise silently connect to `build_request`'s fixed
/// gpt-realtime-2 endpoint, and path tricks like `"openai/../google"`.
/// settings_store's `KNOWN_VOICE_PROVIDER_MODELS` already restricts the persisted
/// value on load; this is the defense-in-depth boundary at session start. PR2
/// adds Gemini and lets `OpenAiRealtime` carry the model name for more OpenAI
/// models.
pub fn select_provider(voice_provider_model: &str) -> Result<Arc<dyn RealtimeProvider>, String> {
    match voice_provider_model {
        "openai/gpt-realtime-2" => Ok(Arc::new(OpenAiRealtime::new())),
        "google/gemini-2.5-flash-live" => Err("voice provider not yet supported".to_string()),
        _ => Err("unknown voice provider".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- RealtimeAuth redaction (moved from session_manager) -----------------

    #[test]
    fn realtime_auth_debug_is_redacted() {
        let auth = RealtimeAuth::Byok(SecretString::new("sk-supersecret".into()));
        let dbg = format!("{auth:?}");
        assert!(!dbg.contains("supersecret"), "Debug must not leak the key");
        assert_eq!(dbg, "RealtimeAuth::Byok(***)");
    }

    #[test]
    fn bearer_header_carries_the_key_but_auth_does_not_serialize() {
        let auth = RealtimeAuth::Byok(SecretString::new("sk-abc".into()));
        let h = auth.bearer_header().expect("header");
        assert_eq!(h.to_str().unwrap(), "Bearer sk-abc");
        // (compile-time) RealtimeAuth has no Serialize/Clone derive — see type def.
    }

    // ---- build_request: key exposure window ----------------------------------

    #[test]
    fn build_request_carries_authorization_and_url() {
        let p = OpenAiRealtime::new();
        let auth = RealtimeAuth::Byok(SecretString::new("sk-xyz".into()));
        let req = p.build_request(&auth).expect("request");
        assert_eq!(
            req.headers()
                .get("Authorization")
                .expect("authorization header")
                .to_str()
                .unwrap(),
            "Bearer sk-xyz"
        );
        assert_eq!(req.uri().host(), Some("api.openai.com"));
    }

    // ---- initial_frames (was session_update_includes_tools) ------------------

    #[test]
    fn initial_frames_advertise_tools() {
        let p = OpenAiRealtime::new();
        let tools = vec![ToolSchema {
            kind: "function".into(),
            name: "write_note".into(),
            description: "save a note".into(),
            parameters: serde_json::json!({ "type": "object" }),
        }];
        let frames = p.initial_frames(&tools);
        assert_eq!(frames.len(), 1);
        let Message::Text(t) = &frames[0] else {
            panic!("expected a text frame");
        };
        let v: Value = serde_json::from_str(t.as_str()).unwrap();
        assert_eq!(v["type"], "session.update");
        assert_eq!(v["session"]["tools"][0]["name"], "write_note");
        assert_eq!(v["session"]["tool_choice"], "auto");
    }

    #[test]
    fn initial_frames_enables_input_audio_transcription() {
        // koe-pbe: the GA Realtime session config nests transcription at
        // `session.audio.input.transcription.model`. Without this the server never
        // emits `conversation.item.input_audio_transcription.completed`, so the
        // user half of the conversation log stays empty in production (dormant).
        // The model must be the token-billed `gpt-4o-mini-transcribe` (so its usage
        // is meterable as tokens, not an opaque duration) — see parse_asr_usage.
        let p = OpenAiRealtime::new();
        let frames = p.initial_frames(&[]);
        let Message::Text(t) = &frames[0] else {
            panic!("expected a text frame");
        };
        let v: Value = serde_json::from_str(t.as_str()).unwrap();
        assert_eq!(
            v["session"]["audio"]["input"]["transcription"]["model"],
            "gpt-4o-mini-transcribe",
            "input audio transcription must be enabled with the token-billed model"
        );
        // The tools advertisement must survive the merge (same session.update).
        assert_eq!(v["session"]["tool_choice"], "auto");
    }

    // ---- parse_usage (moved from session_manager) ----------------------------

    #[test]
    fn parse_usage_extracts_token_details() {
        let event = serde_json::json!({
            "type": "response.done",
            "response": { "usage": {
                "input_token_details": { "audio_tokens": 100, "text_tokens": 10, "cached_tokens": 5 },
                "output_token_details": { "audio_tokens": 200, "text_tokens": 20 }
            }}
        });
        let u = parse_usage(&event).expect("usage");
        assert_eq!(u.audio_input_tokens, 100);
        assert_eq!(u.text_input_tokens, 10);
        assert_eq!(u.cached_input_tokens, 5);
        assert_eq!(u.audio_output_tokens, 200);
        assert_eq!(u.text_output_tokens, 20);
    }

    #[test]
    fn parse_usage_missing_usage_is_none() {
        assert!(parse_usage(&serde_json::json!({ "type": "response.done" })).is_none());
    }

    // ---- parse_frame normalization -------------------------------------------
    //
    // parse_frame now returns a `Vec<ProviderEvent>` (a frame can normalize to more
    // than one event — see the user `.completed` ASR case below). Single-event
    // frames assert on a one-element slice; "nothing to surface" frames assert an
    // empty slice.

    #[test]
    fn parse_frame_maps_function_call() {
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({
            "type": "response.function_call_arguments.done",
            "call_id": "c1", "name": "write_note", "arguments": "{\"text\":\"hi\"}"
        });
        let evs = p.parse_frame(&ev);
        let [ProviderEvent::FunctionCall(pending)] = evs.as_slice() else {
            panic!("expected a single FunctionCall, got {} events", evs.len());
        };
        assert_eq!(pending.call_id, "c1");
        assert_eq!(pending.name, "write_note");
        // args are carried raw (unparsed) until the dispatch cap admits.
        assert_eq!(pending.args_raw, "{\"text\":\"hi\"}");
    }

    #[test]
    fn parse_frame_function_call_missing_fields_default() {
        // Missing call_id/name → empty strings; missing arguments → empty raw
        // string (the read loop falls back to null on parse failure). Mirrors the
        // pre-trait defaults.
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({ "type": "response.function_call_arguments.done" });
        let evs = p.parse_frame(&ev);
        let [ProviderEvent::FunctionCall(pending)] = evs.as_slice() else {
            panic!("expected a single FunctionCall, got {} events", evs.len());
        };
        assert_eq!(pending.call_id, "");
        assert_eq!(pending.name, "");
        assert_eq!(pending.args_raw, "");
    }

    #[test]
    fn parse_frame_drops_oversized_args() {
        let p = OpenAiRealtime::new();
        let huge = "A".repeat(MAX_ARGS_LEN + 1);
        let ev = serde_json::json!({
            "type": "response.function_call_arguments.done",
            "call_id": "big", "name": "write_note", "arguments": huge
        });
        assert!(matches!(p.parse_frame(&ev).as_slice(), [ProviderEvent::Ignored]));
    }

    #[test]
    fn parse_frame_maps_response_done_to_usage() {
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 100 } } }
        });
        let evs = p.parse_frame(&ev);
        let [ProviderEvent::Usage(u)] = evs.as_slice() else {
            panic!("expected a single Usage, got {} events", evs.len());
        };
        assert_eq!(u.audio_input_tokens, 100);
    }

    #[test]
    fn parse_frame_response_done_without_usage_is_ignored() {
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({ "type": "response.done" });
        assert!(matches!(p.parse_frame(&ev).as_slice(), [ProviderEvent::Ignored]));
    }

    #[test]
    fn parse_frame_audio_delta_is_ignored() {
        // The audio_handler seam consumes audio.delta; the normalized path skips it.
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({ "type": "response.audio.delta", "delta": "AAAA" });
        assert!(matches!(p.parse_frame(&ev).as_slice(), [ProviderEvent::Ignored]));
    }

    #[test]
    fn parse_frame_ga_audio_delta_is_ignored() {
        // GA wire name (koe-bd7): like the beta name above, the GA
        // `response.output_audio.delta` is consumed by the audio_handler seam;
        // the normalized path must keep ignoring it (pinned explicitly so a
        // future non-Ignored catch-all cannot change this silently).
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({ "type": "response.output_audio.delta", "delta": "AAAA" });
        assert!(matches!(p.parse_frame(&ev).as_slice(), [ProviderEvent::Ignored]));
    }

    #[test]
    fn parse_frame_unknown_is_ignored() {
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({ "type": "response.created" });
        assert!(matches!(p.parse_frame(&ev).as_slice(), [ProviderEvent::Ignored]));
    }

    // ---- barge-in (koe-bx7) ----------------------------------------------------

    #[test]
    fn parse_frame_maps_speech_started() {
        // The server-VAD speech-start frame is the barge-in trigger: exactly one
        // normalized SpeechStarted, regardless of extra fields on the wire event.
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({
            "type": "input_audio_buffer.speech_started",
            "audio_start_ms": 1234,
            "item_id": "item_1"
        });
        assert!(matches!(p.parse_frame(&ev).as_slice(), [ProviderEvent::SpeechStarted]));
    }

    #[test]
    fn cancel_frame_is_response_cancel() {
        let p = OpenAiRealtime::new();
        let Some(Message::Text(txt)) = p.cancel_frame() else {
            panic!("expected a text response.cancel frame");
        };
        let v: serde_json::Value = serde_json::from_str(txt.as_str()).unwrap();
        assert_eq!(v["type"], "response.cancel");
    }

    // ---- transcript (koe-emd) ------------------------------------------------

    #[test]
    fn transcript_role_maps_to_recorder_role_string() {
        // The stored `role` must match the values the sqlite tests already use.
        assert_eq!(TranscriptRole::User.as_role_str(), "user");
        assert_eq!(TranscriptRole::Assistant.as_role_str(), "assistant");
    }

    #[test]
    fn parse_frame_maps_user_input_transcription_completed() {
        // A `.completed` frame WITHOUT a usage block yields just the transcript
        // (the ASR usage is metered only when present — see the ASR tests below).
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "transcript": "do a web search please"
        });
        let evs = p.parse_frame(&ev);
        let [ProviderEvent::Transcript { role, text }] = evs.as_slice() else {
            panic!("expected a single user Transcript, got {} events", evs.len());
        };
        assert_eq!(*role, TranscriptRole::User);
        assert_eq!(text, "do a web search please");
    }

    #[test]
    fn parse_frame_maps_assistant_transcript_ga_event() {
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({
            "type": "response.output_audio_transcript.done",
            "transcript": "here are the results"
        });
        let evs = p.parse_frame(&ev);
        let [ProviderEvent::Transcript { role, text }] = evs.as_slice() else {
            panic!("expected a single assistant Transcript, got {} events", evs.len());
        };
        assert_eq!(*role, TranscriptRole::Assistant);
        assert_eq!(text, "here are the results");
    }

    #[test]
    fn parse_frame_maps_assistant_transcript_beta_event() {
        // The superseded beta interface used `response.audio_transcript.done`;
        // matched as a fallback so the log fills regardless of the handshake.
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({
            "type": "response.audio_transcript.done",
            "transcript": "beta path reply"
        });
        let evs = p.parse_frame(&ev);
        let [ProviderEvent::Transcript { role, text }] = evs.as_slice() else {
            panic!("expected a single assistant Transcript, got {} events", evs.len());
        };
        assert_eq!(*role, TranscriptRole::Assistant);
        assert_eq!(text, "beta path reply");
    }

    #[test]
    fn parse_frame_blank_or_missing_transcript_is_ignored() {
        // A silent / empty turn must NOT produce an empty conversation-log row.
        // A blank transcript with no usage normalizes to NO events (empty slice).
        let p = OpenAiRealtime::new();
        for ev in [
            serde_json::json!({
                "type": "conversation.item.input_audio_transcription.completed",
                "transcript": "   "
            }),
            serde_json::json!({
                "type": "response.output_audio_transcript.done",
                "transcript": ""
            }),
            serde_json::json!({ "type": "response.output_audio_transcript.done" }),
        ] {
            assert!(
                p.parse_frame(&ev).is_empty(),
                "blank/missing transcript must surface no events"
            );
        }
    }

    #[test]
    fn parse_frame_transcript_delta_is_ignored() {
        // Streaming deltas are skipped so each turn is journalled exactly once
        // (only the `.completed` / `.done` finalized event records).
        let p = OpenAiRealtime::new();
        for ev in [
            serde_json::json!({
                "type": "conversation.item.input_audio_transcription.delta",
                "delta": "do a"
            }),
            serde_json::json!({
                "type": "response.audio_transcript.delta",
                "delta": "here"
            }),
            serde_json::json!({
                "type": "response.output_audio_transcript.delta",
                "delta": "here"
            }),
        ] {
            assert!(matches!(p.parse_frame(&ev).as_slice(), [ProviderEvent::Ignored]));
        }
    }

    // ---- ASR usage on the user `.completed` frame (koe-pbe) -------------------

    /// A canonical GA token-usage `.completed` frame (per OpenAI Realtime docs):
    /// the user transcript PLUS an ASR usage block billed at the ASR model's rate.
    fn user_completed_with_usage(transcript: &str) -> Value {
        serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "item_id": "item_1",
            "content_index": 0,
            "transcript": transcript,
            "usage": {
                "type": "tokens",
                "total_tokens": 22,
                "input_tokens": 13,
                "input_token_details": { "text_tokens": 0, "audio_tokens": 13 },
                "output_tokens": 9
            }
        })
    }

    #[test]
    fn parse_frame_user_completed_surfaces_transcript_and_asr_usage() {
        // koe-pbe: the user `.completed` frame carries BOTH the transcript AND a
        // separately-billed ASR usage. parse_frame must surface both, transcript
        // FIRST (so it is journalled before a budget gate could stop the loop),
        // usage second — each exactly once (no double-record / double-count).
        let p = OpenAiRealtime::new();
        let evs = p.parse_frame(&user_completed_with_usage("search the web for rust"));
        let [ProviderEvent::Transcript { role, text }, ProviderEvent::Usage(u)] = evs.as_slice()
        else {
            panic!(
                "expected [Transcript, Usage] from the ASR .completed frame, got {} events",
                evs.len()
            );
        };
        assert_eq!(*role, TranscriptRole::User);
        assert_eq!(text, "search the web for rust");
        // 13 audio input tokens + 9 output (text) tokens are surfaced.
        assert!(u.cost_nanodollars() > 0, "ASR usage must meter a non-zero cost");
    }

    #[test]
    fn asr_usage_is_conservatively_over_counted() {
        // The ASR model (gpt-4o-mini-transcribe) is cheaper than the realtime model,
        // but we bill its tokens at the realtime rates (audio_input / text_output)
        // so we OVER-count, never under-count = fail-closed (a budget can only trip
        // EARLY, never leak). Mapping: ASR audio_tokens -> audio_input_tokens,
        // ASR text input -> text_input_tokens, ASR output_tokens -> text_output_tokens.
        let p = OpenAiRealtime::new();
        let evs = p.parse_frame(&user_completed_with_usage("hello"));
        let [_, ProviderEvent::Usage(u)] = evs.as_slice() else {
            panic!("expected a Usage event on the ASR .completed frame");
        };
        assert_eq!(u.audio_input_tokens, 13, "ASR audio tokens billed as audio input");
        assert_eq!(u.text_input_tokens, 0);
        assert_eq!(u.text_output_tokens, 9, "ASR output tokens billed as text output");
        assert_eq!(u.audio_output_tokens, 0);
        assert_eq!(u.cached_input_tokens, 0);
        // The metered cost equals the conservative realtime-rate mapping exactly.
        let expected = crate::cost_tracker::Usage {
            audio_input_tokens: 13,
            text_output_tokens: 9,
            ..Default::default()
        }
        .cost_nanodollars();
        assert_eq!(u.cost_nanodollars(), expected);
    }

    #[test]
    fn asr_usage_reconciles_partial_breakdown_against_coarse_input() {
        // Fail-closed defense: a PARTIAL `input_token_details` (only audio_tokens,
        // text_tokens missing) whose sum is LESS than the coarse `input_tokens` must
        // still bill the remainder (at the highest input rate = audio), never drop
        // it — the metered input is >= input_tokens (over-count), never under-count.
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "transcript": "hello",
            "usage": {
                "type": "tokens",
                "total_tokens": 25,
                "input_tokens": 20, // 7 more than the breakdown's 13
                "input_token_details": { "audio_tokens": 13 }, // text_tokens absent
                "output_tokens": 5
            }
        });
        let evs = p.parse_frame(&ev);
        let [_, ProviderEvent::Usage(u)] = evs.as_slice() else {
            panic!("expected a Usage event on the ASR .completed frame");
        };
        // 13 detail audio + 7 unaccounted remainder = 20, all billed as audio input.
        assert_eq!(u.audio_input_tokens, 20, "remainder billed = no under-count");
        assert_eq!(u.text_input_tokens, 0);
        assert_eq!(u.text_output_tokens, 5);
    }

    #[test]
    fn asr_usage_reconciles_against_total_tokens_no_undercount() {
        // koe-pbe R-C (Codex HIGH): a usage missing `output_tokens` (or with mis-named
        // fields) but reporting a larger `total_tokens` must STILL bill the
        // unaccounted tokens (at the audio rate), never silently drop them.
        let p = OpenAiRealtime::new();

        // (a) output_tokens absent, but total_tokens implies 10 unaccounted tokens.
        let ev = serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "transcript": "hi",
            "usage": {
                "type": "tokens",
                "total_tokens": 30,
                "input_tokens": 20,
                "input_token_details": { "audio_tokens": 20, "text_tokens": 0 }
                // output_tokens deliberately absent
            }
        });
        let evs = p.parse_frame(&ev);
        let [_, ProviderEvent::Usage(u)] = evs.as_slice() else {
            panic!("expected a Usage event");
        };
        // 20 input audio + 10 unaccounted (30 total - 20 input) billed as audio input.
        assert_eq!(u.audio_input_tokens, 30, "unaccounted total billed = no under-count");
        assert_eq!(u.text_input_tokens, 0);
        assert_eq!(u.text_output_tokens, 0);

        // (b) only `total_tokens` present (e.g. our breakdown field names are wrong):
        // the whole total is still metered at the audio rate, not dropped.
        let ev2 = serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "transcript": "hi",
            "usage": { "type": "tokens", "total_tokens": 50 }
        });
        let evs2 = p.parse_frame(&ev2);
        let [_, ProviderEvent::Usage(u2)] = evs2.as_slice() else {
            panic!("expected a Usage event from total_tokens-only usage");
        };
        assert_eq!(u2.audio_input_tokens, 50, "total_tokens-only is metered, not dropped");
        assert!(u2.cost_nanodollars() > 0);
    }

    #[test]
    fn parse_frame_user_completed_without_usage_is_transcript_only() {
        // Until the ASR usage block arrives (older shape / not sent), the transcript
        // still records. No usage = no metered cost for that turn — the realtime
        // model's own audio input (response.done) + the 30-min session timeout are
        // the backstops; the exact live shape is pinned in koe-ef8.
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "transcript": "no usage block here"
        });
        let evs = p.parse_frame(&ev);
        assert!(
            matches!(evs.as_slice(), [ProviderEvent::Transcript { role: TranscriptRole::User, .. }]),
            "transcript without usage records the turn but meters no cost"
        );
    }

    #[test]
    fn parse_frame_user_completed_malformed_usage_is_fail_closed() {
        // A present-but-unparseable usage block (wrong shape, a `duration` usage we
        // never select, or junk) must NOT panic and must NOT silently fabricate a
        // zero Usage event that re-emits a no-op cost-update. The transcript still
        // records; the ASR cost for that turn is simply not metered (backstopped),
        // and koe-ef8 pins the live shape so this path is not hit in practice.
        let p = OpenAiRealtime::new();
        for usage in [
            serde_json::json!("not an object"),
            serde_json::json!({ "type": "duration", "seconds": 1.5 }),
            serde_json::json!({ "type": "tokens" }), // no token counts at all
            serde_json::json!({ "input_token_details": "wrong type" }),
        ] {
            let ev = serde_json::json!({
                "type": "conversation.item.input_audio_transcription.completed",
                "transcript": "still recorded",
                "usage": usage
            });
            let evs = p.parse_frame(&ev);
            assert!(
                matches!(
                    evs.as_slice(),
                    [ProviderEvent::Transcript { role: TranscriptRole::User, .. }]
                ),
                "malformed usage must record the transcript only (no Usage, no panic)"
            );
        }
    }

    // ---- select_provider -----------------------------------------------------

    #[test]
    fn select_provider_openai_ok() {
        assert!(select_provider("openai/gpt-realtime-2").is_ok());
    }

    #[test]
    fn select_provider_google_is_not_yet_supported() {
        // `unwrap_err()` would require `dyn RealtimeProvider: Debug`; match instead.
        match select_provider("google/gemini-2.5-flash-live") {
            Err(e) => assert!(e.contains("not yet supported"), "got: {e}"),
            Ok(_) => panic!("expected google to be unsupported in PR1"),
        }
    }

    #[test]
    fn select_provider_unknown_is_rejected() {
        assert!(select_provider("evil/model").is_err());
        assert!(select_provider("").is_err());
    }

    #[test]
    fn select_provider_rejects_other_openai_models_and_path_tricks() {
        // PR1 wires only gpt-realtime-2; an "openai/<other>" must NOT silently
        // connect to it (build_request's URL is fixed), and a prefix/path trick
        // must not route to OpenAI.
        assert!(select_provider("openai/gpt-4o").is_err());
        assert!(select_provider("openai/../google").is_err());
        assert!(select_provider("openai").is_err());
    }
}
