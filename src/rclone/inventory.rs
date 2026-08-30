use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::Deserialize;

use crate::bootstrap::Runtime;

use super::{config, remotes::RemoteKind, RcloneError};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DriveItem {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub is_dir: bool,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub mod_time: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

pub fn fetch_my_drive(
    runtime: &Runtime,
    executable: &Path,
    scan_id: i64,
    include_permissions: bool,
) -> Result<Vec<DriveItem>, RcloneError> {
    if !executable.is_file() {
        return Err(format!("Rclone executable does not exist: {}", executable.display()).into());
    }

    let cache_dir = runtime.directories.get("CACHE")
        .ok_or("BOREAL CACHE directory is not configured")?;
    fs::create_dir_all(cache_dir)?;
    let cache_path = cache_dir.join(format!("my-drive-inventory-{scan_id}.json"));
    let output_file = File::create(&cache_path)?;
    let config_path = config::path(runtime)?;

    let mut command = Command::new(executable);
    command.args([
        "lsjson",
        &format!("{}:", RemoteKind::MyDriveRo.name()),
        "--recursive",
        "--metadata",
        "--drive-metadata-owner=read",
        "--fast-list",
        "--config",
        config_path.to_string_lossy().as_ref(),
    ]);
    if include_permissions {
        command.arg("--drive-metadata-permissions=read");
    }

    let output = command.stdout(Stdio::from(output_file)).stderr(Stdio::piped()).output()
        .map_err(|error| format!("Unable to execute My Drive inventory: {error}"))?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let _ = fs::remove_file(&cache_path);
        return Err(format!("Rclone My Drive inventory failed: {message}").into());
    }

    let result = parse(&cache_path);
    let _ = fs::remove_file(&cache_path);
    result
}

fn parse(path: &PathBuf) -> Result<Vec<DriveItem>, RcloneError> {
    let file = File::open(path)?;
    let items: Vec<DriveItem> = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("Unable to parse Rclone inventory: {error}"))?;

    if let Some(item) = items.iter().find(|item| item.id.trim().is_empty()) {
        return Err(format!("Rclone returned an item without a Drive ID: {}", item.path).into());
    }
    Ok(items)
}
