use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::PathBuf;

const APP_NAME: &str = "boreal";
const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    version: u32,
    web_host: String,
    web_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            web_host: "127.0.0.1".to_string(),
            web_port: 8765,
        }
    }
}

fn get_boreal_home() -> Result<PathBuf, Box<dyn Error>> {
    let data_dir = dirs::data_local_dir()
        .ok_or("Unable to determine local application data directory")?;

    Ok(data_dir.join(APP_NAME))
}

fn ensure_boreal_home() -> Result<PathBuf, Box<dyn Error>> {
    let boreal_home = get_boreal_home()?;

    if !boreal_home.exists() {
        fs::create_dir_all(&boreal_home)?;
        println!("Created BOREAL home: {}", boreal_home.display());
    }

    Ok(boreal_home)
}

fn ensure_config(boreal_home: &PathBuf) -> Result<Config, Box<dyn Error>> {
    let config_path = boreal_home.join(CONFIG_FILE);

    if config_path.exists() {
        let contents = fs::read_to_string(&config_path)?;
        let config: Config = serde_json::from_str(&contents)?;
        return Ok(config);
    }

    let config = Config::default();

    let json = serde_json::to_string_pretty(&config)?;

    fs::write(&config_path, json)?;

    println!("Created configuration: {}", config_path.display());

    Ok(config)
}

fn main() -> Result<(), Box<dyn Error>> {
    let boreal_home = ensure_boreal_home()?;
    let config = ensure_config(&boreal_home)?;

    println!("BOREAL home: {}", boreal_home.display());
    println!(
        "Web interface: http://{}:{}",
        config.web_host,
        config.web_port
    );

    Ok(())
}