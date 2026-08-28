use std::{
    sync::{
        Arc,
        RwLock,
    },
};

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
    ///
    /// Rclone initialization happens in the background so that the
    /// WebUI can start immediately and report initialization progress.
    pub rclone: RwLock<RcloneState>,
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
    /// Create the initial application state.
    ///
    /// Rclone starts in the Initializing state. The actual initialization
    /// process is started separately so that the WebUI does not have to wait.
    pub fn new(
        runtime: Runtime,
    ) -> Self {
        Self {
            runtime,

            rclone: RwLock::new(
                RcloneState::Initializing,
            ),
        }
    }

    /// Start Rclone initialization in the background.
    ///
    /// Rclone installation uses blocking filesystem/process operations, so
    /// the work is moved to Tokio's blocking thread pool.
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
    ///
    /// This keeps the WebUI from holding the application-state lock while
    /// rendering templates.
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
}