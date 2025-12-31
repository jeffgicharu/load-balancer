//! rustlb - A high-performance Layer 4/7 load balancer
//!
//! Usage:
//!     rustlb --config <path>
//!
//! See --help for more options.

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use rustlb::backend::BackendRouter;
use rustlb::config::{load_config, Config, ConfigWatcher};
use rustlb::frontend::FrontendListener;
use rustlb::health::{HealthChecker, HealthConfig, HealthState};
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

    /// Disable config file watching
    #[arg(long)]
    no_watch: bool,
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
    run(config, cli.config, cli.no_watch)
}

/// Run the load balancer with the given configuration.
fn run(config: Config, config_path: PathBuf, no_watch: bool) -> Result<()> {
    // Create tokio runtime
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;

    runtime.block_on(async { run_async(config, config_path, no_watch).await })
}

/// Async entry point for the load balancer.
async fn run_async(config: Config, config_path: PathBuf, no_watch: bool) -> Result<()> {
    // Create shutdown channel
    let (shutdown_tx, _) = broadcast::channel::<()>(16);

    // Create health state with config defaults
    let health_config = HealthConfig {
        unhealthy_threshold: config.health_check_defaults.unhealthy_threshold,
        healthy_threshold: config.health_check_defaults.healthy_threshold,
        cooldown: config.health_check_defaults.cooldown,
    };
    let health_state = Arc::new(HealthState::with_config(health_config));

    // Create backend router
    let router = Arc::new(BackendRouter::new(&config.backends, &config.frontends));

    // Store handles for all tasks
    let mut handles = Vec::new();

    // Start health checker
    let health_checker = HealthChecker::new(
        Arc::clone(&health_state),
        config.backends.clone(),
        config.health_check_defaults.interval,
        config.health_check_defaults.timeout,
    );
    let shutdown_rx = shutdown_tx.subscribe();
    let health_handle = tokio::spawn(async move {
        health_checker.run(shutdown_rx).await;
    });
    handles.push(health_handle);

    // Start config watcher (unless disabled)
    if !no_watch {
        let shutdown_rx = shutdown_tx.subscribe();
        let watcher = ConfigWatcher::new(
            config_path,
            Box::new(move |new_config| {
                info!(
                    frontends = new_config.frontends.len(),
                    backends = new_config.backends.len(),
                    "config reload triggered"
                );
                // Note: Full hot reload would require recreating router and listeners
                // For now, we just log the event. Full implementation would use ArcSwap
                // in the router to atomically swap the configuration.
                warn!("hot reload of listeners not yet implemented - restart required for changes");
            }),
        );
        let watcher_handle = tokio::spawn(async move {
            watcher.run(shutdown_rx).await;
        });
        handles.push(watcher_handle);
    }

    // Start frontend listeners
    for frontend_config in config.frontends {
        let router = Arc::clone(&router);
        let shutdown_rx = shutdown_tx.subscribe();

        let listener = FrontendListener::bind(frontend_config.clone(), router)
            .await
            .with_context(|| {
                format!(
                    "failed to bind frontend '{}' on {}",
                    frontend_config.name, frontend_config.listen
                )
            })?;

        let handle = tokio::spawn(async move {
            listener.run(shutdown_rx).await;
        });

        handles.push(handle);
    }

    info!("rustlb is running");
    info!("press Ctrl+C to stop, send SIGHUP to reload config");

    // Wait for shutdown signal
    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            info!("received shutdown signal");
        }
        Err(e) => {
            error!(error = %e, "failed to listen for shutdown signal");
        }
    }

    // Signal all tasks to shut down
    info!("initiating graceful shutdown");
    let _ = shutdown_tx.send(());

    // Wait for all tasks to finish with timeout
    let shutdown_timeout = Duration::from_secs(30);
    let shutdown_deadline = tokio::time::sleep(shutdown_timeout);
    tokio::pin!(shutdown_deadline);

    for (i, handle) in handles.into_iter().enumerate() {
        tokio::select! {
            result = handle => {
                if let Err(e) = result {
                    warn!(task = i, error = %e, "task panicked during shutdown");
                }
            }
            _ = &mut shutdown_deadline => {
                warn!("shutdown timeout reached, forcing exit");
                break;
            }
        }
    }

    info!("rustlb shut down complete");
    Ok(())
}
