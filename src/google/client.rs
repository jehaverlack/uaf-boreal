use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::bootstrap::Runtime;

use super::GoogleError;

#[allow(dead_code)]
#[derive(Clone)]
pub struct GoogleClientConfig {
    pub client_id: String,
    pub(crate) client_secret: String,
    pub project_id: Option<String>,
}

impl fmt::Debug for GoogleClientConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleClientConfig")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[redacted]")
            .field("project_id", &self.project_id)
            .finish()
    }
}

#[derive(Debug, Deserialize)]
struct GoogleCredentialsFile {
    installed: GoogleInstalledCredentials,
}

#[derive(Debug, Deserialize)]
struct GoogleInstalledCredentials {
    client_id: String,
    client_secret: String,

    #[serde(default)]
    project_id: Option<String>,

    auth_uri: String,
    token_uri: String,

    #[serde(default)]
    redirect_uris: Vec<String>,
}

/// Return the BOREAL Google OAuth client configuration path.
///
/// Linux/macOS:
///
///     ~/.boreal/conf/google-client.json
///
/// Windows:
///
///     %LOCALAPPDATA%\boreal\conf\google-client.json
pub fn path(runtime: &Runtime) -> Result<PathBuf, GoogleError> {
    let conf_dir = runtime
        .directories
        .get("CONF")
        .ok_or("BOREAL CONF directory is not configured")?;

    Ok(conf_dir.join("google-client.json"))
}

/// Detect and validate an existing Google OAuth client file.
pub fn detect(runtime: &Runtime) -> Result<Option<GoogleClientConfig>, GoogleError> {
    let config_path = path(runtime)?;

    if !config_path.is_file() {
        return Ok(None);
    }

    let data = fs::read(&config_path).map_err(|error| {
        format!(
            "Unable to read Google client configuration {}: {error}",
            config_path.display()
        )
    })?;

    let config = validate(&data)?;

    Ok(Some(config))
}

/// Validate a Google Desktop OAuth credentials JSON file.
pub fn validate(data: &[u8]) -> Result<GoogleClientConfig, GoogleError> {
    let credentials: GoogleCredentialsFile = serde_json::from_slice(data)
        .map_err(|error| format!("Invalid Google client JSON: {error}"))?;

    let installed = credentials.installed;

    if installed.client_id.trim().is_empty() {
        return Err("Google client_id is missing".into());
    }

    if !installed.client_id.ends_with(".apps.googleusercontent.com") {
        return Err("Google client_id does not appear to be a valid OAuth Client ID".into());
    }

    if installed.client_secret.trim().is_empty() {
        return Err("Google client_secret is missing".into());
    }

    if installed.auth_uri.trim().is_empty() {
        return Err("Google auth_uri is missing".into());
    }

    if installed.token_uri.trim().is_empty() {
        return Err("Google token_uri is missing".into());
    }

    /*
     * Desktop OAuth clients normally contain redirect URI information.
     * We accept Google's generated values without requiring a specific URI.
     */
    let _ = installed.redirect_uris;

    Ok(GoogleClientConfig {
        client_id: installed.client_id,
        client_secret: installed.client_secret,
        project_id: installed.project_id,
    })
}

/// Validate and save a Google OAuth credentials JSON file.
///
/// The original Google-generated JSON is preserved.
pub fn import(runtime: &Runtime, data: &[u8]) -> Result<GoogleClientConfig, GoogleError> {
    let config = validate(data)?;

    let config_path = path(runtime)?;

    let parent = config_path
        .parent()
        .ok_or("Unable to determine Google client configuration directory")?;

    fs::create_dir_all(parent)?;

    fs::write(&config_path, data).map_err(|error| {
        format!(
            "Unable to save Google client configuration {}: {error}",
            config_path.display()
        )
    })?;

    set_private_permissions(&config_path)?;

    println!(
        "Google OAuth client configuration saved: {}",
        config_path.display()
    );

    Ok(config)
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), GoogleError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();

    permissions.set_mode(0o600);

    fs::set_permissions(path, permissions)?;

    Ok(())
}

#[cfg(windows)]
fn set_private_permissions(_path: &Path) -> Result<(), GoogleError> {
    Ok(())
}
