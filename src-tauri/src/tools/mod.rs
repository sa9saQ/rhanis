//! M1 tools registered into the dispatcher (koe-2gy).
//!
//! koe-2gy ships `write_note` (a SAFE, path-free reference tool). koe-s7i adds
//! `web_search` / `read_file` / `take_screenshot` here by extending
//! [`register_m1_tools`] — no change to the dispatcher itself.

pub mod notes;

use std::sync::Arc;

use crate::storage::adapter::RecorderAdapter;
use crate::tool_dispatcher::ToolRegistry;

/// Registers every M1 tool (impl + `session.update` schema) into the dispatcher
/// registry. The single place koe-s7i extends to wire the remaining tools.
pub fn register_m1_tools(registry: &mut ToolRegistry, recorder: Arc<dyn RecorderAdapter>) {
    registry.register(
        "write_note",
        notes::write_note_tool(recorder),
        notes::write_note_schema(),
    );
}
