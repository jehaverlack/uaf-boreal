pub mod command;
pub mod config;
pub mod install;

use std::{
    error::Error,
    path::PathBuf,
};

use crate::bootstrap::Runtime;

/// Current state of the BOREAL-managed Rclone executable.
#[derive(Debug, Clone)]
pub struct RcloneStatus {
    /// Full path to the BOREAL-managed Rclone executable.
    pub path: PathBuf,

    /// Version string reported by `rclone version`.
    ///
    /// Example:
    ///
    ///     rclone v1.75.0
    ///
    pub version: String,
}

/// Return the expected BOREAL-managed Rclone executable path.
///
/// Linux/macOS:
///
///     ~/.boreal/bin/rclone
///
/// Windows:
///
///     %LOCALAPPDATA%\boreal\bin\rclone.exe
///
pub fn executable_path(
    runtime: &Runtime,
) -> Result<PathBuf, Box<dyn Error>> {
    let bin_dir = runtime
        .directories
        .get("BIN")
        .ok_or(
            "BOREAL BIN directory is not configured",
        )?;

    let executable_name = if cfg!(windows) {
        "rclone.exe"
    } else {
        "rclone"
    };

    Ok(
        bin_dir.join(
            executable_name,
        ),
    )
}

/// Detect the BOREAL-managed Rclone installation.
///
/// This function does not search PATH because BOREAL intentionally manages
/// and uses its own private Rclone executable.
///
/// If the executable exists but cannot be successfully executed, this
/// function returns an error rather than reporting it as installed.
pub fn detect(
    runtime: &Runtime,
) -> Result<Option<RcloneStatus>, Box<dyn Error>> {
    let path = executable_path(
        runtime,
    )?;

    if !path.is_file() {
        return Ok(
            None,
        );
    }

    let version = command::version(
        &path,
    )?;

    Ok(
        Some(
            RcloneStatus {
                path,
                version,
            },
        ),
    )
}

/// Ensure that BOREAL has a working Rclone executable.
///
/// If Rclone is already installed and working, it is returned immediately.
///
/// If Rclone is missing, BOREAL downloads and installs a user-local copy.
///
/// If an existing BOREAL-managed executable is present but cannot be
/// executed, BOREAL attempts to replace it with a fresh copy.
pub fn ensure_installed(
    runtime: &Runtime,
) -> Result<RcloneStatus, Box<dyn Error>> {
    match detect(
        runtime,
    ) {
        Ok(
            Some(
                status,
            ),
        ) => {
            return Ok(
                status,
            );
        }

        Ok(
            None,
        ) => {
            println!(
                "BOREAL-managed Rclone was not found."
            );
        }

        Err(
            error,
        ) => {
            eprintln!(
                "Existing BOREAL Rclone installation is not usable: {error}"
            );

            eprintln!(
                "BOREAL will reinstall Rclone."
            );
        }
    }

    let installed_path = install::install(
        runtime,
    )?;

    let version = command::version(
        &installed_path,
    )?;

    Ok(
        RcloneStatus {
            path: installed_path,
            version,
        },
    )
}