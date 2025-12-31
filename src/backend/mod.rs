//! Backend pool management and load balancing algorithms.

pub mod algorithms;
mod router;

pub use router::BackendRouter;
