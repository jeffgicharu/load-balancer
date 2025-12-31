//! Backend pool management and load balancing algorithms.

pub mod algorithms;
mod pool;
mod router;

pub use pool::BackendPool;
pub use router::BackendRouter;
