//! Frontend listeners and protocol handlers.
//!
//! This module handles accepting client connections and dispatching
//! them to the appropriate protocol handler (TCP or HTTP).

mod http;
mod listener;
mod tcp;

pub use listener::FrontendListener;
