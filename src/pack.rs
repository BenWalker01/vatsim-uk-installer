//! Controller pack install and patching.

use crate::manifest::PackRelease;
use std::path::Path;

/// TODO: download `release.full`, verify sha256, extract into `dest`.
pub fn install_full(_release: &PackRelease, _dest: &Path) -> anyhow::Result<()> {
    anyhow::bail!("controller pack install not implemented")
}

/// TODO: download `release.changes_only`, verify sha256, overlay onto `dest`.
/// The caller saves `State.pack_version` after each successful patch.
pub fn apply_changes(_release: &PackRelease, _dest: &Path) -> anyhow::Result<()> {
    anyhow::bail!("controller pack patching not implemented")
}
