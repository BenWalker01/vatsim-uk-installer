//! Persistent installer state (installed pack version, EuroScope path).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct State {
    pub pack_version: Option<crate::manifest::PackVersion>,
    pub pack_dir: Option<PathBuf>,
    pub euroscope_path: Option<PathBuf>,
    #[serde(default)]
    pub theme: ThemePreference,
    /// How many backups to keep; `None` means the default.
    #[serde(default)]
    pub backups_to_keep: Option<usize>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemePreference {
    #[default]
    Auto,
    Light,
    Dark,
}

impl ThemePreference {
    pub fn egui(self) -> eframe::egui::ThemePreference {
        match self {
            Self::Auto => eframe::egui::ThemePreference::System,
            Self::Light => eframe::egui::ThemePreference::Light,
            Self::Dark => eframe::egui::ThemePreference::Dark,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{State, ThemePreference};

    #[test]
    fn theme_defaults_to_auto_for_existing_state_files() {
        let state: State = serde_json::from_str("{}").unwrap();
        assert_eq!(state.theme, ThemePreference::Auto);
    }

    #[test]
    fn theme_preference_round_trips() {
        for theme in [
            ThemePreference::Auto,
            ThemePreference::Light,
            ThemePreference::Dark,
        ] {
            let json = serde_json::to_string(&theme).unwrap();
            assert_eq!(
                serde_json::from_str::<ThemePreference>(&json).unwrap(),
                theme
            );
        }
    }
}
