use std::{fs, path::Path, process::Command, time::Duration};

use serde_json::Value;

use crate::bootstrap::Runtime;

use super::{RcloneError, config, remotes::RemoteKind};

#[derive(Debug, Clone)]
pub struct RemoteIdentity {
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub account_id: Option<String>,
    pub raw_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleSheetLocation {
    pub spreadsheet_id: String,
    pub gid: String,
}

pub fn parse_google_sheet_url(url: &str) -> Result<GoogleSheetLocation, RcloneError> {
    let marker = "docs.google.com/spreadsheets/d/";
    let start = url
        .find(marker)
        .ok_or("Directory source must be a Google Sheets URL")?
        + marker.len();
    let remainder = &url[start..];
    let spreadsheet_id = remainder.split('/').next().unwrap_or("").trim();
    if spreadsheet_id.is_empty()
        || !spreadsheet_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err("Google Sheets URL contains an invalid spreadsheet ID".into());
    }
    let gid = url
        .split(['?', '#', '&'])
        .filter_map(|part| part.strip_prefix("gid="))
        .next()
        .unwrap_or("0");
    if gid.is_empty() || !gid.chars().all(|character| character.is_ascii_digit()) {
        return Err("Google Sheets URL contains an invalid worksheet gid".into());
    }
    Ok(GoogleSheetLocation {
        spreadsheet_id: spreadsheet_id.to_string(),
        gid: gid.to_string(),
    })
}

pub fn download_google_sheet_csv(
    runtime: &Runtime,
    url: &str,
) -> Result<(GoogleSheetLocation, Vec<u8>), RcloneError> {
    let location = parse_google_sheet_url(url)?;
    let config_path = config::path(runtime)?;
    let access_token = read_access_token(&config_path)?;
    let export_url = format!(
        "https://docs.google.com/spreadsheets/d/{}/export?format=csv&gid={}",
        location.spreadsheet_id, location.gid,
    );
    let response = google_client()?
        .get(export_url)
        .bearer_auth(access_token)
        .send()
        .map_err(|error| format!("Unable to download directory spreadsheet: {error}"))?;
    let status = response.status();
    let bytes = response
        .bytes()
        .map_err(|error| format!("Unable to read directory spreadsheet download: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "Directory spreadsheet download returned {status}; verify Viewer access and Shared Drive download permissions"
        ).into());
    }
    if bytes.len() > 10 * 1024 * 1024 {
        return Err("Directory spreadsheet CSV is larger than 10 MiB".into());
    }
    Ok((location, bytes.to_vec()))
}

pub fn fetch_read_only_account(
    runtime: &Runtime,
    executable: &Path,
) -> Result<RemoteIdentity, RcloneError> {
    let config_path = config::path(runtime)?;
    let output = Command::new(executable)
        .args([
            "config",
            "userinfo",
            &format!("{}:", RemoteKind::MyDriveRo.name()),
            "--json",
            "--timeout",
            "20s",
            "--contimeout",
            "10s",
            "--config",
            config_path.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|error| format!("Unable to query authenticated Google account: {error}"))?;
    if output.status.success() {
        return parse_rclone_userinfo(&output.stdout);
    }

    log::debug!(
        "Rclone userinfo unavailable; trying Google Drive about API: {}",
        String::from_utf8_lossy(&output.stderr).trim(),
    );
    fetch_drive_about(&config_path)
}

fn parse_rclone_userinfo(bytes: &[u8]) -> Result<RemoteIdentity, RcloneError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid rclone userinfo response: {error}"))?;
    identity_from_value(value)
}

fn fetch_drive_about(config_path: &Path) -> Result<RemoteIdentity, RcloneError> {
    let access_token = read_access_token(config_path)?;
    let client = google_client()?;
    let response = client
        .get("https://www.googleapis.com/drive/v3/about?fields=user%28displayName%2CemailAddress%2CpermissionId%29")
        .bearer_auth(access_token)
        .send()
        .map_err(|error| format!("Google Drive account lookup failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("Unable to read Google Drive account response: {error}"))?;
    if !status.is_success() {
        return Err(format!("Google Drive account lookup returned {status}: {body}").into());
    }
    let value: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Invalid Google Drive account response: {error}"))?;
    let user = value
        .get("user")
        .cloned()
        .ok_or("Google Drive account response did not include a user")?;
    let identity = identity_from_value(user)?;
    if identity.email.is_none() {
        return Err("Google Drive returned user information without an email address".into());
    }
    Ok(identity)
}

fn google_client() -> Result<reqwest::blocking::Client, RcloneError> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("Unable to create Google API client: {error}").into())
}

fn read_access_token(config_path: &Path) -> Result<String, RcloneError> {
    let config_text = fs::read_to_string(config_path).map_err(|error| {
        format!("Unable to read rclone configuration for Google access: {error}")
    })?;
    let token_json = remote_setting(&config_text, RemoteKind::MyDriveRo.name(), "token")
        .ok_or("The read-only rclone remote does not contain an OAuth token")?;
    let token: Value = serde_json::from_str(token_json)
        .map_err(|error| format!("Invalid OAuth token in the read-only rclone remote: {error}"))?;
    token
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| "The read-only rclone remote OAuth token has no access token".into())
}

fn identity_from_value(value: Value) -> Result<RemoteIdentity, RcloneError> {
    let raw_json = serde_json::to_string(&value)?;
    Ok(RemoteIdentity {
        email: string_field(&value, &["Email", "email", "EmailAddress", "emailAddress"]),
        display_name: string_field(&value, &["Name", "name", "DisplayName", "displayName"]),
        account_id: string_field(
            &value,
            &["ID", "Id", "id", "UserID", "userId", "permissionId"],
        ),
        raw_json,
    })
}

fn remote_setting<'a>(config: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut in_section = false;
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_section = &line[1..line.len() - 1] == section;
            continue;
        }
        if !in_section || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() == key {
            return Some(value.trim());
        }
    }
    None
}

fn string_field(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{parse_google_sheet_url, remote_setting, string_field};

    #[test]
    fn reads_common_userinfo_field_names() {
        let value = serde_json::json!({
            "Email": "user@example.edu",
            "DisplayName": "Example User",
            "ID": "123"
        });
        assert_eq!(
            string_field(&value, &["email", "Email"]).as_deref(),
            Some("user@example.edu")
        );
        assert_eq!(
            string_field(&value, &["displayName", "DisplayName"]).as_deref(),
            Some("Example User")
        );
    }

    #[test]
    fn reads_token_only_from_requested_remote() {
        let config = "[other]\ntoken = wrong\n[my-drive-ro]\ntype = drive\ntoken = {\"access_token\":\"secret\"}\n";
        assert_eq!(
            remote_setting(config, "my-drive-ro", "token"),
            Some("{\"access_token\":\"secret\"}"),
        );
    }

    #[test]
    fn parses_google_sheet_and_worksheet_ids() {
        let location = parse_google_sheet_url(
            "https://docs.google.com/spreadsheets/d/abc_DEF-123/edit?gid=42#gid=42",
        )
        .expect("Google Sheets URL should parse");
        assert_eq!(location.spreadsheet_id, "abc_DEF-123");
        assert_eq!(location.gid, "42");
    }
}
