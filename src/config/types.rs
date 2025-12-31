//! Configuration data types.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;

/// Root configuration structure.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Global settings
    #[serde(default)]
    pub global: GlobalConfig,

    /// Default health check settings
    #[serde(default)]
    pub health_check_defaults: HealthCheckDefaults,

    /// Frontend definitions (where we listen)
    #[serde(default)]
    pub frontends: Vec<FrontendConfig>,

    /// Backend pool definitions (upstream servers)
    #[serde(default)]
    pub backends: Vec<BackendConfig>,
}

/// Global configuration settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GlobalConfig {
    /// Log level: trace, debug, info, warn, error
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Log format: json or pretty
    #[serde(default = "default_log_format")]
    pub log_format: LogFormat,

    /// Metrics configuration
    #[serde(default)]
    pub metrics: MetricsConfig,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            log_format: LogFormat::Json,
            metrics: MetricsConfig::default(),
        }
    }
}

/// Log output format.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Json,
    Pretty,
}

/// Metrics endpoint configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MetricsConfig {
    /// Whether metrics endpoint is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Address to bind metrics server
    #[serde(default = "default_metrics_address")]
    pub address: SocketAddr,

    /// Path for metrics endpoint
    #[serde(default = "default_metrics_path")]
    pub path: String,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            address: default_metrics_address(),
            path: default_metrics_path(),
        }
    }
}

/// Default health check settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthCheckDefaults {
    /// How often to probe backends
    #[serde(default = "default_health_interval", with = "humantime_serde")]
    pub interval: Duration,

    /// Timeout for health check response
    #[serde(default = "default_health_timeout", with = "humantime_serde")]
    pub timeout: Duration,

    /// Consecutive failures before marking unhealthy
    #[serde(default = "default_unhealthy_threshold")]
    pub unhealthy_threshold: u32,

    /// Consecutive successes before marking healthy
    #[serde(default = "default_healthy_threshold")]
    pub healthy_threshold: u32,

    /// Cooldown before retrying unhealthy backend
    #[serde(default = "default_cooldown", with = "humantime_serde")]
    pub cooldown: Duration,
}

impl Default for HealthCheckDefaults {
    fn default() -> Self {
        Self {
            interval: default_health_interval(),
            timeout: default_health_timeout(),
            unhealthy_threshold: default_unhealthy_threshold(),
            healthy_threshold: default_healthy_threshold(),
            cooldown: default_cooldown(),
        }
    }
}

/// Frontend configuration (listener).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FrontendConfig {
    /// Unique name for this frontend
    pub name: String,

    /// Address and port to listen on
    pub listen: SocketAddr,

    /// Protocol: tcp or http
    #[serde(default)]
    pub protocol: Protocol,

    /// Name of the backend pool to use
    pub backend: String,

    /// Load balancing algorithm
    #[serde(default)]
    pub algorithm: Algorithm,

    /// HTTP-specific settings
    #[serde(default)]
    pub http: Option<HttpConfig>,

    /// TCP-specific settings
    #[serde(default)]
    pub tcp: Option<TcpConfig>,
}

/// Protocol type.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[default]
    Tcp,
    Http,
}

/// Load balancing algorithm.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Algorithm {
    #[default]
    RoundRobin,
    Weighted,
    LeastConnections,
    IpHash,
}

/// HTTP-specific configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct HttpConfig {
    /// Headers to add to requests going to backend
    #[serde(default)]
    pub request_headers: std::collections::HashMap<String, String>,

    /// Headers to add to responses going to client
    #[serde(default)]
    pub response_headers: std::collections::HashMap<String, String>,
}

/// TCP-specific configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TcpConfig {
    /// Connection timeout
    #[serde(default = "default_connect_timeout", with = "humantime_serde")]
    pub connect_timeout: Duration,
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self {
            connect_timeout: default_connect_timeout(),
        }
    }
}

/// Backend pool configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackendConfig {
    /// Unique name for this backend pool
    pub name: String,

    /// List of upstream servers
    pub servers: Vec<ServerConfig>,

    /// Health check configuration for this backend
    #[serde(default)]
    pub health_check: Option<HealthCheckConfig>,
}

/// Individual server configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    /// Server address and port
    pub address: SocketAddr,

    /// Weight for weighted load balancing (default: 1)
    #[serde(default = "default_weight")]
    pub weight: u32,
}

/// Health check configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthCheckConfig {
    /// Type of health check: tcp or http
    #[serde(default, rename = "type")]
    pub check_type: HealthCheckType,

    /// HTTP path to check (for HTTP health checks)
    #[serde(default)]
    pub path: Option<String>,

    /// Expected HTTP status code (for HTTP health checks)
    #[serde(default = "default_expected_status")]
    pub expected_status: u16,

    /// Override interval for this backend
    #[serde(default, with = "option_humantime_serde")]
    pub interval: Option<Duration>,

    /// Override timeout for this backend
    #[serde(default, with = "option_humantime_serde")]
    pub timeout: Option<Duration>,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_type: HealthCheckType::Tcp,
            path: None,
            expected_status: default_expected_status(),
            interval: None,
            timeout: None,
        }
    }
}

/// Type of health check.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthCheckType {
    #[default]
    Tcp,
    Http,
}

// Default value functions
fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> LogFormat {
    LogFormat::Json
}

fn default_true() -> bool {
    true
}

fn default_metrics_address() -> SocketAddr {
    "127.0.0.1:9090".parse().unwrap()
}

fn default_metrics_path() -> String {
    "/metrics".to_string()
}

fn default_health_interval() -> Duration {
    Duration::from_secs(10)
}

fn default_health_timeout() -> Duration {
    Duration::from_secs(5)
}

fn default_unhealthy_threshold() -> u32 {
    3
}

fn default_healthy_threshold() -> u32 {
    2
}

fn default_cooldown() -> Duration {
    Duration::from_secs(30)
}

fn default_connect_timeout() -> Duration {
    Duration::from_secs(10)
}

fn default_weight() -> u32 {
    1
}

fn default_expected_status() -> u16 {
    200
}

/// Custom serde module for humantime durations.
mod humantime_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = humantime::format_duration(*duration).to_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        humantime::parse_duration(&s).map_err(serde::de::Error::custom)
    }
}

/// Custom serde module for optional humantime durations.
mod option_humantime_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match duration {
            Some(d) => {
                let s = humantime::format_duration(*d).to_string();
                serializer.serialize_some(&s)
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => {
                let d = humantime::parse_duration(&s).map_err(serde::de::Error::custom)?;
                Ok(Some(d))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config {
            global: GlobalConfig::default(),
            health_check_defaults: HealthCheckDefaults::default(),
            frontends: vec![],
            backends: vec![],
        };
        assert_eq!(config.global.log_level, "info");
    }

    #[test]
    fn test_algorithm_serde() {
        let algo: Algorithm = serde_yaml::from_str("round_robin").unwrap();
        assert_eq!(algo, Algorithm::RoundRobin);

        let algo: Algorithm = serde_yaml::from_str("least_connections").unwrap();
        assert_eq!(algo, Algorithm::LeastConnections);
    }
}
