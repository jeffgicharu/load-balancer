//! Request ID generation for request tracing.
//!
//! Generates unique identifiers for each request to enable tracing
//! through logs and distributed systems.

use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

/// Counter for short request IDs.
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a new UUID-based request ID.
///
/// This provides globally unique IDs suitable for distributed tracing.
pub fn generate_request_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generate a short request ID based on a counter.
///
/// This is faster than UUID but only unique within a single process.
/// Format: `req-{counter}` where counter is zero-padded to 16 hex digits.
pub fn generate_short_request_id() -> String {
    let count = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("req-{:016x}", count)
}

/// Request ID wrapper that can be included in tracing spans.
#[derive(Clone, Debug)]
pub struct RequestId(String);

impl RequestId {
    /// Create a new random request ID.
    pub fn new() -> Self {
        Self(generate_request_id())
    }

    /// Create a new short request ID.
    pub fn short() -> Self {
        Self(generate_short_request_id())
    }

    /// Create a request ID from an existing string (e.g., from a header).
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Get the request ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for RequestId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_generate_request_id() {
        let id1 = generate_request_id();
        let id2 = generate_request_id();

        // UUIDs should be different
        assert_ne!(id1, id2);

        // Should be valid UUID format (36 chars with hyphens)
        assert_eq!(id1.len(), 36);
        assert!(id1.contains('-'));
    }

    #[test]
    fn test_generate_short_request_id() {
        let id1 = generate_short_request_id();
        let id2 = generate_short_request_id();

        // Should be different
        assert_ne!(id1, id2);

        // Should have the expected prefix
        assert!(id1.starts_with("req-"));
        assert!(id2.starts_with("req-"));
    }

    #[test]
    fn test_short_request_id_uniqueness() {
        let mut ids = HashSet::new();
        for _ in 0..1000 {
            let id = generate_short_request_id();
            assert!(ids.insert(id), "duplicate ID generated");
        }
    }

    #[test]
    fn test_request_id_wrapper() {
        let id = RequestId::new();
        assert!(!id.as_str().is_empty());

        let short_id = RequestId::short();
        assert!(short_id.as_str().starts_with("req-"));

        let custom_id = RequestId::from_string("custom-123");
        assert_eq!(custom_id.as_str(), "custom-123");
    }

    #[test]
    fn test_request_id_display() {
        let id = RequestId::from_string("test-id-123");
        assert_eq!(format!("{}", id), "test-id-123");
    }
}
