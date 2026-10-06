//! Sequential update planning for the controller pack.

use crate::manifest::{Manifest, PackRelease, PackVersion};

pub enum Plan {
    UpToDate,
    /// Apply each release's changes-only zip, oldest first.
    Patches(Vec<PackRelease>),
    /// Nothing installed, or the installed version is unknown: install this full pack.
    Reinstall(PackRelease),
}

pub fn plan(manifest: &Manifest, installed: Option<PackVersion>) -> Option<Plan> {
    let latest = manifest.latest()?;
    let Some(current) = installed else {
        return Some(Plan::Reinstall(latest.clone()));
    };
    if current == latest.version {
        return Some(Plan::UpToDate);
    }
    // Changes-only zips are only valid on top of a known release.
    if !manifest.releases.iter().any(|r| r.version == current) {
        return Some(Plan::Reinstall(latest.clone()));
    }
    let pending: Vec<_> = manifest
        .releases
        .iter()
        .filter(|r| r.version > current)
        .cloned()
        .collect();
    Some(if pending.is_empty() {
        Plan::UpToDate
    } else {
        Plan::Patches(pending)
    })
}

/// Run a plan, saving the installed version after every step so an interruption resumes cleanly.
pub fn execute(
    plan: Plan,
    dir: &std::path::Path,
    shared: &crate::download::Shared,
) -> anyhow::Result<()> {
    let mut state = crate::state::State::load();
    state.pack_dir = Some(dir.to_path_buf());
    match plan {
        Plan::UpToDate => {}
        Plan::Reinstall(r) => {
            crate::pack::install_full(&r, dir, shared)?;
            state.pack_version = Some(r.version);
            state.save()?;
        }
        Plan::Patches(releases) => {
            for r in releases {
                crate::pack::apply_changes(&r, dir, shared)?;
                state.pack_version = Some(r.version);
                state.save()?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Asset, EuroScopeInfo};

    fn rel(tag: &str) -> PackRelease {
        let a = Asset {
            url: String::new(),
            sha256: String::new(),
        };
        PackRelease {
            version: tag.parse().unwrap(),
            full: a.clone(),
            changes_only: a,
        }
    }

    fn manifest(tags: &[&str]) -> Manifest {
        Manifest {
            euroscope: EuroScopeInfo {
                required_version: String::new(),
                download_url: String::new(),
            },
            vcredist_url: String::new(),
            releases: tags.iter().map(|t| rel(t)).collect(),
        }
    }

    fn v(s: &str) -> Option<PackVersion> {
        Some(s.parse().unwrap())
    }

    #[test]
    fn applies_each_release_in_order() {
        let m = manifest(&["2026_08", "2026_09", "2026_09a", "2026_10"]);
        match plan(&m, v("2026_09")) {
            Some(Plan::Patches(p)) => {
                let tags: Vec<_> = p.iter().map(|r| r.version.to_string()).collect();
                assert_eq!(tags, ["2026_09a", "2026_10"]);
            }
            _ => panic!(),
        }
        assert!(matches!(plan(&m, v("2026_10")), Some(Plan::UpToDate)));
    }

    #[test]
    fn unknown_or_missing_version_reinstalls_latest() {
        let m = manifest(&["2026_09", "2026_10"]);
        assert!(matches!(plan(&m, None), Some(Plan::Reinstall(_))));
        assert!(matches!(plan(&m, v("2020_01")), Some(Plan::Reinstall(_))));
    }
}
