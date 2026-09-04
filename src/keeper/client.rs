use std::{collections::HashMap, path::PathBuf, process::Command};

use serde::Deserialize;

use crate::bootstrap::Runtime;

pub type KeeperError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone)]
pub struct SharedFolder {
    pub folder_uid: String,
    pub name: String,
    pub folder_type: String,
    pub folder_path: String,
    pub access: Vec<FolderAccess>,
}

#[derive(Debug, Clone)]
pub struct FolderAccess {
    pub shared_to: String,
    pub permissions: String,
    pub target_kind: String,
}

#[derive(Debug, Deserialize)]
struct SharedFolderReportRow {
    #[serde(rename = "Folder UID")]
    folder_uid: String,
    #[serde(rename = "Folder Name")]
    folder_name: String,
    #[serde(rename = "Type", default)]
    folder_type: String,
    #[serde(rename = "Shared To", default)]
    shared_to: String,
    #[serde(rename = "Permissions", default)]
    permissions: String,
    #[serde(rename = "Folder Path", default)]
    folder_path: String,
}

pub fn config_path(runtime: &Runtime) -> Result<PathBuf, KeeperError> {
    let conf = runtime
        .directories
        .get("CONF")
        .ok_or("BOREAL CONF directory is not configured")?
        .join("keeper");
    std::fs::create_dir_all(&conf)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&conf, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(conf.join("config.json"))
}

pub fn default_command(runtime: &Runtime) -> String {
    let filename = if cfg!(windows) {
        "keeper.exe"
    } else {
        "keeper"
    };
    runtime
        .directories
        .get("BIN")
        .map(|bin| bin.join(filename))
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| filename.to_string())
}

pub fn version(command: &str) -> Result<String, KeeperError> {
    let output = Command::new(command_path(command)?)
        .arg("--version")
        .output()?;
    if !output.status.success() {
        return Err(command_error("Keeper Commander version check failed", &output.stderr).into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn login_status(runtime: &Runtime, command: &str) -> Result<String, KeeperError> {
    let output = keeper_command(runtime, command)?
        .arg("login-status")
        .output()?;
    if !output.status.success() {
        return Err(command_error("Keeper login check failed", &output.stderr).into());
    }
    let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if status.is_empty() {
        return Err("Keeper Commander did not report an authenticated session".into());
    }
    Ok(status)
}

pub fn shared_folders(runtime: &Runtime, command: &str) -> Result<Vec<SharedFolder>, KeeperError> {
    let output = keeper_command(runtime, command)?
        .args(["share-report", "--folders", "--format", "json"])
        .output()?;
    if !output.status.success() {
        return Err(command_error("Keeper shared-folder report failed", &output.stderr).into());
    }
    let stdout = String::from_utf8(output.stdout)?;
    let json = json_array(&stdout).ok_or(
        "Keeper Commander did not return a JSON shared-folder report; sign in and try again",
    )?;
    let rows: Vec<SharedFolderReportRow> = serde_json::from_str(json)?;
    let mut folders: HashMap<String, SharedFolder> = HashMap::new();
    for row in rows {
        if row.folder_uid.trim().is_empty() {
            continue;
        }
        let folder = folders
            .entry(row.folder_uid.clone())
            .or_insert_with(|| SharedFolder {
                folder_uid: row.folder_uid,
                name: row.folder_name,
                folder_type: row.folder_type,
                folder_path: row.folder_path,
                access: Vec::new(),
            });
        if !row.shared_to.trim().is_empty() {
            let target_kind = if row.shared_to.starts_with("(Team User)") {
                "team-user"
            } else if row.shared_to.starts_with("(Team)") {
                "team"
            } else {
                "user"
            };
            folder.access.push(FolderAccess {
                shared_to: row.shared_to,
                permissions: row.permissions,
                target_kind: target_kind.to_string(),
            });
        }
    }
    let mut folders: Vec<_> = folders.into_values().collect();
    folders.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(folders)
}

fn keeper_command(runtime: &Runtime, command: &str) -> Result<Command, KeeperError> {
    if command.trim().is_empty() {
        return Err("Configure the Keeper Commander executable path".into());
    }
    let mut process = Command::new(command_path(command)?);
    process
        .arg("--silent")
        .arg("--config")
        .arg(config_path(runtime)?);
    Ok(process)
}

fn command_path(command: &str) -> Result<PathBuf, KeeperError> {
    let command = command.trim();
    if command.is_empty() {
        return Err("Configure the Keeper Commander executable path".into());
    }
    if command == "~" || command.starts_with("~/") || command.starts_with("~\\") {
        let home = dirs::home_dir().ok_or("Unable to determine the user home directory")?;
        let relative = command
            .trim_start_matches('~')
            .trim_start_matches(['/', '\\']);
        return Ok(home.join(relative));
    }
    Ok(PathBuf::from(command))
}

fn json_array(output: &str) -> Option<&str> {
    let start = output.find('[')?;
    let end = output.rfind(']')?;
    (end >= start).then_some(&output[start..=end])
}

fn command_error(prefix: &str, stderr: &[u8]) -> String {
    let detail = String::from_utf8_lossy(stderr);
    let detail = detail.trim();
    if detail.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}: {detail}")
    }
}

#[cfg(test)]
mod tests {
    use super::{SharedFolderReportRow, command_path, json_array};

    #[test]
    fn parses_only_the_expected_shared_folder_metadata() {
        let json = r#"[{"Folder UID":"uid","Folder Name":"Operations","Type":"Shared Folder","Shared To":"person@example.edu","Permissions":"Can Manage Users","Folder Path":"/Operations","password":"must-not-be-modeled"}]"#;
        let rows: Vec<SharedFolderReportRow> = serde_json::from_str(json).unwrap();
        assert_eq!(rows[0].folder_name, "Operations");
        assert_eq!(rows[0].shared_to, "person@example.edu");
    }

    #[test]
    fn extracts_json_from_commander_output() {
        assert_eq!(json_array("notice\n[]\n"), Some("[]"));
    }

    #[test]
    fn expands_a_user_relative_commander_path() {
        let path = command_path("~/.boreal/keeper-env/bin/keeper").unwrap();
        assert!(path.ends_with(".boreal/keeper-env/bin/keeper"));
        assert!(!path.to_string_lossy().starts_with('~'));
    }
}
