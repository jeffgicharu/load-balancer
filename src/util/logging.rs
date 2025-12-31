//! Logging initialization and configuration.

use crate::config::LogFormat;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize the logging system.
///
/// # Arguments
///
/// * `level` - Log level filter (e.g., "info", "debug")
/// * `format` - Log output format (json or pretty)
pub fn init_logging(level: &str, format: &LogFormat) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    let registry = tracing_subscriber::registry().with(filter);

    match format {
        LogFormat::Json => {
            registry
                .with(fmt::layer().json())
                .init();
        }
        LogFormat::Pretty => {
            registry
                .with(fmt::layer().pretty())
                .init();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Can only init logging once per process, so we don't test init_logging directly
    #[test]
    fn test_log_format_variants() {
        assert_eq!(LogFormat::Json, LogFormat::Json);
        assert_eq!(LogFormat::Pretty, LogFormat::Pretty);
        assert_ne!(LogFormat::Json, LogFormat::Pretty);
    }
}
