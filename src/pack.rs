//! Controller pack install and patching.

use crate::{
    download::{self, Shared},
    manifest::{Asset, PackRelease, PackVersion},
};
use std::path::{Path, PathBuf};

/// Default pack location: `%APPDATA%\EuroScope\UK`.
pub fn default_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("EuroScope").join("UK"))
}

/// Version recorded in `<dir>\version.txt` (the source of truth), if present and a valid tag.
pub fn installed_version(dir: &Path) -> Option<PackVersion> {
    std::fs::read_to_string(dir.join("version.txt"))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Download and extract the full pack for `release` into `dest`.
pub fn install_full(release: &PackRelease, dest: &Path, shared: &Shared) -> anyhow::Result<()> {
    update_staged(dest, false, |staging| {
        download_and_extract(
            &release.full,
            staging,
            shared,
            &format!("Installing controller pack {}", release.version),
        )
    })
}

/// Apply every changes-only archive to a staged copy, replacing `dest` only when all succeed.
pub fn apply_changes_batch(
    releases: &[PackRelease],
    dest: &Path,
    shared: &Shared,
) -> anyhow::Result<()> {
    update_staged(dest, true, |staging| {
        for release in releases {
            let version_file = staging.join("version.txt");
            if version_file.exists() {
                std::fs::remove_file(&version_file)?;
            }
            download_and_extract(
                &release.changes_only,
                staging,
                shared,
                &format!("Applying update {}", release.version),
            )?;
            ensure_version_file(&version_file, release.version)?;
        }
        Ok(())
    })
}

fn ensure_version_file(path: &Path, version: PackVersion) -> anyhow::Result<()> {
    if !path.exists() {
        std::fs::write(path, version.to_string())?;
    }
    Ok(())
}

fn update_staged(
    dest: &Path,
    copy_existing: bool,
    update: impl FnOnce(&Path) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| anyhow::anyhow!("pack directory has no parent: {}", dest.display()))?;
    std::fs::create_dir_all(parent)?;
    if copy_existing && !dest.is_dir() {
        anyhow::bail!(
            "installed pack directory does not exist: {}",
            dest.display()
        );
    }

    let staging = unique_sibling(dest, "updating")?;
    std::fs::create_dir(&staging)?;
    let result = (|| {
        if copy_existing {
            copy_dir_contents(dest, &staging)?;
        }
        update(&staging)?;
        promote(&staging, dest)
    })();
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

fn copy_dir_contents(source: &Path, dest: &Path) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            std::fs::create_dir(&to)?;
            copy_dir_contents(&from, &to)?;
        } else if kind.is_file() {
            std::fs::copy(&from, &to)?;
        } else {
            anyhow::bail!("unsupported file type in pack: {}", from.display());
        }
    }
    Ok(())
}

fn unique_sibling(path: &Path, purpose: &str) -> anyhow::Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("pack directory has no parent: {}", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("invalid pack directory: {}", path.display()))?
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

fn promote(staging: &Path, dest: &Path) -> anyhow::Result<()> {
    if !dest.exists() {
        return std::fs::rename(staging, dest).map_err(Into::into);
    }

    let previous = unique_sibling(dest, "previous")?;
    std::fs::rename(dest, &previous)?;
    if let Err(promote_error) = std::fs::rename(staging, dest) {
        return match std::fs::rename(&previous, dest) {
            Ok(()) => Err(promote_error.into()),
            Err(restore_error) => Err(anyhow::anyhow!(
                "could not install updated pack: {promote_error}; could not restore previous pack from {}: {restore_error}",
                previous.display()
            )),
        };
    }
    std::fs::remove_dir_all(&previous)?;
    Ok(())
}

fn download_and_extract(
    asset: &Asset,
    dest: &Path,
    shared: &Shared,
    message: &str,
) -> anyhow::Result<()> {
    download::set_message(shared, message);
    std::fs::create_dir_all(dest)?;
    let tmp = std::env::temp_dir().join(format!("vatsim-uk-installer-{}.zip", std::process::id()));
    let result = download::download(&asset.url, &asset.sha256, &tmp, shared)
        .and_then(|_| extract(&tmp, dest));
    let _ = std::fs::remove_file(&tmp);
    result
}

/// Download the full pack for `release` and return its pristine ASR files (`Data/ASR/**.asr`),
/// keyed by path relative to `Data/ASR`. Nothing is written to the pack.
pub fn fetch_pristine_asrs(
    release: &PackRelease,
    shared: &Shared,
) -> anyhow::Result<crate::layout::Baseline> {
    download::set_message(
        shared,
        format!("Downloading pack {} for comparison", release.version),
    );
    let tmp = std::env::temp_dir().join(format!(
        "vatsim-uk-installer-ref-{}.zip",
        std::process::id()
    ));
    let result = download::download(&release.full.url, &release.full.sha256, &tmp, shared)
        .and_then(|_| read_asrs_from_zip(&tmp));
    let _ = std::fs::remove_file(&tmp);
    result
}

fn read_asrs_from_zip(zip_path: &Path) -> anyhow::Result<crate::layout::Baseline> {
    use std::io::Read;
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    let mut out = crate::layout::Baseline::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(full) = entry.enclosed_name() else {
            continue;
        };
        let Ok(rel) = full.strip_prefix(Path::new("UK").join("Data").join("ASR")) else {
            continue;
        };
        if entry.is_dir()
            || !rel
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("asr"))
        {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        let key = rel.to_string_lossy().replace('\\', "/");
        out.insert(key, crate::layout::norm(&String::from_utf8_lossy(&bytes)));
    }
    Ok(out)
}

/// Extract the `UK/` folder of `zip_path` into `dest` (the zip's top-level `UK` is stripped; other
/// top-level files such as README.pdf are skipped), overwriting existing files and rejecting paths that escape `dest`.
fn extract(zip_path: &Path, dest: &Path) -> anyhow::Result<()> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(full) = entry.enclosed_name() else {
            anyhow::bail!("unsafe path in zip: {}", entry.name());
        };
        let Ok(rel) = full.strip_prefix("UK") else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::io::copy(&mut entry, &mut std::fs::File::create(&out)?)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    #[ignore = "downloads ~16MB from GitHub"]
    fn downloads_live_patch() {
        let m = crate::manifest::Manifest::fetch().unwrap();
        let dest = std::env::temp_dir().join("vuk-live-patch");
        let _ = std::fs::remove_dir_all(&dest);
        apply_changes_batch(
            std::slice::from_ref(m.latest().unwrap()),
            &dest,
            &Shared::default(),
        )
        .unwrap();
        let top: Vec<_> = std::fs::read_dir(&dest)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        println!("top-level: {top:?}");
        assert!(!dest.join("UK").exists());
        assert!(top.len() > 0);
        std::fs::remove_dir_all(&dest).unwrap();
    }

    #[test]
    fn reads_version_file() {
        let dir = std::env::temp_dir().join(format!("vuk-ver-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(installed_version(&dir), None);
        std::fs::write(dir.join("version.txt"), "2026_09a\r\n").unwrap();
        assert_eq!(installed_version(&dir).unwrap().to_string(), "2026_09a");
        std::fs::write(dir.join("version.txt"), "2025_07x1").unwrap();
        assert_eq!(installed_version(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extracts_and_overwrites() {
        let dir = std::env::temp_dir().join(format!("vuk-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("t.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        w.start_file("UK/a/b.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(b"new").unwrap();
        w.start_file("README.pdf", zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(b"x").unwrap();
        w.finish().unwrap();

        let dest = dir.join("out");
        std::fs::create_dir_all(dest.join("a")).unwrap();
        std::fs::write(dest.join("a").join("b.txt"), "old").unwrap();
        extract(&zip_path, &dest).unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("a").join("b.txt")).unwrap(),
            "new"
        );
        assert!(!dest.join("README.pdf").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn failed_staged_update_preserves_the_live_pack() {
        let dir = std::env::temp_dir().join(format!("vuk-atomic-fail-{}", std::process::id()));
        let pack = dir.join("UK");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(pack.join("existing.txt"), "original").unwrap();

        let result = update_staged(&pack, true, |staging| {
            std::fs::write(staging.join("existing.txt"), "partial")?;
            anyhow::bail!("simulated patch failure");
        });

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(pack.join("existing.txt")).unwrap(),
            "original"
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn successful_staged_update_replaces_the_live_pack() {
        let dir = std::env::temp_dir().join(format!("vuk-atomic-ok-{}", std::process::id()));
        let pack = dir.join("UK");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(pack.join("existing.txt"), "original").unwrap();

        update_staged(&pack, true, |staging| {
            std::fs::write(staging.join("existing.txt"), "updated")?;
            std::fs::write(staging.join("new.txt"), "new")?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(pack.join("existing.txt")).unwrap(),
            "updated"
        );
        assert_eq!(
            std::fs::read_to_string(pack.join("new.txt")).unwrap(),
            "new"
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn full_reinstall_discards_files_not_in_the_new_pack() {
        let dir = std::env::temp_dir().join(format!("vuk-full-reinstall-{}", std::process::id()));
        let pack = dir.join("UK");
        std::fs::create_dir_all(pack.join("obsolete")).unwrap();
        std::fs::write(pack.join("obsolete.txt"), "old").unwrap();
        std::fs::write(pack.join("obsolete").join("old.txt"), "old").unwrap();

        update_staged(&pack, false, |staging| {
            std::fs::write(staging.join("version.txt"), "2026_10")?;
            Ok(())
        })
        .unwrap();

        assert!(pack.join("version.txt").is_file());
        assert!(!pack.join("obsolete.txt").exists());
        assert!(!pack.join("obsolete").exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_version_file_is_written_without_overwriting_archive_marker() {
        let dir = std::env::temp_dir().join(format!("vuk-version-fallback-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let version_file = dir.join("version.txt");

        ensure_version_file(&version_file, "2026_10".parse().unwrap()).unwrap();
        assert_eq!(std::fs::read_to_string(&version_file).unwrap(), "2026_10");
        std::fs::write(&version_file, "2026_10a").unwrap();
        ensure_version_file(&version_file, "2026_10".parse().unwrap()).unwrap();
        assert_eq!(std::fs::read_to_string(&version_file).unwrap(), "2026_10a");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
