//! Planned MCP tools, not a protocol implementation or running server.
//!
//! First implement stdio using a maintained MCP SDK, then test initialize,
//! tools/list and tools/call with a real client. Keep stdout protocol-only.
//! A future Mac collector will submit candidates through a separately authorized
//! write tool. Agents reviewing code should only need the read tools below.

pub const SEARCH_LESSONS: &str = "search_lessons";
pub const GET_LESSON: &str = "get_lesson";

pub const PLANNED_TOOLS: [&str; 2] = [SEARCH_LESSONS, GET_LESSON];
