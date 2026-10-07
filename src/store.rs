//! JSON files stored in `%APPDATA%\vatsim-uk-installer`.

use serde::{Serialize, de::DeserializeOwned};
use std::path::PathBuf;

pub fn dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("vatsim-uk-installer"))
}

pub fn path(file: &str) -> Option<PathBuf> {
    dir().map(|d| d.join(file))
}

pub fn load_json<T: DeserializeOwned>(file: &str) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(path(file)?).ok()?).ok()
}

pub fn save_json<T: Serialize>(file: &str, value: &T) -> anyhow::Result<()> {
    let path = path(file).ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}
