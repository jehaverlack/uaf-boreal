use std::{collections::HashSet, path::Path};

use serde::Deserialize;

use crate::{bootstrap::Runtime, database::migration::MigrationSource};

use super::{RcloneError, command, config, identity, inventory, remotes::RemoteKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedDriveDestination {
    pub drive_id: String,
    pub drive_name: String,
    pub folder_id: String,
    pub folder_name: String,
}

/// Validate that a folder ID is accessible through the read-only remote and
/// belongs to one of the account's Shared Drives. This performs only a Shared
/// Drive listing and one exact Google Drive metadata lookup authenticated by
/// the Rclone-managed token; it does not build an inventory.
pub fn validate_destination(
    runtime: &Runtime,
    executable: &Path,
    folder_id: &str,
    require_shared_drive: bool,
) -> Result<SharedDriveDestination, RcloneError> {
    let folder = identity::fetch_google_drive_folder(runtime, folder_id)?;
    if folder.drive_id.is_empty() {
        if require_shared_drive {
            return Err("My Drive migrations require a Shared Drive destination folder".into());
        }
        return Ok(SharedDriveDestination {
            drive_id: String::new(),
            drive_name: "My Drive".to_string(),
            folder_id: folder.id,
            folder_name: folder.name,
        });
    }
    let drives = inventory::discover_shared_drives(runtime, executable)?;
    if drives.is_empty() {
        return Err("The authenticated read-only account cannot access any Shared Drives".into());
    }
    let drive = drives
        .into_iter()
        .find(|drive| drive.id == folder.drive_id)
        .ok_or("The destination belongs to a Shared Drive that is not available to the authenticated read-only account")?;
    let folder_name = if folder.id == drive.id {
        drive.name.clone()
    } else {
        folder.name
    };
    Ok(SharedDriveDestination {
        drive_id: drive.id,
        drive_name: drive.name,
        folder_id: folder.id,
        folder_name,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DestinationEntry {
    name: String,
}

pub fn preflight_copy(
    runtime: &Runtime,
    executable: &Path,
    destination_drive_id: &str,
    destination_folder_id: &str,
    sources: &[MigrationSource],
) -> Result<(), RcloneError> {
    let config_path = config::path(runtime)?;
    let refresh = command::run(
        executable,
        [
            "backend",
            "drives",
            &format!("{}:", RemoteKind::MyDriveRw.name()),
            "--json",
            "--config",
            config_path.to_string_lossy().as_ref(),
        ],
    )?;
    if !refresh.status.success() {
        return Err(format!(
            "My Drive RW authorization failed: {}",
            String::from_utf8_lossy(&refresh.stderr).trim()
        )
        .into());
    }
    let folder = identity::fetch_google_drive_folder_for_remote(
        runtime,
        RemoteKind::MyDriveRw,
        destination_folder_id,
    )?;
    if folder.drive_id != destination_drive_id {
        return Err("The destination now resolves to a different Shared Drive".into());
    }
    if !folder.can_add_children {
        return Err("The My Drive RW account cannot add content to the destination folder".into());
    }

    let destination = destination_remote(destination_drive_id, destination_folder_id);
    let output = command::run(
        executable,
        [
            "lsjson",
            &destination,
            "--max-depth",
            "1",
            "--config",
            config_path.to_string_lossy().as_ref(),
        ],
    )?;
    if !output.status.success() {
        return Err(format!(
            "Unable to inspect the migration destination: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    let entries: Vec<DestinationEntry> = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Unable to parse destination contents: {error}"))?;
    let existing = entries
        .into_iter()
        .map(|entry| entry.name)
        .collect::<HashSet<_>>();
    let mut selected = HashSet::new();
    for source in sources {
        if !selected.insert(source.name.clone()) {
            return Err(format!(
                "Multiple selected items are named '{}'. Select unique top-level names before starting the migration.",
                source.name
            )
            .into());
        }
        if existing.contains(&source.name) {
            return Err(format!(
                "The destination already contains an item named '{}'. BOREAL will not merge or overwrite it.",
                source.name
            )
            .into());
        }
    }
    Ok(())
}

pub fn copy_source(
    runtime: &Runtime,
    executable: &Path,
    source_kind: &str,
    source: &MigrationSource,
    destination_drive_id: &str,
    destination_folder_id: &str,
) -> Result<(), RcloneError> {
    let config_path = config::path(runtime)?;
    let source_remote = if source_kind == "shared-with-me" {
        format!(
            "{},shared_with_me=true:{}",
            RemoteKind::MyDriveRo.name(),
            source.relative_path.trim_start_matches('/')
        )
    } else {
        format!(
            "{}:{}",
            RemoteKind::MyDriveRo.name(),
            source.relative_path.trim_start_matches('/')
        )
    };
    let destination_root = destination_remote(destination_drive_id, destination_folder_id);
    let destination = format!("{destination_root}{}", source.name);
    let operation = if source.is_directory {
        "copy"
    } else {
        "copyto"
    };
    let mut arguments = vec![
        operation.to_string(),
        source_remote,
        destination,
        "--drive-server-side-across-configs".to_string(),
        "--immutable".to_string(),
        "--config".to_string(),
        config_path.to_string_lossy().into_owned(),
    ];
    if source.is_directory {
        arguments.push("--create-empty-src-dirs".to_string());
    }
    let output = command::run(executable, &arguments)?;
    if !output.status.success() {
        return Err(format!(
            "Rclone could not copy '{}': {}",
            source.name,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(())
}

fn destination_remote(drive_id: &str, folder_id: &str) -> String {
    if drive_id.is_empty() {
        format!(
            "{},root_folder_id={folder_id}:",
            RemoteKind::MyDriveRw.name()
        )
    } else {
        format!(
            "{},team_drive={drive_id},root_folder_id={folder_id}:",
            RemoteKind::MyDriveRw.name()
        )
    }
}
