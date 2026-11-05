//! Dreamer's domain, local SQLite knowledge store and future adapter boundaries.
//! Includes MCP transports, immutable lesson lifecycle and optional agent-driven Mac learning.

pub mod activity;
pub mod collector;
pub mod config;
pub mod consolidation;
pub mod ingest;
pub mod lesson;
pub mod lifecycle;
pub mod mcp;
pub mod store;

#[cfg(feature = "mac-client")]
pub mod mac;

pub mod repository;
pub mod retrieval;

pub mod conversation;
