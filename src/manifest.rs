//! Describes what the installer can fetch: EuroScope/VC++ info plus the controller pack releases.

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// GitHub releases API for the controller pack. Tags look like `2026_10` or `2026_09a`.
pub const RELEASES_URL: &str =
    "https://api.github.com/repos/VATSIM-UK/uk-controller-pack/releases?per_page=100";

/// Pack release tag: `YYYY_MM` with an optional hotfix letter (`2026_09a`).
/// Ordering is chronological: `2026_09` < `2026_09a` < `2026_09b` < `2026_10`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackVersion {
    pub year: u16,
    pub month: u8,
    pub suffix: Option<char>,
}

impl FromStr for PackVersion {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        let (y, rest) = s
            .split_once('_')
            .ok_or_else(|| anyhow::anyhow!("bad pack version: {s}"))?;
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        let (m, suffix) = rest.split_at(digits);
        let mut chars = suffix.chars();
        let suffix = chars.next();
        if chars.next().is_some() || suffix.is_some_and(|c| !c.is_ascii_lowercase()) {
            anyhow::bail!("bad pack version: {s}");
        }
        Ok(PackVersion { year: y.parse()?, month: m.parse()?, suffix })
    }
}

impl fmt::Display for PackVersion {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}_{:02}", self.year, self.month)?;
        if let Some(c) = self.suffix {
            write!(f, "{c}")?;
        }
        Ok(())
    }
}

impl Serialize for PackVersion {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PackVersion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone)]
pub struct Manifest {
    pub euroscope: EuroScopeInfo,
    pub vcredist_url: String,
    /// All pack releases, sorted oldest to newest.
    pub releases: Vec<PackRelease>,
}

#[derive(Debug, Clone)]
pub struct EuroScopeInfo {
    /// Exact version required; newer versions are not supported.
    pub required_version: String,
    pub download_url: String,
}

/// One tagged release. Each has a full pack and a changes-only zip relative to the previous release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackRelease {
    pub version: PackVersion,
    pub full: Asset,
    pub changes_only: Asset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
}

impl Manifest {
    pub fn latest(&self) -> Option<&PackRelease> {
        self.releases.last()
    }

    /// TODO: fetch `RELEASES_URL`, map `uk_controller_pack_<tag>.zip` and
    /// `changes_only_<tag>.zip` assets (url + `digest`) into `PackRelease`s, skip drafts/prereleases.
    /// EuroScope/VC++ values need a source (hard-coded or a small JSON); see docs.
    pub fn fetch() -> anyhow::Result<Manifest> {
        anyhow::bail!("manifest fetching not implemented")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_orders_tags() {
        let v = |s: &str| s.parse::<PackVersion>().unwrap();
        assert!(v("2026_09") < v("2026_09a"));
        assert!(v("2026_09a") < v("2026_09b"));
        assert!(v("2026_09b") < v("2026_10"));
        assert!(v("2025_12") < v("2026_01"));
        assert_eq!(v("2026_09a").to_string(), "2026_09a");
        assert!("garbage".parse::<PackVersion>().is_err());
    }
}
