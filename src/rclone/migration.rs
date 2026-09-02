use std::path::Path;

use crate::bootstrap::Runtime;

use super::{RcloneError, identity, inventory};

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
pub fn validate_shared_drive_destination(
    runtime: &Runtime,
    executable: &Path,
    folder_id: &str,
) -> Result<SharedDriveDestination, RcloneError> {
    let drives = inventory::discover_shared_drives(runtime, executable)?;
    if drives.is_empty() {
        return Err("The authenticated read-only account cannot access any Shared Drives".into());
    }

    let folder = identity::fetch_google_drive_folder(runtime, folder_id)?;
    let drive = drives
        .into_iter()
        .find(|drive| drive.id == folder.drive_id)
        .ok_or("The destination belongs to a Shared Drive that is not available to the authenticated read-only account")?;
    Ok(SharedDriveDestination {
        drive_id: drive.id,
        drive_name: drive.name,
        folder_id: folder.id,
        folder_name: folder.name,
    })
}
