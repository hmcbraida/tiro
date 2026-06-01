//! Agent module -- LLM-backed assistant integration.
//!
//! Submodules are populated incrementally; this file re-exports the
//! public types the rest of the crate consumes.

pub mod config;
pub mod prompt;
pub mod session;
pub mod store;
pub mod tools;

pub mod runtime;

pub use runtime::{AgentEvent, AgentRuntime};
