use crate::config;

use serde_json::Value;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

const APP_NAME: &str = "boreal";

const DEFAULT_BOREAL: &str = include_str!("../tmpl/boreal.json");

const DEFAULT_CONFIG: &str = include_str!("../tmpl/conf/config.json");

const DEFAULT_SECRETS: &str = include_str!("../tmpl/conf/secrets.json");

pub struct Runtime {
    pub boreal_home: PathBuf,
    pub boreal: Value,
    pub directories: BTreeMap<String, PathBuf>,
}

pub fn initialize() -> Result<Runtime, Box<dyn Error>> {
    /*
     * Determine and create BOREAL_HOME.
     *
     * Linux/macOS:
     *     ~/.boreal
     *
     * Windows:
     *     %LOCALAPPDATA%\boreal
     */
    let boreal_home = get_boreal_home()?;

    ensure_directory(&boreal_home)?;

    /*
     * Initialize boreal.json from the embedded template
     * if it does not already exist.
     */
    let boreal_file = boreal_home.join("boreal.json");

    create_from_template(&boreal_file, DEFAULT_BOREAL)?;

    /*
     * Load boreal.json.
     */
    let mut boreal = config::load_json(&boreal_file)?;

    /*
     * The platform determines BOREAL.DIRS.home.
     */
    let home_changed = config::set_boreal_home(&mut boreal, &boreal_home)?;

    if home_changed {
        config::save_json(&boreal_file, &boreal)?;
    }

    /*
     * Resolve every directory configured under
     * BOREAL.DIRS.
     */
    let mut directories = config::resolve_all_directories(&boreal, &boreal_home)?;

    add_upgrade_safe_directories(&mut directories, &boreal_home)?;

    /*
     * Create every configured directory.
     */
    for path in directories.values() {
        ensure_directory(path)?;
    }

    /*
     * CONF is currently required because config.json
     * and secrets.json are initialized there.
     */
    let conf_dir = directories
        .get("CONF")
        .ok_or("Missing BOREAL.DIRS.conf in boreal.json")?;

    let config_file = conf_dir.join("config.json");

    let secrets_file = conf_dir.join("secrets.json");

    /*
     * Initialize configuration files from their
     * embedded templates.
     *
     * Existing files are never overwritten.
     */
    create_from_template(&config_file, DEFAULT_CONFIG)?;

    create_from_template(&secrets_file, DEFAULT_SECRETS)?;

    /*
     * Restrict secrets.json on Unix-like systems.
     */
    protect_secrets(&secrets_file)?;

    Ok(Runtime {
        boreal_home,
        boreal,
        directories,
    })
}

/// Supply directories introduced after the first BOREAL configuration
/// versions without overwriting an existing boreal.json.
fn add_upgrade_safe_directories(
    directories: &mut BTreeMap<String, PathBuf>,
    boreal_home: &Path,
) -> Result<(), Box<dyn Error>> {
    if !directories.contains_key("BIN") {
        directories.insert("BIN".to_string(), boreal_home.join("bin"));
    }

    if !directories.contains_key("CACHE") {
        let data_dir = directories
            .get("DATA")
            .ok_or("Missing BOREAL.DIRS.data in boreal.json")?;
        directories.insert("CACHE".to_string(), data_dir.join("cache"));
    }

    Ok(())
}

/// Determine the platform-specific BOREAL home.
///
/// Linux:
///     ~/.boreal
///
/// macOS:
///     ~/.boreal
///
/// Windows:
///     %LOCALAPPDATA%\boreal
fn get_boreal_home() -> Result<PathBuf, Box<dyn Error>> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let home = dirs::home_dir().ok_or("Unable to determine user home directory")?;

        return Ok(home.join(format!(".{APP_NAME}")));
    }

    #[cfg(target_os = "windows")]
    {
        let data_dir =
            dirs::data_local_dir().ok_or("Unable to determine local application data directory")?;

        return Ok(data_dir.join(APP_NAME));
    }

    #[allow(unreachable_code)]
    Err("Unsupported operating system".into())
}

/// Create a directory and any missing parent
/// directories.
fn ensure_directory(path: &Path) -> Result<(), Box<dyn Error>> {
    if !path.exists() {
        fs::create_dir_all(path)?;

        println!("Created directory: {}", path.display());
    }

    Ok(())
}

/// Create a file from an embedded template if it
/// does not already exist.
///
/// Existing files are never overwritten.
fn create_from_template(path: &Path, template: &str) -> Result<(), Box<dyn Error>> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, template)?;

    println!("Created file: {}", path.display());

    Ok(())
}

/// Restrict secrets.json access on Unix systems.
///
/// Linux/macOS:
///     mode 0600
///
/// Windows:
///     uses the ACL inherited from %LOCALAPPDATA%.
fn protect_secrets(path: &Path) -> Result<(), Box<dyn Error>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let permissions = fs::Permissions::from_mode(0o600);

        fs::set_permissions(path, permissions)?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplies_upgrade_safe_bin_and_cache_directories() {
        let boreal_home = PathBuf::from("/test/.boreal");
        let mut directories = BTreeMap::new();
        directories.insert("HOME".to_string(), boreal_home.clone());
        directories.insert("DATA".to_string(), boreal_home.join("data"));

        add_upgrade_safe_directories(&mut directories, &boreal_home)
            .expect("upgrade-safe directories should be added");

        assert_eq!(directories.get("BIN"), Some(&boreal_home.join("bin")));
        assert_eq!(
            directories.get("CACHE"),
            Some(&boreal_home.join("data/cache"))
        );
    }

    #[test]
    fn preserves_configured_bin_directory() {
        let boreal_home = PathBuf::from("/test/.boreal");
        let configured_bin = PathBuf::from("/custom/boreal-bin");
        let mut directories = BTreeMap::new();
        directories.insert("HOME".to_string(), boreal_home.clone());
        directories.insert("BIN".to_string(), configured_bin.clone());
        directories.insert("DATA".to_string(), boreal_home.join("data"));

        add_upgrade_safe_directories(&mut directories, &boreal_home)
            .expect("configured directories should remain valid");

        assert_eq!(directories.get("BIN"), Some(&configured_bin));
    }
}
