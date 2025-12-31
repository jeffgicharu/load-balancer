//! rustlb - A high-performance Layer 4/7 load balancer
//!
//! This crate provides a production-ready load balancer with support for:
//! - TCP (Layer 4) and HTTP (Layer 7) protocols
//! - Multiple load balancing algorithms
//! - Active and passive health checking
//! - Hot configuration reload
//! - Prometheus metrics

pub mod backend;
pub mod config;
pub mod frontend;
pub mod health;
pub mod metrics;
pub mod proxy;
pub mod util;

pub use config::Config;
