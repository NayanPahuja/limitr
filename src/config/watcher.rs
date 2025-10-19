//! Notify-based configuration hot-reload watcher.
//!
//! - Watches a single JSON file using notify::RecommendedWatcher.
//! - On create/modify events, attempts to reload/validate and atomically replace the cache.

use crate::config::ConfigCache;
use crate::config::loader::load_rate_limit_config_from_file;
use crate::errors::RateLimitError;
use arc_swap::ArcSwap;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Watch configuration file for changes and trigger reloads.
///
/// This function spawns a watcher that monitors the given configuration file.
/// On a valid change (Modify or Create event), it will attempt to load and
/// validate the new configuration. If successful, it atomically swaps the
/// new `ConfigCache` into the shared `ArcSwap`.
pub async fn watch_config_file(
    path: PathBuf,
    shared_cache: Arc<ArcSwap<ConfigCache>>,
) -> Result<(), notify::Error> {
    // We use a tokio MPSC channel to bridge the sync watcher thread with our async task.
    let (tx, mut rx) = mpsc::channel(1);

    // Create the watcher and pass it a closure that sends events to our channel.
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            // We use blocking_send as this callback is synchronous.
            if let Err(e) = tx.blocking_send(res) {
                // This can happen if the receiver is dropped, meaning the main task ended.
                // It's not a critical error in that case.
                debug!("Failed to send config file event: {}", e);
            }
        },
        notify::Config::default(),
    )?;

    // Start watching the specified path.
    watcher.watch(&path, RecursiveMode::NonRecursive)?;
    info!("Watching config file for changes: {}", path.display());

    // This is the core event loop for our async task.
    while let Some(res) = rx.recv().await {
        match res {
            Ok(event) => {
                if should_reload(&event) {
                    info!(
                        "Config file change detected. Event: {:?}. Triggering reload.",
                        event.kind
                    );
                    reload_config(&path, &shared_cache).await;
                } else {
                    debug!("Ignoring irrelevant filesystem event: {:?}", event.kind);
                }
            }
            Err(e) => {
                crate::metrics::record_config_reload(false);
                // Errors from `notify` can be significant.
                error!("Error watching config file: {}", e);
            }
        }
    }

    // This part is unreachable unless the watcher's sender side is dropped,
    // which shouldn't happen while the watcher is alive.
    warn!("Configuration watcher task is shutting down.");
    Ok(())
}

/// Helper function to determine if a filesystem event should trigger a reload.
/// We are interested in file modifications and creations.
fn should_reload(event: &Event) -> bool {
    matches!(
        event.kind,
        notify::EventKind::Modify(_) | notify::EventKind::Create(_)
    )
}

/// Performs the actual config reloading and atomic swap.
async fn reload_config(path: &Path, shared_cache: &Arc<ArcSwap<ConfigCache>>) {
    // 1. Load the new rate limit config from file. Your existing function already includes validation.
    let new_rate_limit_config = match load_rate_limit_config_from_file(path).await {
        Ok(config) => config,
        Err(e) => {
            // Log the specific error but continue with the old configuration.
            // This is crucial for service stability.
            match e {
                RateLimitError::FileSystemError(io_err) => {
                    error!(
                        "Failed to read config file '{}': {}. Keeping old config.",
                        path.display(),
                        io_err
                    );
                }
                RateLimitError::JsonError(json_err) => {
                    error!(
                        "Failed to parse JSON from '{}': {}. Keeping old config.",
                        path.display(),
                        json_err
                    );
                }
                RateLimitError::ConfigurationError(validation_err) => {
                    error!(
                        "New configuration in '{}' is invalid: {}. Keeping old config.",
                        path.display(),
                        validation_err
                    );
                }
                _ => {
                    crate::metrics::record_config_reload(false);
                    error!(
                        "An unexpected error occurred while reloading config: {}. Keeping old config.",
                        e
                    );
                }
            }
            return;
        }
    };

    // 2. Build a new ConfigCache from the successfully loaded and validated config.
    let new_cache = ConfigCache::new(new_rate_limit_config);
    info!("New configuration loaded and validated successfully.");

    // 3. Atomically swap the new cache into the shared pointer.
    // The `store` method performs the atomic replacement. All existing readers
    // will finish with the old `Arc`, and new readers will get the new one.
    shared_cache.store(Arc::new(new_cache));
    crate::metrics::record_config_reload(true);
    info!("Configuration hot-reloaded successfully. Service is now using the new settings.");
}
