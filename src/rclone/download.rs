use std::{path::Path, process::Command};

use crate::rclone::{RcloneError, remotes::RemoteKind};

pub struct DownloadRequest<'a> {
    pub executable: &'a Path,
    pub config_path: &'a Path,
    pub relative_path: &'a str,
    pub destination: &'a Path,
    pub is_directory: bool,
    pub shared_with_me: bool,
    pub shared_drive_id: Option<&'a str>,
}

pub fn copy_item(request: DownloadRequest<'_>) -> Result<(), RcloneError> {
    let source = format!("{}:{}", RemoteKind::MyDriveRo.name(), request.relative_path);
    let mut command = Command::new(request.executable);
    command.arg(if request.is_directory {
        "copy"
    } else {
        "copyto"
    });
    command.args([
        source.as_str(),
        request.destination.to_string_lossy().as_ref(),
    ]);
    command.args(["--config", request.config_path.to_string_lossy().as_ref()]);
    command.arg("--create-empty-src-dirs");
    if request.shared_with_me {
        command.arg("--drive-shared-with-me");
    }
    if let Some(drive_id) = request.shared_drive_id {
        command.args(["--drive-team-drive", drive_id, "--drive-root-folder-id", ""]);
    }

    let output = command
        .output()
        .map_err(|error| format!("Unable to start Rclone download: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Rclone download failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(())
}
