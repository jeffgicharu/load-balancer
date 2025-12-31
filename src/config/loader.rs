//! Configuration file loading.

use crate::config::{validate_config, Config};
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during configuration loading.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read configuration file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("failed to parse YAML: {0}")]
    ParseError(#[from] serde_yaml::Error),

    #[error("configuration validation failed: {0}")]
    ValidationError(String),
}

/// Load configuration from a YAML file.
///
/// This function reads the file, parses the YAML, and validates the configuration.
///
/// # Arguments
///
/// * `path` - Path to the configuration file
///
/// # Returns
///
/// The parsed and validated configuration, or an error.
pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
    let path = path.as_ref();

    // Read file contents
    let contents = std::fs::read_to_string(path)?;

    // Parse YAML
    let config: Config = serde_yaml::from_str(&contents)?;

    // Validate configuration
    validate_config(&config).map_err(ConfigError::ValidationError)?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_minimal_config() {
        let yaml = r#"
frontends:
  - name: test
    listen: "127.0.0.1:8080"
    backend: test-backend

backends:
  - name: test-backend
    servers:
      - address: "127.0.0.1:9000"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();

        let config = load_config(file.path()).unwrap();
        assert_eq!(config.frontends.len(), 1);
        assert_eq!(config.backends.len(), 1);
    }

    #[test]
    fn test_load_missing_file() {
        let result = load_config("/nonexistent/path/config.yaml");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ReadError(_)));
    }

    #[test]
    fn test_load_invalid_yaml() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"not: valid: yaml: {{{}}}").unwrap();

        let result = load_config(file.path());
        assert!(result.is_err());
    }
}
