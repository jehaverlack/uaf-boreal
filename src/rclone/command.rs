use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Output},
};

use super::RcloneError;

/// Execute the BOREAL-managed Rclone executable.
///
/// This is the central process execution function for the
/// Rclone subsystem.
pub fn run<I, S>(executable: &Path, args: I) -> Result<Output, RcloneError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if !executable.is_file() {
        return Err(format!("Rclone executable does not exist: {}", executable.display()).into());
    }

    let output = Command::new(executable)
        .args(args)
        .output()
        .map_err(|error| {
            format!(
                "Unable to execute Rclone at {}: {error}",
                executable.display()
            )
        })?;

    Ok(output)
}

/// Query the Rclone version.
///
/// Returns the first line produced by:
///
///     rclone version
///
/// Example:
///
///     rclone v1.75.0
///
pub fn version(executable: &Path) -> Result<String, RcloneError> {
    let output = run(executable, ["version"])?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        return Err(format!(
            "Rclone version check failed for {}: {}",
            executable.display(),
            stderr.trim()
        )
        .into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let version = stdout
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .ok_or_else(|| {
            format!(
                "Rclone returned no version information: {}",
                executable.display()
            )
        })?;

    Ok(version.to_string())
}
