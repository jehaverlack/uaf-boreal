use serde_json::Value;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

const APP_NAME: &str = "boreal";

const DEFAULT_BOREAL: &str =
    include_str!("../tmpl/boreal.json");

const DEFAULT_CONFIG: &str =
    include_str!("../tmpl/conf/config.json");

const DEFAULT_SECRETS: &str =
    include_str!("../tmpl/conf/secrets.json");

fn main() -> Result<(), Box<dyn Error>> {
    let boreal_home = get_boreal_home()?;

    ensure_directory(&boreal_home)?;

    let boreal_file = boreal_home.join("boreal.json");

    create_from_template(
        &boreal_file,
        DEFAULT_BOREAL,
    )?;

    let mut boreal = load_json(&boreal_file)?;

    set_boreal_home(
        &mut boreal,
        &boreal_home,
    )?;

    save_json(
        &boreal_file,
        &boreal,
    )?;

    let conf_dir = resolve_config_dir(
        &boreal,
        "conf",
        &boreal_home,
    )?;

    let data_dir = resolve_config_dir(
        &boreal,
        "data",
        &boreal_home,
    )?;

    let logs_dir = resolve_config_dir(
        &boreal,
        "logs",
        &boreal_home,
    )?;

    ensure_directory(&conf_dir)?;
    ensure_directory(&data_dir)?;
    ensure_directory(&logs_dir)?;

    let config_file = conf_dir.join("config.json");
    let secrets_file = conf_dir.join("secrets.json");

    create_from_template(
        &config_file,
        DEFAULT_CONFIG,
    )?;

    create_from_template(
        &secrets_file,
        DEFAULT_SECRETS,
    )?;

    protect_secrets(&secrets_file)?;

    println!("BOREAL initialized.");
    println!("BOREAL home : {}", boreal_home.display());
    println!("Config dir  : {}", conf_dir.display());
    println!("Data dir    : {}", data_dir.display());
    println!("Logs dir    : {}", logs_dir.display());

    Ok(())
}

/// Determine the platform-specific BOREAL home directory.
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
        let home = dirs::home_dir()
            .ok_or("Unable to determine user home directory")?;

        return Ok(home.join(".boreal"));
    }

    #[cfg(target_os = "windows")]
    {
        let data_dir = dirs::data_local_dir()
            .ok_or("Unable to determine local application data directory")?;

        return Ok(data_dir.join(APP_NAME));
    }

    #[allow(unreachable_code)]
    Err("Unsupported operating system".into())
}

/// Create a directory and any missing parent directories.
fn ensure_directory(path: &Path) -> Result<(), Box<dyn Error>> {
    if !path.exists() {
        fs::create_dir_all(path)?;

        println!(
            "Created directory: {}",
            path.display()
        );
    }

    Ok(())
}

/// Create a file from an embedded template if it does not
/// already exist.
///
/// Existing user configuration is never overwritten.
fn create_from_template(
    path: &Path,
    template: &str,
) -> Result<(), Box<dyn Error>> {
    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, template)?;

    println!(
        "Created file: {}",
        path.display()
    );

    Ok(())
}

/// Load a JSON file.
fn load_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    let contents = fs::read_to_string(path)?;

    let value: Value =
        serde_json::from_str(&contents)?;

    Ok(value)
}

/// Save JSON in a human-readable format.
fn save_json(
    path: &Path,
    value: &Value,
) -> Result<(), Box<dyn Error>> {
    let json =
        serde_json::to_string_pretty(value)?;

    fs::write(
        path,
        format!("{json}\n"),
    )?;

    Ok(())
}

/// Populate BOREAL.DIRS.home with the platform-specific
/// BOREAL home directory.
fn set_boreal_home(
    boreal: &mut Value,
    boreal_home: &Path,
) -> Result<(), Box<dyn Error>> {
    let home = boreal
        .get_mut("BOREAL")
        .and_then(|v| v.get_mut("DIRS"))
        .and_then(|v| v.get_mut("home"))
        .ok_or(
            "Missing BOREAL.DIRS.home in boreal.json",
        )?;

    *home = Value::String(
        boreal_home
            .to_string_lossy()
            .into_owned(),
    );

    Ok(())
}

/// Read one of the directory definitions from:
///
/// BOREAL.DIRS.<name>
///
/// and expand the BOREAL-local pseudo-variable HOME.
fn resolve_config_dir(
    boreal: &Value,
    name: &str,
    boreal_home: &Path,
) -> Result<PathBuf, Box<dyn Error>> {
    let value = boreal
        .get("BOREAL")
        .and_then(|v| v.get("DIRS"))
        .and_then(|v| v.get(name))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!(
                "Missing BOREAL.DIRS.{name} in boreal.json"
            )
        })?;

    resolve_home_path(
        value,
        boreal_home,
    )
}

/// Expand the BOREAL-local pseudo-variable HOME.
///
/// Examples:
///
/// HOME
/// HOME/conf
/// HOME/data
/// HOME/logs
///
/// HOME refers to BOREAL.DIRS.home and is not the operating
/// system HOME environment variable.
fn resolve_home_path(
    value: &str,
    boreal_home: &Path,
) -> Result<PathBuf, Box<dyn Error>> {
    if value == "HOME" {
        return Ok(
            boreal_home.to_path_buf(),
        );
    }

    if let Some(relative) =
        value.strip_prefix("HOME/")
    {
        return Ok(
            boreal_home.join(relative),
        );
    }

    if let Some(relative) =
        value.strip_prefix(r"HOME\")
    {
        return Ok(
            boreal_home.join(relative),
        );
    }

    Ok(PathBuf::from(value))
}

/// Restrict access to secrets.json on Unix systems.
///
/// Linux/macOS:
///     chmod 0600 secrets.json
///
/// Windows:
///     Uses the ACL inherited from %LOCALAPPDATA%.
fn protect_secrets(
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let permissions =
            fs::Permissions::from_mode(0o600);

        fs::set_permissions(
            path,
            permissions,
        )?;
    }

    Ok(())
}