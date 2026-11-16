//! gitrat: a terminal UI for staging, viewing diffs and committing.
//!
//! The binary (`main.rs`) only parses the CLI and runs the event loop; every
//! module lives here so the integration tests in `tests/` can use it.

pub mod app;
pub mod diff;
pub mod event_handler;
pub mod git;
pub mod terminal;
pub mod theme;
pub mod types;
pub mod ui;
