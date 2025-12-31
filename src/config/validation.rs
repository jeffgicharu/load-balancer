//! Configuration validation.

use crate::config::{Config, HealthCheckType, Protocol};
use std::collections::HashSet;

/// Validate the configuration.
///
/// Checks for:
/// - At least one frontend and one backend
/// - Unique frontend and backend names
/// - Frontend backend references exist
/// - HTTP health checks have paths
/// - No duplicate listen addresses
///
/// # Returns
///
/// `Ok(())` if valid, or an error message describing the problem.
pub fn validate_config(config: &Config) -> Result<(), String> {
    let mut errors = Vec::new();

    // Check for at least one frontend
    if config.frontends.is_empty() {
        errors.push("at least one frontend must be defined".to_string());
    }

    // Check for at least one backend
    if config.backends.is_empty() {
        errors.push("at least one backend must be defined".to_string());
    }

    // Collect backend names for reference checking
    let backend_names: HashSet<&str> = config.backends.iter().map(|b| b.name.as_str()).collect();

    // Check for duplicate backend names
    if backend_names.len() != config.backends.len() {
        errors.push("duplicate backend names detected".to_string());
    }

    // Check for unique frontend names and valid backend references
    let mut frontend_names = HashSet::new();
    let mut listen_addresses = HashSet::new();

    for frontend in &config.frontends {
        // Check for empty name
        if frontend.name.is_empty() {
            errors.push("frontend name cannot be empty".to_string());
        }

        // Check for duplicate frontend names
        if !frontend_names.insert(&frontend.name) {
            errors.push(format!("duplicate frontend name: {}", frontend.name));
        }

        // Check for duplicate listen addresses
        if !listen_addresses.insert(frontend.listen) {
            errors.push(format!(
                "duplicate listen address: {} (frontend: {})",
                frontend.listen, frontend.name
            ));
        }

        // Check that backend reference exists
        if !backend_names.contains(frontend.backend.as_str()) {
            errors.push(format!(
                "frontend '{}' references non-existent backend '{}'",
                frontend.name, frontend.backend
            ));
        }

        // Check HTTP-specific requirements
        if frontend.protocol == Protocol::Http && frontend.http.is_none() {
            // HTTP config is optional, but we could warn here if needed
        }
    }

    // Validate backends
    for backend in &config.backends {
        // Check for empty name
        if backend.name.is_empty() {
            errors.push("backend name cannot be empty".to_string());
        }

        // Check for at least one server
        if backend.servers.is_empty() {
            errors.push(format!(
                "backend '{}' must have at least one server",
                backend.name
            ));
        }

        // Check server weights
        for server in &backend.servers {
            if server.weight == 0 {
                errors.push(format!(
                    "server {} in backend '{}' has weight 0 (must be >= 1)",
                    server.address, backend.name
                ));
            }
        }

        // Check HTTP health check has path
        if let Some(ref hc) = backend.health_check {
            if hc.check_type == HealthCheckType::Http && hc.path.is_none() {
                errors.push(format!(
                    "backend '{}' has HTTP health check but no path specified",
                    backend.name
                ));
            }
        }
    }

    // Validate log level
    let valid_levels = ["trace", "debug", "info", "warn", "error"];
    if !valid_levels.contains(&config.global.log_level.to_lowercase().as_str()) {
        errors.push(format!(
            "invalid log level '{}', must be one of: {}",
            config.global.log_level,
            valid_levels.join(", ")
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use std::net::SocketAddr;

    fn minimal_config() -> Config {
        Config {
            global: GlobalConfig::default(),
            health_check_defaults: HealthCheckDefaults::default(),
            frontends: vec![FrontendConfig {
                name: "test".to_string(),
                listen: "127.0.0.1:8080".parse().unwrap(),
                protocol: Protocol::Http,
                backend: "test-backend".to_string(),
                algorithm: Algorithm::RoundRobin,
                http: None,
                tcp: None,
            }],
            backends: vec![BackendConfig {
                name: "test-backend".to_string(),
                servers: vec![ServerConfig {
                    address: "127.0.0.1:9000".parse().unwrap(),
                    weight: 1,
                }],
                health_check: None,
            }],
        }
    }

    #[test]
    fn test_valid_config() {
        let config = minimal_config();
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_no_frontends() {
        let mut config = minimal_config();
        config.frontends.clear();
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least one frontend"));
    }

    #[test]
    fn test_no_backends() {
        let mut config = minimal_config();
        config.backends.clear();
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least one backend"));
    }

    #[test]
    fn test_missing_backend_reference() {
        let mut config = minimal_config();
        config.frontends[0].backend = "nonexistent".to_string();
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("non-existent backend"));
    }

    #[test]
    fn test_duplicate_frontend_names() {
        let mut config = minimal_config();
        config.frontends.push(FrontendConfig {
            name: "test".to_string(),
            listen: "127.0.0.1:8081".parse().unwrap(),
            protocol: Protocol::Http,
            backend: "test-backend".to_string(),
            algorithm: Algorithm::RoundRobin,
            http: None,
            tcp: None,
        });
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("duplicate frontend name"));
    }

    #[test]
    fn test_duplicate_listen_address() {
        let mut config = minimal_config();
        config.frontends.push(FrontendConfig {
            name: "test2".to_string(),
            listen: "127.0.0.1:8080".parse().unwrap(), // Same as first
            protocol: Protocol::Http,
            backend: "test-backend".to_string(),
            algorithm: Algorithm::RoundRobin,
            http: None,
            tcp: None,
        });
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("duplicate listen address"));
    }

    #[test]
    fn test_http_health_check_missing_path() {
        let mut config = minimal_config();
        config.backends[0].health_check = Some(HealthCheckConfig {
            check_type: HealthCheckType::Http,
            path: None, // Missing path for HTTP check
            expected_status: 200,
            interval: None,
            timeout: None,
        });
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no path specified"));
    }

    #[test]
    fn test_zero_weight() {
        let mut config = minimal_config();
        config.backends[0].servers[0].weight = 0;
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("weight 0"));
    }
}
