use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// Runtime configuration for the BOREAL WebUI.
pub struct WebAppConfig {
    pub listen: String,
    pub port: u16,
    pub open_browser: bool,
}

/// Load a JSON file.
pub fn load_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    let contents = fs::read_to_string(path)?;

    let value = serde_json::from_str(&contents)?;

    Ok(value)
}

/// Save JSON in human-readable form.
pub fn save_json(path: &Path, value: &Value) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_string_pretty(value)?;

    fs::write(path, format!("{json}\n"))?;

    Ok(())
}

/// Read BOREAL.WEBAPP configuration.
pub fn get_webapp_config(boreal: &Value) -> Result<WebAppConfig, Box<dyn Error>> {
    let webapp = boreal
        .get("BOREAL")
        .and_then(|value| value.get("WEBAPP"))
        .ok_or("Missing BOREAL.WEBAPP in boreal.json")?;

    let listen = webapp
        .get("listen")
        .and_then(Value::as_str)
        .ok_or("Missing or invalid BOREAL.WEBAPP.listen")?
        .to_string();

    let port = webapp
        .get("port")
        .and_then(Value::as_u64)
        .ok_or("Missing or invalid BOREAL.WEBAPP.port")?;

    if port > u16::MAX as u64 {
        return Err(format!("Invalid BOREAL.WEBAPP.port: {port}").into());
    }

    let open_browser = webapp
        .get("open_browser")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    Ok(WebAppConfig {
        listen,
        port: port as u16,
        open_browser,
    })
}

/// Set BOREAL.DIRS.home to the platform-resolved
/// BOREAL_HOME.
///
/// Returns true if boreal.json needs to be rewritten.
pub fn set_boreal_home(boreal: &mut Value, boreal_home: &Path) -> Result<bool, Box<dyn Error>> {
    let dirs = boreal
        .get_mut("BOREAL")
        .and_then(|value| value.get_mut("DIRS"))
        .and_then(Value::as_object_mut)
        .ok_or("Missing BOREAL.DIRS in boreal.json")?;

    let resolved_home = boreal_home.to_string_lossy().into_owned();

    let current_home = dirs.get("home").and_then(Value::as_str).unwrap_or("");

    if current_home == resolved_home {
        return Ok(false);
    }

    dirs.insert("home".to_string(), Value::String(resolved_home));

    Ok(true)
}

/// Resolve every directory declared under
/// BOREAL.DIRS.
///
/// The name of each directory automatically becomes
/// an uppercase pseudo-variable.
///
/// Example:
///
/// {
///   "home": "...",
///   "conf": "HOME/conf",
///   "data": "HOME/data",
///   "sqlite": "DATA/sqlite",
///   "logs": "HOME/logs"
/// }
///
/// resolves:
///
/// HOME   -> ~/.boreal
/// CONF   -> ~/.boreal/conf
/// DATA   -> ~/.boreal/data
/// SQLITE -> ~/.boreal/data/sqlite
/// LOGS   -> ~/.boreal/logs
pub fn resolve_all_directories(
    boreal: &Value,
    boreal_home: &Path,
) -> Result<BTreeMap<String, PathBuf>, Box<dyn Error>> {
    let dirs = boreal
        .get("BOREAL")
        .and_then(|value| value.get("DIRS"))
        .and_then(Value::as_object)
        .ok_or("Missing BOREAL.DIRS in boreal.json")?;

    /*
     * Every configured directory name is also a
     * possible uppercase pseudo-variable.
     */
    let known_names: HashSet<String> = dirs.keys().map(|name| name.to_uppercase()).collect();

    /*
     * HOME is special. It is already known before
     * boreal.json is processed.
     */
    let mut resolved: BTreeMap<String, PathBuf> = BTreeMap::new();

    resolved.insert("HOME".to_string(), boreal_home.to_path_buf());

    /*
     * Store everything except HOME as unresolved
     * initially.
     */
    let mut unresolved: BTreeMap<String, String> = BTreeMap::new();

    for (name, value) in dirs {
        if name.eq_ignore_ascii_case("home") {
            continue;
        }

        let value = value
            .as_str()
            .ok_or_else(|| format!("BOREAL.DIRS.{name} must be a string"))?;

        unresolved.insert(name.to_uppercase(), value.to_string());
    }

    /*
     * Resolve repeatedly until every directory has
     * been resolved.
     *
     * This makes configuration order irrelevant.
     */
    while !unresolved.is_empty() {
        let mut progress = false;

        let names: Vec<String> = unresolved.keys().cloned().collect();

        for name in names {
            let value = unresolved
                .get(&name)
                .expect("unresolved directory disappeared");

            match resolve_directory_value(value, &resolved, &known_names)? {
                Some(path) => {
                    resolved.insert(name.clone(), path);

                    unresolved.remove(&name);

                    progress = true;
                }

                None => {
                    /*
                     * This entry depends on another
                     * directory which has not yet
                     * been resolved.
                     */
                }
            }
        }

        /*
         * No progress means a circular dependency
         * or unresolved pseudo-variable exists.
         */
        if !progress {
            let unresolved_list = unresolved
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join(", ");

            return Err(format!(
                "Unable to resolve BOREAL directories: \
                     {unresolved_list}"
            )
            .into());
        }
    }

    Ok(resolved)
}

/// Resolve one directory definition.
///
/// Some(path)
///     The path is fully resolvable.
///
/// None
///     The path depends on another configured
///     directory which is not resolved yet.
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
    let upper_value = value.to_uppercase();

    if known_names.contains(&upper_value) {
        if let Some(base) = resolved.get(&upper_value) {
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
    if let Some((prefix, remainder)) = split_pseudo_path(value) {
        let pseudo = prefix.to_uppercase();

        if known_names.contains(&pseudo) {
            let Some(base) = resolved.get(&pseudo) else {
                return Ok(None);
            };

            let mut path = base.clone();

            /*
             * Accept either slash convention in
             * boreal.json.
             */
            for component in remainder.split(['/', '\\']).filter(|part| !part.is_empty()) {
                path.push(component);
            }

            return Ok(Some(path));
        }

        /*
         * An uppercase prefix probably indicates an
         * intended BOREAL pseudo-variable.
         */
        if is_pseudo_name(prefix) {
            return Err(format!(
                "Unknown BOREAL directory pseudo-variable \
                     '{prefix}' in path '{value}'"
            )
            .into());
        }
    }

    /*
     * No pseudo-variable detected.
     * Treat as a literal filesystem path.
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
/// Windows-style separators are also accepted.
fn split_pseudo_path(value: &str) -> Option<(&str, &str)> {
    let slash = value.find('/');

    let backslash = value.find('\\');

    let index = match (slash, backslash) {
        (Some(a), Some(b)) => Some(a.min(b)),

        (Some(a), None) => Some(a),

        (None, Some(b)) => Some(b),

        (None, None) => None,
    }?;

    if index == 0 {
        return None;
    }

    Some((&value[..index], &value[index + 1..]))
}

/// Determine whether a string looks like one of
/// BOREAL's uppercase pseudo-variable names.
fn is_pseudo_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}
