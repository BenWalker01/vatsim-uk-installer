//! Controller pack install and patching.

use crate::manifest::{PackRelease, PatchRelease};
use std::path::Path;

/// TODO: download, verify sha256, extract the full baseline into `dest`.
pub fn install_baseline(_release: &PackRelease, _dest: &Path) -> anyhow::Result<()> {
    anyhow::bail!("controller pack install not implemented")
}

/// TODO: download, verify sha256, overlay changed files onto `dest`
/// (and honour a deletions list in the patch).
pub fn apply_patch(_patch: &PatchRelease, _dest: &Path) -> anyhow::Result<()> {
    anyhow::bail!("controller pack patching not implemented")
}
