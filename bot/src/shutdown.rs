//! Graceful Shutdown Handler
//!
//! Handles Ctrl+C and other termination signals for clean shutdown.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::signal;

/// Global shutdown flag
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Check if shutdown has been requested
pub fn is_shutdown_requested() -> bool {
    SHUTDOWN.load(Ordering::Relaxed)
}

/// Request shutdown
pub fn request_shutdown() {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

/// Shared shutdown signal
#[derive(Clone)]
pub struct ShutdownSignal {
    shutdown: Arc<AtomicBool>,
}

impl ShutdownSignal {
    pub fn new() -> Self {
        Self {
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if shutdown has been requested
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    /// Request shutdown
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Get a clone of the shutdown signal
    pub fn clone_signal(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

/// Wait for shutdown signal (Ctrl+C)
pub async fn wait_for_shutdown() {
    match signal::ctrl_c().await {
        Ok(()) => {
            println!("\nReceived shutdown signal, cleaning up...");
            request_shutdown();
        }
        Err(e) => {
            eprintln!("Failed to listen for shutdown signal: {}", e);
        }
    }
}

/// Run shutdown handler in background
/// Returns a ShutdownSignal that will be triggered on Ctrl+C
pub fn spawn_shutdown_handler() -> ShutdownSignal {
    let signal = ShutdownSignal::new();
    let signal_clone = signal.clone();

    tokio::spawn(async move {
        wait_for_shutdown().await;
        signal_clone.shutdown();
    });

    signal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shutdown_signal() {
        let signal = ShutdownSignal::new();
        assert!(!signal.is_shutdown());
        
        signal.shutdown();
        assert!(signal.is_shutdown());
    }
}
