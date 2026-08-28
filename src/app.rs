use std::{
    sync::{
        Arc,
        RwLock,
    },
};

use tokio::sync::watch;

use crate::{
    bootstrap::Runtime,
    rclone::{
        self,
        RcloneStatus,
    },
};

/// Shared runtime state for the BOREAL application.
pub struct AppState {
    pub runtime: Runtime,

    /// Current state of the BOREAL-managed Rclone installation.
    pub rclone: RwLock<RcloneState>,

    /// Signals that BOREAL should shut down.
    shutdown_tx: watch::Sender<bool>,
}

/// Current state of the BOREAL-managed Rclone installation.
#[derive(Debug, Clone)]
pub enum RcloneState {
    /// BOREAL is checking, downloading, installing, or verifying Rclone.
    Initializing,

    /// Rclone is installed and has been successfully verified.
    Ready(RcloneStatus),

    /// BOREAL attempted to install or verify Rclone, but the operation failed.
    Error(String),
}

impl AppState {
    /// Create the shared application state.
    pub fn new(
        runtime: Runtime,
    ) -> Self {
        let (
            shutdown_tx,
            _shutdown_rx,
        ) = watch::channel(
            false,
        );

        Self {
            runtime,

            rclone: RwLock::new(
                RcloneState::Initializing,
            ),

            shutdown_tx,
        }
    }

    /// Start Rclone initialization in the background.
    pub fn initialize_rclone(
        state: Arc<Self>,
    ) {
        tokio::spawn(
            async move {
                println!(
                    "Checking BOREAL-managed Rclone..."
                );

                let worker_state = Arc::clone(
                    &state,
                );

                let result = tokio::task::spawn_blocking(
                    move || {
                        rclone::ensure_installed(
                            &worker_state.runtime,
                        )
                    },
                )
                .await;

                let new_state = match result {
                    Ok(
                        Ok(
                            status,
                        ),
                    ) => {
                        println!(
                            "Rclone ready: {}",
                            status.version
                        );

                        println!(
                            "Rclone path: {}",
                            status.path.display()
                        );

                        RcloneState::Ready(
                            status,
                        )
                    }

                    Ok(
                        Err(
                            error,
                        ),
                    ) => {
                        let message = error.to_string();

                        eprintln!(
                            "Rclone initialization failed: {message}"
                        );

                        RcloneState::Error(
                            message,
                        )
                    }

                    Err(
                        error,
                    ) => {
                        let message = format!(
                            "Rclone initialization task failed: {error}"
                        );

                        eprintln!(
                            "{message}"
                        );

                        RcloneState::Error(
                            message,
                        )
                    }
                };

                match state.rclone.write() {
                    Ok(
                        mut rclone,
                    ) => {
                        *rclone = new_state;
                    }

                    Err(
                        error,
                    ) => {
                        eprintln!(
                            "Unable to update Rclone application state: {error}"
                        );
                    }
                }
            },
        );
    }

    /// Return a snapshot of the current Rclone state.
    pub fn rclone_state(
        &self,
    ) -> RcloneState {
        match self.rclone.read() {
            Ok(
                state,
            ) => state.clone(),

            Err(
                error,
            ) => {
                RcloneState::Error(
                    format!(
                        "Unable to read Rclone application state: {error}"
                    ),
                )
            }
        }
    }

    /// Request a graceful BOREAL shutdown.
    ///
    /// This is used by both the WebUI Quit action and other application
    /// components that may need to request shutdown.
    pub fn request_shutdown(
        &self,
    ) {
        let _ = self.shutdown_tx.send(
            true,
        );
    }

    /// Subscribe to application shutdown requests.
    pub fn shutdown_receiver(
        &self,
    ) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }
}