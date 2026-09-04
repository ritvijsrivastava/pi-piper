//! Library crate backing the `piper` binary.
//!
//! Splitting the logic out into a library (with a thin binary in
//! `main.rs`) lets integration tests in `tests/` exercise the process and
//! server modules directly, e.g. to drive a real `pi` process end-to-end
//! over a real WebSocket connection.

pub mod config;
pub mod rpc;
pub mod server;
pub mod state;
