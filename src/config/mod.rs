//! Controller pack configuration: stored locally, applied by patching the pack files.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod apply;
mod display;
mod keybind;
mod profile;
mod text_files;

pub use apply::apply;
pub use keybind::{asel_from_vk, asel_name, pressed_vk};

pub const CONFIG_FILE: &str = "controller_pack_config.json";

pub const RATINGS: &[&str] = &[
    "OBS",
    "S1",
    "S2",
    "S3",
    "C1",
    "C2 (not used)",
    "C3",
    "I1",
    "I2 (not used)",
    "I3",
    "SUP",
    "ADM",
];

pub const COAST_OPTIONS: &[(&str, &str, &str)] = &[
    (
        "1",
        "Blue (default): suitable for NOVA based systems (most APP units)",
        "9076039",
    ),
    (
        "2",
        "Grey: suitable for NODE based systems (STC, LTC, MPC)",
        "5324604",
    ),
    ("3", "Yellow: high contrast", "32896"),
];

pub const LAND_OPTIONS: &[(&str, &str, &str)] = &[
    ("1", "Mid grey (default)", "3947580"),
    ("2", "Dark grey", "1777181"),
    ("3", "Light grey", "8158332"),
];

/// Embedded preview image for a coastline (`"coastline"`) or land (`"land"`) option.
pub fn preview_bytes(kind: &str, key: &str) -> Option<&'static [u8]> {
    Some(match (kind, key) {
        ("coastline", "1") => include_bytes!("../../data/coastline1.png"),
        ("coastline", "2") => include_bytes!("../../data/coastline2.png"),
        ("coastline", "3") => include_bytes!("../../data/coastline3.png"),
        ("land", "1") => include_bytes!("../../data/land1.png"),
        ("land", "2") => include_bytes!("../../data/land2.png"),
        ("land", "3") => include_bytes!("../../data/land3.png"),
        _ => return None,
    })
}

/// Field names and `y`/`n` flags match the original configurator's JSON so existing files load unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub name: String,
    pub initials: String,
    pub cid: String,
    pub rating: String,
    pub password: String,
    pub cpdlc: String,
    pub discord_presence: String,
    pub realistic_tags: String,
    pub realistic_conversion: String,
    pub coast_choice: String,
    pub land_choice: String,
    pub asel_key: String,
    /// `off`, `radar` (radar displays only) or `on` (SMRs too).
    pub rdf_mode: String,
    /// A numeric text size, or `default` to leave the pack as shipped.
    pub font_size: String,
    pub advanced_config: bool,
    pub north_up_smrs: bool,
}

pub fn font_size_value(value: &str) -> f32 {
    let size = value.parse::<f32>().unwrap_or(3.5).clamp(0.0, 15.0);
    (size * 2.0).round() / 2.0
}

pub const RDF_OPTIONS: &[(&str, &str)] = &[
    ("off", "Off (default, realistic)"),
    ("radar", "Radar displays only"),
    ("on", "Radar displays and SMRs"),
];

impl Default for Config {
    fn default() -> Self {
        Config {
            name: String::new(),
            initials: String::new(),
            cid: String::new(),
            rating: "0".into(),
            password: String::new(),
            cpdlc: String::new(),
            discord_presence: "n".into(),
            realistic_tags: "y".into(),
            realistic_conversion: "y".into(),
            coast_choice: "1".into(),
            land_choice: "1".into(),
            asel_key: String::new(),
            rdf_mode: "off".into(),
            font_size: "default".into(),
            advanced_config: false,
            north_up_smrs: false,
        }
    }
}

pub fn is_valid_cid(cid: &str) -> bool {
    cid.chars().all(|c| c.is_ascii_digit()) && (6..=7).contains(&cid.len())
}

impl Config {
    /// Returns the first validation problem, if any.
    pub fn validate(&self) -> Option<&'static str> {
        if self.name.trim().is_empty() {
            Some("Name is required.")
        } else if !(2..=3).contains(&self.initials.trim().chars().count()) {
            Some("Initials must be 2-3 letters.")
        } else if !is_valid_cid(&self.cid) {
            Some("CID must be a 6 or 7 digit number.")
        } else if self.password.is_empty() {
            Some("Password is required.")
        } else {
            None
        }
    }
}

/// Local storage: `%APPDATA%\vatsim-uk-installer\controller_pack_config.json`.
pub fn local_path() -> Option<PathBuf> {
    crate::common::store::path(CONFIG_FILE)
}

fn read(path: &Path) -> Option<Config> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Load the local config; if none exists (first run) fall back to the one the old configurator
/// left in the pack directory.
pub fn load(pack_dir: Option<&Path>) -> Config {
    local_path()
        .and_then(|p| read(&p))
        .or_else(|| pack_dir.and_then(|d| read(&d.join(CONFIG_FILE))))
        .unwrap_or_default()
}

/// Whether a config exists locally or in the pack.
pub fn exists(pack_dir: Option<&Path>) -> bool {
    local_path().is_some_and(|p| p.exists())
        || pack_dir.is_some_and(|d| d.join(CONFIG_FILE).exists())
}

pub fn save(cfg: &Config) -> anyhow::Result<()> {
    crate::common::store::save_json(CONFIG_FILE, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cid_validation() {
        assert!(is_valid_cid("1234567"));
        assert!(!is_valid_cid("12345"));
        assert!(!is_valid_cid("12a456"));
    }

    #[test]
    fn old_json_loads() {
        let c: Config =
            serde_json::from_str(r#"{"name":"A","cid":"1234567","coast_choice":"2"}"#).unwrap();
        assert_eq!(c.coast_choice, "2");
        assert_eq!(c.land_choice, "1");
    }

    #[test]
    fn font_size_value_clamps_and_rounds_to_half_steps() {
        assert_eq!(font_size_value("6.5"), 6.5);
        assert_eq!(font_size_value("6.3"), 6.5);
        assert_eq!(font_size_value("-1"), 0.0);
        assert_eq!(font_size_value("16"), 15.0);
        assert_eq!(font_size_value("small"), 3.5);
    }
}
