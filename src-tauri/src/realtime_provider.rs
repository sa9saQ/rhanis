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

/// One server frame normalized across providers. [`RealtimeProvider::parse_frame`]
/// maps a provider's wire event (OpenAI `response.*`, later Gemini
/// `BidiGenerateContent`) to one of these so the session loop stays
/// provider-agnostic.
pub enum ProviderEvent {
    /// A tool call to dispatch (OpenAI: `response.function_call_arguments.done`).
    /// Carries raw, size-capped, UNPARSED arguments — see [`PendingCall`].
    FunctionCall(PendingCall),
    /// A usage report to add to the cost tracker (OpenAI: `response.done`).
    Usage(Usage),
    /// Server audio output. PR1's OpenAI impl never emits this — the
    /// `audio_handler` closure in `run_read_loop` already consumes
    /// `response.audio.delta`. Declared as the forward type contract for PR2
    /// (Gemini audio + 16 kHz integration).
    #[allow(dead_code)]
    AudioDelta,
    /// Transcript / ack / delta / unknown — the loop continues without action.
    Ignored,
}

// ---- provider trait ----------------------------------------------------------

/// A realtime voice provider. All methods are synchronous + pure: the socket,
/// the WS config, the read loop, cost and dispatch stay in `session_manager`.
/// The provider only supplies the handshake `Request`, the initial setup frames,
/// and a normalizer from one wire event to [`ProviderEvent`]. Stateless behind
/// `Arc<dyn RealtimeProvider>` → trivially object-safe (no `BoxFuture`).
pub trait RealtimeProvider: Send + Sync + 'static {
    /// Builds the WS upgrade request (URL + auth + any provider headers).
    fn build_request(&self, auth: &RealtimeAuth) -> Result<Request, &'static str>;
    /// Frames sent immediately after connect (tool advertisement + session config).
    fn initial_frames(&self, tools: &[ToolSchema]) -> Vec<Message>;
    /// Normalizes one decoded server frame into a [`ProviderEvent`].
    fn parse_frame(&self, event: &Value) -> ProviderEvent;
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
        // `session.update` advertising the dispatcher's tools so the model can
        // issue function calls (else the dispatch loop is permanently idle).
        let session_update = serde_json::json!({
            "type": "session.update",
            "session": { "tools": tools, "tool_choice": "auto" }
        });
        vec![Message::Text(session_update.to_string().into())]
    }

    fn parse_frame(&self, event: &Value) -> ProviderEvent {
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
                    return ProviderEvent::Ignored;
                }
                // Carry the arguments UNPARSED. `session_manager::handle_text`
                // parses them only after the MAX_INFLIGHT_DISPATCHES cap admits the
                // call, so a saturated burst is dropped without paying the per-frame
                // JSON parse (the pre-trait code checked the cap before parsing the
                // arguments — koe-wj2 DoS guard).
                ProviderEvent::FunctionCall(PendingCall {
                    call_id,
                    name,
                    args_raw: args_raw.to_string(),
                })
            }
            Some("response.done") => match parse_usage(event) {
                Some(usage) => ProviderEvent::Usage(usage),
                None => ProviderEvent::Ignored,
            },
            // Audio deltas are consumed by the `audio_handler` seam in the read
            // loop, so the normalized path ignores them (PR1). PR2 will route
            // Gemini audio through `ProviderEvent::AudioDelta`.
            Some("response.audio.delta") => ProviderEvent::Ignored,
            _ => ProviderEvent::Ignored,
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

    #[test]
    fn parse_frame_maps_function_call() {
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({
            "type": "response.function_call_arguments.done",
            "call_id": "c1", "name": "write_note", "arguments": "{\"text\":\"hi\"}"
        });
        match p.parse_frame(&ev) {
            ProviderEvent::FunctionCall(pending) => {
                assert_eq!(pending.call_id, "c1");
                assert_eq!(pending.name, "write_note");
                // args are carried raw (unparsed) until the dispatch cap admits.
                assert_eq!(pending.args_raw, "{\"text\":\"hi\"}");
            }
            _ => panic!("expected FunctionCall"),
        }
    }

    #[test]
    fn parse_frame_function_call_missing_fields_default() {
        // Missing call_id/name → empty strings; missing arguments → empty raw
        // string (the read loop falls back to null on parse failure). Mirrors the
        // pre-trait defaults.
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({ "type": "response.function_call_arguments.done" });
        match p.parse_frame(&ev) {
            ProviderEvent::FunctionCall(pending) => {
                assert_eq!(pending.call_id, "");
                assert_eq!(pending.name, "");
                assert_eq!(pending.args_raw, "");
            }
            _ => panic!("expected FunctionCall"),
        }
    }

    #[test]
    fn parse_frame_drops_oversized_args() {
        let p = OpenAiRealtime::new();
        let huge = "A".repeat(MAX_ARGS_LEN + 1);
        let ev = serde_json::json!({
            "type": "response.function_call_arguments.done",
            "call_id": "big", "name": "write_note", "arguments": huge
        });
        assert!(matches!(p.parse_frame(&ev), ProviderEvent::Ignored));
    }

    #[test]
    fn parse_frame_maps_response_done_to_usage() {
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({
            "type": "response.done",
            "response": { "usage": { "input_token_details": { "audio_tokens": 100 } } }
        });
        match p.parse_frame(&ev) {
            ProviderEvent::Usage(u) => assert_eq!(u.audio_input_tokens, 100),
            _ => panic!("expected Usage"),
        }
    }

    #[test]
    fn parse_frame_response_done_without_usage_is_ignored() {
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({ "type": "response.done" });
        assert!(matches!(p.parse_frame(&ev), ProviderEvent::Ignored));
    }

    #[test]
    fn parse_frame_audio_delta_is_ignored() {
        // The audio_handler seam consumes audio.delta; the normalized path skips it.
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({ "type": "response.audio.delta", "delta": "AAAA" });
        assert!(matches!(p.parse_frame(&ev), ProviderEvent::Ignored));
    }

    #[test]
    fn parse_frame_unknown_is_ignored() {
        let p = OpenAiRealtime::new();
        let ev = serde_json::json!({ "type": "response.created" });
        assert!(matches!(p.parse_frame(&ev), ProviderEvent::Ignored));
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
