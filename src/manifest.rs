//! Remote manifest describing what the installer can fetch.

use serde::{Deserialize, Serialize};

pub const MANIFEST_URL: &str = "https://example.invalid/vatsim-uk/manifest.json"; // TODO: real URL

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub euroscope: EuroScopeInfo,
    pub vcredist_url: String,
    /// Full controller pack, used for first installs.
    pub baseline: PackRelease,
    /// Changes-only releases, in any order.
    pub patches: Vec<PatchRelease>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EuroScopeInfo {
    pub minimum_version: String,
    pub recommended_version: String,
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRelease {
    pub version: u32,
    pub url: String,
    pub sha256: String,
}

/// A changes-only release that upgrades `from` -> `version`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchRelease {
    pub from: u32,
    pub version: u32,
    pub url: String,
    pub sha256: String,
}

impl Manifest {
    pub fn latest_version(&self) -> u32 {
        self.patches
            .iter()
            .map(|p| p.version)
            .max()
            .unwrap_or(0)
            .max(self.baseline.version)
    }

    /// TODO: fetch over HTTP and deserialize.
    pub fn fetch() -> anyhow::Result<Manifest> {
        anyhow::bail!("manifest fetching not implemented")
    }
}
