use std::{path::Path, process::Command};

use serde_json::Value;

use crate::bootstrap::Runtime;

use super::{config, remotes::RemoteKind, RcloneError};

#[derive(Debug, Clone)]
pub struct RemoteIdentity {
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub account_id: Option<String>,
    pub raw_json: String,
}

pub fn fetch_read_only_account(
    runtime: &Runtime,
    executable: &Path,
) -> Result<RemoteIdentity, RcloneError> {
    let config_path = config::path(runtime)?;
    let output = Command::new(executable)
        .args([
            "config", "userinfo", &format!("{}:", RemoteKind::MyDriveRo.name()),
            "--json", "--timeout", "20s", "--contimeout", "10s", "--config",
            config_path.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|error| format!("Unable to query authenticated Google account: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Unable to query authenticated Google account: {}",
            String::from_utf8_lossy(&output.stderr).trim(),
        ).into());
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Invalid rclone userinfo response: {error}"))?;
    let raw_json = serde_json::to_string(&value)?;
    Ok(RemoteIdentity {
        email: string_field(&value, &["Email", "email", "EmailAddress", "emailAddress"]),
        display_name: string_field(&value, &["Name", "name", "DisplayName", "displayName"]),
        account_id: string_field(&value, &["ID", "Id", "id", "UserID", "userId"]),
        raw_json,
    })
}

fn string_field(value: &Value, names: &[&str]) -> Option<String> {
    names.iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::string_field;

    #[test]
    fn reads_common_userinfo_field_names() {
        let value = serde_json::json!({
            "Email": "user@example.edu",
            "DisplayName": "Example User",
            "ID": "123"
        });
        assert_eq!(string_field(&value, &["email", "Email"]).as_deref(), Some("user@example.edu"));
        assert_eq!(string_field(&value, &["displayName", "DisplayName"]).as_deref(), Some("Example User"));
    }
}
