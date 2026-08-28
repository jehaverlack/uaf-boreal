use crate::{
    bootstrap::Runtime,
    rclone::{
        self,
        RcloneStatus,
    },
};

/// Shared runtime state for the BOREAL application.
///
/// Initialization failures for optional/supporting components such as
/// Rclone are stored here instead of terminating BOREAL.
pub struct AppState {
    pub runtime: Runtime,
    pub rclone: RcloneState,
}

/// Current state of the BOREAL-managed Rclone installation.
#[derive(Debug, Clone)]
pub enum RcloneState {
    /// Rclone is installed and has been successfully verified.
    Ready(RcloneStatus),

    /// BOREAL attempted to install or verify Rclone, but the operation failed.
    Error(String),
}

impl AppState {
    /// Initialize application state.
    ///
    /// Rclone installation is attempted automatically. A failure does not
    /// prevent BOREAL from starting; instead, the error is recorded so the
    /// WebUI can report it to the user.
    pub fn initialize(
        runtime: Runtime,
    ) -> Self {
        println!("Checking BOREAL-managed Rclone...");

        let rclone = match rclone::ensure_installed(
            &runtime,
        ) {
            Ok(status) => {
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

            Err(error) => {
                let message = error.to_string();

                eprintln!(
                    "Rclone initialization failed: {message}"
                );

                eprintln!(
                    "BOREAL will continue running."
                );

                RcloneState::Error(
                    message,
                )
            }
        };

        Self {
            runtime,
            rclone,
        }
    }
}