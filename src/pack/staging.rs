//! Replace a directory with a prepared copy, restoring the original if the swap fails.

use std::path::{Path, PathBuf};

pub fn unique_sibling(path: &Path, purpose: &str) -> anyhow::Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("directory has no parent: {}", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("invalid directory: {}", path.display()))?
        .to_string_lossy();
    for attempt in 0..1000 {
        let candidate = parent.join(format!(
            ".{name}-{purpose}-{}-{attempt}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!(
        "could not find a temporary directory for {}",
        path.display()
    )
}

pub fn replace_dir(staging: &Path, dest: &Path) -> anyhow::Result<()> {
    replace_dir_with(staging, dest, |from, to| std::fs::rename(from, to))
}

fn replace_dir_with(
    staging: &Path,
    dest: &Path,
    mut rename: impl FnMut(&Path, &Path) -> std::io::Result<()>,
) -> anyhow::Result<()> {
    if !dest.exists() {
        return rename(staging, dest).map_err(Into::into);
    }

    let previous = unique_sibling(dest, "previous")?;
    rename(dest, &previous)?;
    if let Err(replace_error) = rename(staging, dest) {
        return match rename(&previous, dest) {
            Ok(()) => Err(replace_error.into()),
            Err(restore_error) => Err(anyhow::anyhow!(
                "could not replace {}: {replace_error}; could not restore the original from {}: {restore_error}",
                dest.display(),
                previous.display()
            )),
        };
    }
    std::fs::remove_dir_all(&previous)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_replacement_restores_the_original() {
        let base = std::env::temp_dir().join(format!("vuk-replace-{}", std::process::id()));
        let dest = base.join("UK");
        let staging = base.join("staging");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(dest.join("version.txt"), "old").unwrap();
        std::fs::write(staging.join("version.txt"), "new").unwrap();

        let result = replace_dir_with(&staging, &dest, |from, to| {
            if from == staging && to == dest {
                return Err(std::io::Error::other("simulated failure"));
            }
            std::fs::rename(from, to)
        });

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(dest.join("version.txt")).unwrap(),
            "old"
        );
        assert!(staging.join("version.txt").exists());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
