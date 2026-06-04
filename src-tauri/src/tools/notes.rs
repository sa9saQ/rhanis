//! `write_note` tool (koe-2gy reference tool).
//!
//! A SAFE tool: takes `{ "text": "…" }`, size-bounds it, and persists it through
//! the recorder (`RecorderAdapter::save_note`). Chosen as koe-2gy's first real
//! tool because it has no filesystem-path surface (so no TOCTOU to get wrong —
//! `read_file` is deferred to koe-s7i where a component-safe open is built).
//!
//! The recorder is synchronous (its own `std::sync::Mutex`), so the call goes
//! through `spawn_blocking` per the `RecorderAdapter` contract.
//!
//! transaction N/A · idempotency_key N/A (local note persistence, not billing).

use std::sync::Arc;

use serde_json::Value;

use crate::realtime_types::ToolSchema;
use crate::storage::adapter::RecorderAdapter;
use crate::tool_dispatcher::ToolFn;

/// Max note length persisted (bytes). The dispatcher already caps total tool
/// output; this bounds what we *store* so a runaway model note cannot bloat the DB.
const MAX_NOTE_LEN: usize = 8 * 1024;

/// Builds the `write_note` [`ToolFn`], capturing the recorder.
pub fn write_note_tool(recorder: Arc<dyn RecorderAdapter>) -> ToolFn {
    Arc::new(move |args: Value| {
        let recorder = Arc::clone(&recorder);
        Box::pin(async move {
            let text = args
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if text.is_empty() {
                return Err("note text is required".to_string());
            }
            let text = cap_note(text);
            // Recorder is sync (std Mutex) → never call it on the async executor
            // thread; hand it to a blocking worker.
            let id = tokio::task::spawn_blocking(move || recorder.save_note(&text))
                .await
                .map_err(|_| "note task failed".to_string())?
                .map_err(|_| "could not save note".to_string())?;
            Ok(serde_json::json!({ "saved": true, "id": id }).to_string())
        })
    })
}

/// The `session.update` schema advertised to the model for `write_note`.
pub fn write_note_schema() -> ToolSchema {
    ToolSchema {
        kind: "function".into(),
        name: "write_note".into(),
        description: "Save a short note to the user's local notebook.".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "The note text to save." }
            },
            "required": ["text"],
            "additionalProperties": false
        }),
    }
}

/// Truncates a note to [`MAX_NOTE_LEN`] bytes on a char boundary.
fn cap_note(mut text: String) -> String {
    if text.len() <= MAX_NOTE_LEN {
        return text;
    }
    let mut end = MAX_NOTE_LEN;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use crate::storage::adapter::{ConversationEvent, Note, RecorderError};

    /// Minimal recorder double: records `save_note` calls; the rest is unused
    /// by this tool and panics if touched (asserts the tool only saves notes).
    struct MockRecorder {
        saved: Mutex<Vec<String>>,
    }
    impl MockRecorder {
        fn new() -> Arc<Self> {
            Arc::new(Self { saved: Mutex::new(Vec::new()) })
        }
    }
    impl RecorderAdapter for MockRecorder {
        fn save_note(&self, text: &str) -> Result<i64, RecorderError> {
            let mut s = self.saved.lock().unwrap();
            s.push(text.to_string());
            Ok(s.len() as i64)
        }
        fn list_recent_notes(&self, _limit: u32) -> Result<Vec<Note>, RecorderError> {
            unimplemented!("write_note never lists")
        }
        fn log_conversation_event(&self, _r: &str, _k: &str, _s: &str) -> Result<i64, RecorderError> {
            unimplemented!("write_note never logs")
        }
        fn list_recent_events(&self, _limit: u32) -> Result<Vec<ConversationEvent>, RecorderError> {
            unimplemented!("write_note never lists")
        }
        fn add_month_cost(&self, _m: u32, _n: u64) -> Result<u64, RecorderError> {
            unimplemented!("write_note never touches cost")
        }
        fn load_cost_snapshot(&self, _m: u32) -> Result<Option<u64>, RecorderError> {
            unimplemented!("write_note never touches cost")
        }
        fn health_check(&self) -> Result<(), RecorderError> {
            unimplemented!("write_note never health-checks")
        }
    }

    #[tokio::test]
    async fn saves_trimmed_text_and_returns_id() {
        let rec = MockRecorder::new();
        let tool = write_note_tool(rec.clone());
        let out = tool(serde_json::json!({ "text": "  buy milk  " })).await.unwrap();
        assert!(out.contains("\"saved\":true"));
        assert!(out.contains("\"id\":1"));
        assert_eq!(rec.saved.lock().unwrap()[0], "buy milk");
    }

    #[tokio::test]
    async fn rejects_empty_or_missing_text() {
        let rec = MockRecorder::new();
        let tool = write_note_tool(rec.clone());
        assert!(tool(serde_json::json!({ "text": "   " })).await.is_err());
        assert!(tool(serde_json::json!({})).await.is_err());
        assert!(rec.saved.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn caps_note_length_on_char_boundary() {
        let rec = MockRecorder::new();
        let tool = write_note_tool(rec.clone());
        let big = "あ".repeat(MAX_NOTE_LEN); // 3 bytes each → far over the cap
        tool(serde_json::json!({ "text": big })).await.unwrap();
        let saved = &rec.saved.lock().unwrap()[0];
        assert!(saved.len() <= MAX_NOTE_LEN);
        assert!(saved.is_char_boundary(saved.len()));
    }

    #[test]
    fn schema_advertises_required_text() {
        let s = write_note_schema();
        assert_eq!(s.name, "write_note");
        assert_eq!(s.parameters["required"][0], "text");
        assert_eq!(s.parameters["additionalProperties"], false);
    }
}
