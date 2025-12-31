//! Utility functions and helpers.

mod logging;
mod request_id;
mod shutdown;

pub use logging::init_logging;
pub use request_id::{generate_request_id, generate_short_request_id, RequestId};
pub use shutdown::ShutdownSignal;
