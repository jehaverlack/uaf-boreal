use std::{
    path::Path,
};

use serde_json::Value;

use crate::{
    bootstrap::Runtime,
    google::client::GoogleClientConfig,
};

use super::{
    command,
    config,
    RcloneError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteKind {
    MyDriveRw,
    MyDriveRo,
}

impl RemoteKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::MyDriveRw => "my-drive-rw",
            Self::MyDriveRo => "my-drive-ro",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::MyDriveRw => "My Drive RW",
            Self::MyDriveRo => "My Drive RO",
        }
    }

    fn scope(self) -> &'static str {
        match self {
            Self::MyDriveRw => "drive",
            Self::MyDriveRo => "drive.readonly",
        }
    }
}

#[derive(Debug, Clone)]
pub enum RemoteState {
    Waiting,
    NotConfigured,
    Configuring,
    Ready,
    Conflict(String),
    Error(String),
}

enum DetectedRemote {
    Missing,
    NeedsAuthorization,
    Ready,
}

pub fn detect(
    runtime: &Runtime,
    executable: &Path,
    client: &GoogleClientConfig,
    kind: RemoteKind,
) -> RemoteState {
    match inspect(runtime, executable, client, kind) {
        Ok(DetectedRemote::Missing | DetectedRemote::NeedsAuthorization) => {
            RemoteState::NotConfigured
        }
        Ok(DetectedRemote::Ready) => RemoteState::Ready,
        Err(error) => RemoteState::Conflict(error.to_string()),
    }
}

pub fn configure(
    runtime: &Runtime,
    executable: &Path,
    client: &GoogleClientConfig,
    kind: RemoteKind,
) -> Result<(), RcloneError> {
    let config_path = config::path(runtime)?;
    let detected = inspect(runtime, executable, client, kind)?;

    if matches!(detected, DetectedRemote::Ready) {
        return Ok(());
    }

    let output = match detected {
        DetectedRemote::Missing => command::run(
            executable,
            [
                "config",
                "create",
                kind.name(),
                "drive",
                "client_id",
                &client.client_id,
                "client_secret",
                &client.client_secret,
                "scope",
                kind.scope(),
                "--config",
                config_path.to_string_lossy().as_ref(),
            ],
        )?,
        DetectedRemote::NeedsAuthorization => command::run(
            executable,
            [
                "config",
                "reconnect",
                &format!("{}:", kind.name()),
                "--config",
                config_path.to_string_lossy().as_ref(),
            ],
        )?,
        DetectedRemote::Ready => unreachable!(),
    };

    if config_path.is_file() {
        protect_config(&config_path)?;
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Rclone could not configure {}: {}",
            kind.label(),
            stderr.trim()
        ).into());
    }

    match inspect(runtime, executable, client, kind)? {
        DetectedRemote::Ready => Ok(()),
        _ => Err(format!(
            "Rclone finished without a usable {} remote",
            kind.label()
        ).into()),
    }
}

fn inspect(
    runtime: &Runtime,
    executable: &Path,
    client: &GoogleClientConfig,
    kind: RemoteKind,
) -> Result<DetectedRemote, RcloneError> {
    let config_path = config::path(runtime)?;

    if !config_path.is_file() {
        return Ok(DetectedRemote::Missing);
    }

    let output = command::run(
        executable,
        [
            "config",
            "dump",
            "--config",
            config_path.to_string_lossy().as_ref(),
        ],
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Unable to inspect Rclone remotes: {}", stderr.trim()).into());
    }

    let remotes: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Invalid Rclone configuration output: {error}"))?;
    let Some(remote) = remotes.get(kind.name()) else {
        return Ok(DetectedRemote::Missing);
    };

    classify_remote(remote, client, kind)
}

fn classify_remote(
    remote: &Value,
    client: &GoogleClientConfig,
    kind: RemoteKind,
) -> Result<DetectedRemote, RcloneError> {

    check_value(remote, "type", "drive", kind)?;
    check_value(remote, "scope", kind.scope(), kind)?;
    check_value(remote, "client_id", &client.client_id, kind)?;

    match remote.get("token").and_then(Value::as_str) {
        Some(token) if !token.trim().is_empty() => Ok(DetectedRemote::Ready),
        _ => Ok(DetectedRemote::NeedsAuthorization),
    }
}

fn check_value(
    remote: &Value,
    key: &str,
    expected: &str,
    kind: RemoteKind,
) -> Result<(), RcloneError> {
    let actual = remote.get(key).and_then(Value::as_str).unwrap_or("");
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "Remote '{}' has {key}='{actual}', expected '{expected}'",
            kind.name()
        ).into())
    }
}

#[cfg(unix)]
fn protect_config(path: &Path) -> Result<(), RcloneError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(0o600),
    )?;
    Ok(())
}

#[cfg(not(unix))]
fn protect_config(_path: &Path) -> Result<(), RcloneError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn client() -> GoogleClientConfig {
        GoogleClientConfig {
            client_id: "test.apps.googleusercontent.com".to_string(),
            client_secret: "secret".to_string(),
            project_id: None,
        }
    }

    #[test]
    fn uses_expected_remote_names_and_scopes() {
        assert_eq!(RemoteKind::MyDriveRw.name(), "my-drive-rw");
        assert_eq!(RemoteKind::MyDriveRw.scope(), "drive");
        assert_eq!(RemoteKind::MyDriveRo.name(), "my-drive-ro");
        assert_eq!(RemoteKind::MyDriveRo.scope(), "drive.readonly");
    }

    #[test]
    fn recognizes_a_ready_remote() {
        let remote = json!({
            "type": "drive",
            "scope": "drive.readonly",
            "client_id": "test.apps.googleusercontent.com",
            "token": "{\"refresh_token\":\"token\"}"
        });

        assert!(matches!(
            classify_remote(&remote, &client(), RemoteKind::MyDriveRo),
            Ok(DetectedRemote::Ready)
        ));
    }

    #[test]
    fn rejects_a_conflicting_scope() {
        let remote = json!({
            "type": "drive",
            "scope": "drive",
            "client_id": "test.apps.googleusercontent.com",
            "token": "token"
        });

        let error = classify_remote(&remote, &client(), RemoteKind::MyDriveRo)
            .err()
            .expect("scope conflict should fail");
        assert!(error.to_string().contains("expected 'drive.readonly'"));
    }
}
