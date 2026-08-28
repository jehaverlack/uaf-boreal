use std::path::PathBuf;

use crate::bootstrap::Runtime;

use super::RcloneError;

/// Return the path to BOREAL's private Rclone configuration file.
///
/// Linux/macOS:
///
///     ~/.boreal/conf/rclone.conf
///
/// Windows:
///
///     %LOCALAPPDATA%\boreal\conf\rclone.conf
#[allow(dead_code)]
pub fn path(
    runtime: &Runtime,
) -> Result<PathBuf, RcloneError> {
    let conf_dir = runtime
        .directories
        .get("CONF")
        .ok_or(
            "BOREAL CONF directory is not configured",
        )?;

    Ok(
        conf_dir.join(
            "rclone.conf",
        ),
    )
}