use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
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
    /*
     * Bootstrap BOREAL_HOME.
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
     * Initialize boreal.json from the embedded template if
     * this is a new installation.
     */
    let boreal_file = boreal_home.join("boreal.json");

    create_from_template(
        &boreal_file,
        DEFAULT_BOREAL,
    )?;

    /*
     * Load boreal.json.
     */
    let mut boreal = load_json(&boreal_file)?;

    /*
     * BOREAL.DIRS.home is determined by the platform rather
     * than by the template.
     */
    let home_changed = set_boreal_home(
        &mut boreal,
        &boreal_home,
    )?;

    if home_changed {
        save_json(
            &boreal_file,
            &boreal,
        )?;
    }

    /*
     * Resolve every directory defined in BOREAL.DIRS.
     *
     * Examples:
     *
     * HOME/conf
     * HOME/data
     * DATA/sqlite
     * DATA/cache
     *
     * Directory names automatically become pseudo-variables
     * using their uppercase names.
     */
    let directories = resolve_all_directories(
        &boreal,
        &boreal_home,
    )?;

    /*
     * Create every configured directory.
     */
    for path in directories.values() {
        ensure_directory(path)?;
    }

    /*
     * conf is currently required because BOREAL initializes
     * config.json and secrets.json there.
     */
    let conf_dir = directories
        .get("CONF")
        .ok_or("Missing BOREAL.DIRS.conf in boreal.json")?;

    let config_file = conf_dir.join("config.json");
    let secrets_file = conf_dir.join("secrets.json");

    /*
     * Initialize configuration files from templates.
     *
     * Existing files are never overwritten.
     */
    create_from_template(
        &config_file,
        DEFAULT_CONFIG,
    )?;

    create_from_template(
        &secrets_file,
        DEFAULT_SECRETS,
    )?;

    /*
     * Restrict secrets.json on Unix-like systems.
     */
    protect_secrets(&secrets_file)?;

    /*
     * Startup summary.
     */
    println!("BOREAL initialized.");
    println!("BOREAL home: {}", boreal_home.display());

    println!("Configured directories:");

    for (name, path) in &directories {
        println!(
            "  {:<12} {}",
            name,
            path.display()
        );
    }

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

        return Ok(
            home.join(format!(".{APP_NAME}"))
        );
    }

    #[cfg(target_os = "windows")]
    {
        let data_dir = dirs::data_local_dir()
            .ok_or(
                "Unable to determine local application data directory",
            )?;

        return Ok(
            data_dir.join(APP_NAME)
        );
    }

    #[allow(unreachable_code)]
    Err("Unsupported operating system".into())
}

/// Create a directory and any missing parent directories.
fn ensure_directory(
    path: &Path,
) -> Result<(), Box<dyn Error>> {
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
/// Existing files are never overwritten.
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

    fs::write(
        path,
        template,
    )?;

    println!(
        "Created file: {}",
        path.display()
    );

    Ok(())
}

/// Load a JSON file.
fn load_json(
    path: &Path,
) -> Result<Value, Box<dyn Error>> {
    let contents =
        fs::read_to_string(path)?;

    let value =
        serde_json::from_str(&contents)?;

    Ok(value)
}

/// Save JSON using human-readable formatting.
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

/// Set BOREAL.DIRS.home to the platform-resolved BOREAL_HOME.
///
/// Returns true if boreal.json needs to be rewritten.
fn set_boreal_home(
    boreal: &mut Value,
    boreal_home: &Path,
) -> Result<bool, Box<dyn Error>> {
    let dirs = boreal
        .get_mut("BOREAL")
        .and_then(|value| value.get_mut("DIRS"))
        .and_then(Value::as_object_mut)
        .ok_or("Missing BOREAL.DIRS in boreal.json")?;

    let resolved_home =
        boreal_home.to_string_lossy().into_owned();

    let current_home = dirs
        .get("home")
        .and_then(Value::as_str)
        .unwrap_or("");

    if current_home == resolved_home {
        return Ok(false);
    }

    dirs.insert(
        "home".to_string(),
        Value::String(resolved_home),
    );

    Ok(true)
}

/// Resolve every directory declared under BOREAL.DIRS.
///
/// The name of each directory becomes a pseudo-variable when
/// written in uppercase.
///
/// Example:
///
/// {
///   "home": "...",
///   "conf": "HOME/conf",
///   "data": "HOME/data",
///   "logs": "HOME/logs",
///   "sqlite": "DATA/sqlite"
/// }
///
/// resolves:
///
/// HOME   -> ~/.boreal
/// CONF   -> ~/.boreal/conf
/// DATA   -> ~/.boreal/data
/// LOGS   -> ~/.boreal/logs
/// SQLITE -> ~/.boreal/data/sqlite
fn resolve_all_directories(
    boreal: &Value,
    boreal_home: &Path,
) -> Result<BTreeMap<String, PathBuf>, Box<dyn Error>> {
    let dirs = boreal
        .get("BOREAL")
        .and_then(|value| value.get("DIRS"))
        .and_then(Value::as_object)
        .ok_or("Missing BOREAL.DIRS in boreal.json")?;

    /*
     * Collect all valid pseudo-variable names first.
     */
    let known_names: HashSet<String> = dirs
        .keys()
        .map(|name| name.to_uppercase())
        .collect();

    /*
     * HOME is special because it has already been determined
     * before boreal.json is read.
     */
    let mut resolved: BTreeMap<String, PathBuf> =
        BTreeMap::new();

    resolved.insert(
        "HOME".to_string(),
        boreal_home.to_path_buf(),
    );

    /*
     * Everything except home starts unresolved.
     */
    let mut unresolved: BTreeMap<String, String> =
        BTreeMap::new();

    for (name, value) in dirs {
        if name.eq_ignore_ascii_case("home") {
            continue;
        }

        let value = value
            .as_str()
            .ok_or_else(|| {
                format!(
                    "BOREAL.DIRS.{name} must be a string"
                )
            })?;

        unresolved.insert(
            name.to_uppercase(),
            value.to_string(),
        );
    }

    /*
     * Resolve directories iteratively.
     *
     * This allows:
     *
     * data   = HOME/data
     * sqlite = DATA/sqlite
     *
     * even if they do not appear in dependency order in JSON.
     */
    while !unresolved.is_empty() {
        let mut progress = false;

        let names: Vec<String> =
            unresolved.keys().cloned().collect();

        for name in names {
            let value = unresolved
                .get(&name)
                .expect("unresolved directory disappeared");

            match resolve_directory_value(
                value,
                &resolved,
                &known_names,
            )? {
                Some(path) => {
                    resolved.insert(
                        name.clone(),
                        path,
                    );

                    unresolved.remove(&name);

                    progress = true;
                }

                None => {
                    /*
                     * Dependency has not been resolved yet.
                     */
                }
            }
        }

        /*
         * No progress means we have either a circular
         * dependency or an unresolved reference.
         */
        if !progress {
            let unresolved_list = unresolved
                .iter()
                .map(|(name, value)| {
                    format!("{name}={value}")
                })
                .collect::<Vec<_>>()
                .join(", ");

            return Err(
                format!(
                    "Unable to resolve BOREAL directories: \
                     {unresolved_list}"
                )
                .into(),
            );
        }
    }

    Ok(resolved)
}

/// Resolve a single directory definition.
///
/// Returns:
///
/// Some(path)
///     The directory can be resolved.
///
/// None
///     The directory references another configured directory
///     that has not yet been resolved.
fn resolve_directory_value(
    value: &str,
    resolved: &BTreeMap<String, PathBuf>,
    known_names: &HashSet<String>,
) -> Result<Option<PathBuf>, Box<dyn Error>> {
    /*
     * Exact pseudo-variable:
     *
     * DATA
     */
    let upper_value =
        value.to_uppercase();

    if known_names.contains(&upper_value) {
        if let Some(base) =
            resolved.get(&upper_value)
        {
            return Ok(Some(base.clone()));
        }

        return Ok(None);
    }

    /*
     * Pseudo-variable followed by a path:
     *
     * DATA/sqlite
     * DATA\sqlite
     */
    if let Some((prefix, remainder)) =
        split_pseudo_path(value)
    {
        let pseudo =
            prefix.to_uppercase();

        /*
         * Only interpret the prefix as a pseudo-variable when
         * it names one of the configured directories.
         */
        if known_names.contains(&pseudo) {
            let Some(base) =
                resolved.get(&pseudo)
            else {
                return Ok(None);
            };

            let mut path =
                base.clone();

            /*
             * Accept either slash style in boreal.json.
             * PathBuf will generate the platform-native path.
             */
            for component in remainder
                .split(['/', '\\'])
                .filter(|part| !part.is_empty())
            {
                path.push(component);
            }

            return Ok(Some(path));
        }

        /*
         * An uppercase-looking prefix strongly suggests the
         * user intended a BOREAL pseudo-variable but misspelled
         * it.
         */
        if is_pseudo_name(prefix) {
            return Err(
                format!(
                    "Unknown BOREAL directory pseudo-variable \
                     '{prefix}' in path '{value}'"
                )
                .into(),
            );
        }
    }

    /*
     * No pseudo-variable. Treat the value as a literal path.
     */
    Ok(Some(PathBuf::from(value)))
}

/// Split:
///
/// DATA/sqlite
///
/// into:
///
/// ("DATA", "sqlite")
///
/// Also supports Windows-style separators:
///
/// DATA\sqlite
fn split_pseudo_path(
    value: &str,
) -> Option<(&str, &str)> {
    let slash =
        value.find('/');

    let backslash =
        value.find('\\');

    let index = match (slash, backslash) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }?;

    if index == 0 {
        return None;
    }

    Some((
        &value[..index],
        &value[index + 1..],
    ))
}

/// Determine whether a string looks like one of BOREAL's
/// uppercase pseudo-variable names.
///
/// Examples:
///
/// HOME
/// DATA
/// SQLITE
/// RCLONE_CACHE
fn is_pseudo_name(
    value: &str,
) -> bool {
    !value.is_empty()
        && value.chars().all(|c| {
            c.is_ascii_uppercase()
                || c.is_ascii_digit()
                || c == '_'
        })
}

/// Restrict secrets.json access on Unix systems.
///
/// Linux/macOS:
///     mode 0600
///
/// Windows:
///     uses the ACL inherited from %LOCALAPPDATA%.
// fn protect_secrets(
//     path: &Path,
// ) -> Result<(), Box<dyn Error>> {
//     #[cfg(unix)]
//     {
//         use std::os::unix::fs::PermissionsExt;

//         let permissions =
//             fs::Permissions::from_mode(0o600);

//         fs::set_permissions(
//             path,
//             permissions,
//         )?;
//     }

//     Ok(())
// }
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

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}