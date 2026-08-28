use std::{
    error::Error,
    ffi::OsStr,
    path::Path,
    process::{
        Command,
        Output,
    },
};

/// Execute the BOREAL-managed Rclone executable.
///
/// This is the central command execution function for the Rclone subsystem.
///
/// Keeping Rclone process execution here means the rest of BOREAL does not
/// need to construct `std::process::Command` instances directly.
pub fn run<I, S>(
    executable: &Path,
    args: I,
) -> Result<Output, Box<dyn Error>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if !executable.is_file() {
        return Err(
            format!(
                "Rclone executable does not exist: {}",
                executable.display()
            )
            .into(),
        );
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
/// The first output line from Rclone is returned.
///
/// Example:
///
///     rclone v1.75.0
///
pub fn version(
    executable: &Path,
) -> Result<String, Box<dyn Error>> {
    let output = run(
        executable,
        ["version"],
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(
            &output.stderr,
        );

        return Err(
            format!(
                "Rclone version check failed for {}: {}",
                executable.display(),
                stderr.trim()
            )
            .into(),
        );
    }

    let stdout = String::from_utf8_lossy(
        &output.stdout,
    );

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