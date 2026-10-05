//! Sequential patch planning for the controller pack.

use crate::manifest::{Manifest, PatchRelease};

pub enum Plan {
    UpToDate,
    /// Apply these patches in order.
    Patches(Vec<PatchRelease>),
    /// Chain is broken or nothing is installed: reinstall the baseline, then apply `Vec`.
    Reinstall(Vec<PatchRelease>),
}

/// Walk from `installed` to the latest version, one patch at a time.
pub fn plan(manifest: &Manifest, installed: Option<u32>) -> Plan {
    let Some(mut current) = installed else {
        return Plan::Reinstall(chain(manifest, manifest.baseline.version).unwrap_or_default());
    };
    let latest = manifest.latest_version();
    if current >= latest {
        return Plan::UpToDate;
    }
    match chain(manifest, current) {
        Some(c) => Plan::Patches(c),
        None => {
            current = manifest.baseline.version;
            Plan::Reinstall(chain(manifest, current).unwrap_or_default())
        }
    }
}

/// Returns the ordered patches from `start` to latest, or None if a link is missing.
fn chain(manifest: &Manifest, start: u32) -> Option<Vec<PatchRelease>> {
    let latest = manifest.latest_version();
    let mut current = start;
    let mut out = Vec::new();
    while current < latest {
        let next = manifest.patches.iter().find(|p| p.from == current)?;
        current = next.version;
        out.push(next.clone());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{EuroScopeInfo, PackRelease};

    fn patch(from: u32, version: u32) -> PatchRelease {
        PatchRelease { from, version, url: String::new(), sha256: String::new() }
    }

    fn manifest(patches: Vec<PatchRelease>) -> Manifest {
        Manifest {
            euroscope: EuroScopeInfo {
                minimum_version: String::new(),
                recommended_version: String::new(),
                download_url: String::new(),
            },
            vcredist_url: String::new(),
            baseline: PackRelease { version: 1, url: String::new(), sha256: String::new() },
            patches,
        }
    }

    #[test]
    fn chains_patches_in_order() {
        let m = manifest(vec![patch(3, 4), patch(1, 2), patch(2, 3)]);
        match plan(&m, Some(2)) {
            Plan::Patches(p) => assert_eq!(p.iter().map(|p| p.version).collect::<Vec<_>>(), [3, 4]),
            _ => panic!(),
        }
        assert!(matches!(plan(&m, Some(4)), Plan::UpToDate));
    }

    #[test]
    fn broken_chain_reinstalls() {
        let m = manifest(vec![patch(1, 2), patch(3, 4)]);
        assert!(matches!(plan(&m, Some(2)), Plan::Reinstall(_)));
    }
}
