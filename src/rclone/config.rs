use std::{
    error::Error,
    path::PathBuf,
};

use crate::bootstrap::Runtime;

/// Return the path to BOREAL's private Rclone configuration file.
///
/// Linux/macOS:
///
///     ~/.boreal/conf/rclone.conf
///
/// Windows:
///
///     %LOCALAPPDATA%\boreal\conf\rclone.conf
///
/// BOREAL will always invoke Rclone with this configuration explicitly rather
/// than relying on Rclone's normal per-user configuration location.
#[allow(dead_code)]
pub fn path(
    runtime: &Runtime,
) -> Result<PathBuf, Box<dyn Error>> {
    let conf_dir = runtime
        .directories
        .get("conf")
        .ok_or(
            "BOREAL conf directory is not configured",
        )?;

    Ok(
        conf_dir.join(
            "rclone.conf",
        ),
    )
}