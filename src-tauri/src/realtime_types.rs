//! Shared kernel for the Realtime function-call dispatch seam (rhanis-2gy + rhanis-e3m).
//!
//! Owned by neither `session_manager` nor `tool_dispatcher` so the two connect
//! through a trait with **no import cycle**:
//!
//! ```text
//! session_manager → realtime_types   (invokes DispatcherSeam via ManagedDispatcher)
//! tool_dispatcher  → realtime_types   (implements DispatcherSeam)
//! ```
//!
//! Neither module imports the other. `lib.rs` is the single wiring point: it
//! registers `NoopDispatcher` until rhanis-2gy swaps in the real one.
//!
//! `BoxFuture` is a local `std`-only type alias (no `futures` crate dependency).
//!
//! transaction N/A · idempotency_key N/A (in-process routing types, not billing).

// The dispatch seam has no in-crate production caller until session_manager
// (rhanis-e3m) wires its read loop to `RealToolDispatcher::dispatch`; the trait and
// the no-op are fully exercised by this module's and the dispatcher's tests.
// Allow dead_code module-wide until rhanis-e3m lands, then drop this so any
// genuinely-unused item resurfaces.
#![allow(dead_code)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

/// A heap-pinned, `Send` future — the object-safe return shape for the async
/// trait method below (`async fn` in traits is not yet `dyn`-compatible, so we
/// box explicitly). `'static` because the future is moved into a spawned task.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One decoded `response.function_call_arguments.done` event from the Realtime
/// API. `call_id` is the Realtime call id and equals `ToolEvent.actionId` on the
/// frontend (`src/features/activity/types.ts`).
#[derive(Debug, Clone)]
pub struct FunctionCall {
    pub call_id: String,
    pub name: String,
    pub args: Value,
}

/// The two frames `session_manager` sends back over the socket after a dispatch:
/// the tool-output item, then the follow-up response trigger. Building the JSON
/// in the dispatcher keeps the WS layer dumb (it just forwards bytes).
#[derive(Debug, Clone)]
pub struct DispatchResult {
    pub conversation_item_create: Value,
    pub response_create: Value,
}

/// A tool's function-calling schema, serialized into the `session.update` tools
/// array so the model knows the tool exists. `parameters` is a JSON Schema
/// object. `kind` serializes as `"type"` and is `"function"` for all M1 tools —
/// the Realtime format requires each callable entry to carry `type: "function"`.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSchema {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// The seam by which `session_manager` (rhanis-e3m) invokes the tool dispatcher
/// (rhanis-2gy) without knowing its concrete type — defined here to break the
/// import cycle.
///
/// `Send + Sync + 'static` because the implementor lives behind
/// `Arc<dyn DispatcherSeam>` in Tauri managed state and is cloned into spawned
/// tokio tasks. `dispatch` returns a `BoxFuture<'static>` that does **not**
/// borrow `&self`, so the implementor must clone the owned state it needs
/// (the `Arc`s) *before* the `Box::pin(async move { … })`.
pub trait DispatcherSeam: Send + Sync + 'static {
    /// Runs one function call to completion (classify → SAFE/CAUTION run now /
    /// DANGER gate → execute → emit a redacted `tool-event`) and returns the
    /// frames to send back. A tool error is encoded as a `function_call_output`
    /// with an error body — the caller always sends both frames.
    fn dispatch(&self, call: FunctionCall) -> BoxFuture<'static, DispatchResult>;

    /// Tool schemas to advertise in `session.update`. Empty for the no-op.
    fn tool_schemas(&self) -> Vec<ToolSchema>;
}

/// Builds the standard `conversation.item.create` + `response.create` pair for a
/// given call id and already-serialized output string. Shared so the real
/// dispatcher and the no-op produce byte-identical envelopes.
///
/// `output` MUST already be a JSON-encoded **string** (Realtime requires the
/// `output` field to be a string) and MUST already be redacted/size-bounded by
/// the caller — this helper does not inspect it.
pub fn function_call_output(call_id: &str, output: String) -> DispatchResult {
    DispatchResult {
        conversation_item_create: serde_json::json!({
            "type": "conversation.item.create",
            "item": {
                "type": "function_call_output",
                "call_id": call_id,
                "output": output,
            }
        }),
        response_create: serde_json::json!({ "type": "response.create" }),
    }
}

/// The pre-rhanis-2gy default and the test double: accepts a function call but runs
/// no tool, replying with a fixed error output. Registered in `lib.rs` until the
/// real dispatcher lands, so the conversation loop stays **functional** (a tool
/// call still gets a well-formed reply and the model can continue) rather than
/// dead. The no-op is exercised by rhanis-e3m (its pre-rebase default) and tests;
/// rhanis-2gy's production wiring uses the real dispatcher.
pub struct NoopDispatcher;

impl DispatcherSeam for NoopDispatcher {
    fn dispatch(&self, call: FunctionCall) -> BoxFuture<'static, DispatchResult> {
        Box::pin(async move {
            function_call_output(
                &call.call_id,
                "{\"error\":\"tool dispatch not yet available\"}".to_string(),
            )
        })
    }

    fn tool_schemas(&self) -> Vec<ToolSchema> {
        Vec::new()
    }
}

/// Tauri managed-state wrapper. `lib.rs` registers a `NoopDispatcher` until
/// rhanis-2gy swaps in the real `RealToolDispatcher`; `session_manager` (rhanis-e3m)
/// reads it via `tauri::State<'_, ManagedDispatcher>` and clones the inner `Arc`
/// into its read loop.
pub struct ManagedDispatcher(pub Arc<dyn DispatcherSeam>);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_dispatcher_returns_well_formed_error_frames() {
        let call = FunctionCall {
            call_id: "call_123".into(),
            name: "anything".into(),
            args: serde_json::json!({ "x": 1 }),
        };
        let res = NoopDispatcher.dispatch(call).await;

        assert_eq!(
            res.conversation_item_create["type"],
            "conversation.item.create"
        );
        let item = &res.conversation_item_create["item"];
        assert_eq!(item["type"], "function_call_output");
        assert_eq!(item["call_id"], "call_123");
        // `output` is a JSON-encoded string (Realtime requires a string), and it
        // carries an error body.
        let out = item["output"].as_str().expect("output is a string");
        assert!(out.contains("error"));
        assert_eq!(res.response_create["type"], "response.create");
    }

    #[test]
    fn noop_advertises_no_tools() {
        assert!(NoopDispatcher.tool_schemas().is_empty());
    }

    #[test]
    fn function_call_output_keeps_output_as_string() {
        let r = function_call_output("call_9", "{\"ok\":true}".to_string());
        assert_eq!(r.conversation_item_create["item"]["call_id"], "call_9");
        // Must remain a string, not a parsed object.
        assert_eq!(
            r.conversation_item_create["item"]["output"],
            "{\"ok\":true}"
        );
    }

    #[test]
    fn tool_schema_serializes_with_function_type() {
        let s = ToolSchema {
            kind: "function".into(),
            name: "write_note".into(),
            description: "save a note".into(),
            parameters: serde_json::json!({ "type": "object" }),
        };
        let v = serde_json::to_value(&s).unwrap();
        // Realtime requires `type: "function"` on each callable tool entry.
        assert_eq!(v["type"], "function");
        assert_eq!(v["name"], "write_note");
        assert_eq!(v["description"], "save a note");
        assert_eq!(v["parameters"]["type"], "object");
    }

    #[test]
    fn managed_dispatcher_holds_trait_object() {
        // Compile-time: NoopDispatcher coerces into Arc<dyn DispatcherSeam>.
        let _m = ManagedDispatcher(Arc::new(NoopDispatcher));
    }
}
