//! rustlb - A high-performance Layer 4/7 load balancer
//!
//! Usage:
//!     rustlb --config <path>
//!
//! See --help for more options.

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use tracing::{error, info};

use rustlb::config::{load_config, Config};
use rustlb::util::init_logging;

/// A high-performance Layer 4/7 load balancer written in Rust.
#[derive(Parser, Debug)]
#[command(name = "rustlb")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the configuration file
    #[arg(short, long, value_name = "FILE")]
    config: PathBuf,

    /// Override log level (trace, debug, info, warn, error)
    #[arg(short, long, value_name = "LEVEL")]
    log_level: Option<String>,

    /// Validate configuration and exit
    #[arg(long)]
    validate: bool,
}

fn main() -> Result<()> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Load configuration
    let config = load_config(&cli.config).with_context(|| {
        format!(
            "failed to load configuration from '{}'",
            cli.config.display()
        )
    })?;

    // Determine log level (CLI overrides config)
    let log_level = cli
        .log_level
        .as_deref()
        .unwrap_or(&config.global.log_level);

    // Initialize logging
    init_logging(log_level, &config.global.log_format);

    // If --validate flag, just validate and exit
    if cli.validate {
        info!("Configuration is valid");
        println!("Configuration is valid.");
        println!("  Frontends: {}", config.frontends.len());
        println!("  Backends: {}", config.backends.len());
        for frontend in &config.frontends {
            println!(
                "    - {} ({:?}) -> {} [{:?}]",
                frontend.name, frontend.protocol, frontend.backend, frontend.algorithm
            );
        }
        return Ok(());
    }

    // Log startup information
    info!(
        config_path = %cli.config.display(),
        frontends = config.frontends.len(),
        backends = config.backends.len(),
        "rustlb starting"
    );

    // Print configuration summary
    for frontend in &config.frontends {
        info!(
            name = %frontend.name,
            listen = %frontend.listen,
            protocol = ?frontend.protocol,
            backend = %frontend.backend,
            algorithm = ?frontend.algorithm,
            "configured frontend"
        );
    }

    for backend in &config.backends {
        info!(
            name = %backend.name,
            servers = backend.servers.len(),
            "configured backend"
        );
    }

    // Run the load balancer
    run(config)
}

/// Run the load balancer with the given configuration.
fn run(config: Config) -> Result<()> {
    // Create tokio runtime
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;

    runtime.block_on(async {
        run_async(config).await
    })
}

/// Async entry point for the load balancer.
async fn run_async(config: Config) -> Result<()> {
    info!("rustlb is running");
    info!("press Ctrl+C to stop");

    // Wait for shutdown signal
    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            info!("received shutdown signal");
        }
        Err(e) => {
            error!(error = %e, "failed to listen for shutdown signal");
        }
    }

    info!("rustlb shutting down");
    Ok(())
}
