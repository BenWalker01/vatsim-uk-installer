//! Persistent installer state (installed pack version, EuroScope path).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct State {
    pub pack_version: Option<u32>,
    pub pack_dir: Option<PathBuf>,
    pub euroscope_path: Option<PathBuf>,
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("vatsim-uk-installer").join("state.json"))
}

impl State {
    pub fn load() -> State {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let p = path().ok_or_else(|| anyhow::anyhow!("no config dir"))?;
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(p, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
