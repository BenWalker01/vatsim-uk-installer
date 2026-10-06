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
    std::fs::read_to_string(dir.join("version.txt")).ok()?.trim().parse().ok()
}


/// Download and extract the full pack for `release` into `dest`.
pub fn install_full(release: &PackRelease, dest: &Path, shared: &Shared) -> anyhow::Result<()> {
    download_and_extract(&release.full, dest, shared, &format!("Installing controller pack {}", release.version))
}

/// Download and overlay the changes-only zip for `release` onto `dest`.
/// The caller saves `State.pack_version` after each successful patch.
pub fn apply_changes(release: &PackRelease, dest: &Path, shared: &Shared) -> anyhow::Result<()> {
    download_and_extract(&release.changes_only, dest, shared, &format!("Applying update {}", release.version))
}

fn download_and_extract(asset: &Asset, dest: &Path, shared: &Shared, message: &str) -> anyhow::Result<()> {
    download::set_message(shared, message);
    std::fs::create_dir_all(dest)?;
    let tmp = std::env::temp_dir().join(format!("vatsim-uk-installer-{}.zip", std::process::id()));
    let result = download::download(&asset.url, &asset.sha256, &tmp, shared).and_then(|_| extract(&tmp, dest));
    let _ = std::fs::remove_file(&tmp);
    result
}

/// Download the full pack for `release` and return its pristine ASR files (`Data/ASR/**.asr`),
/// keyed by path relative to `Data/ASR`. Nothing is written to the pack.
pub fn fetch_pristine_asrs(release: &PackRelease, shared: &Shared) -> anyhow::Result<crate::layout::Baseline> {
    download::set_message(shared, format!("Downloading pack {} for comparison", release.version));
    let tmp = std::env::temp_dir().join(format!("vatsim-uk-installer-ref-{}.zip", std::process::id()));
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
        let Some(full) = entry.enclosed_name() else { continue };
        let Ok(rel) = full.strip_prefix(Path::new("UK").join("Data").join("ASR")) else { continue };
        if entry.is_dir() || !rel.extension().is_some_and(|x| x.eq_ignore_ascii_case("asr")) {
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
        let Ok(rel) = full.strip_prefix("UK") else { continue };
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
        apply_changes(m.latest().unwrap(), &dest, &Shared::default()).unwrap();
        let top: Vec<_> = std::fs::read_dir(&dest).unwrap().flatten().map(|e| e.file_name()).collect();
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
        w.start_file("UK/a/b.txt", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b"new").unwrap();
        w.start_file("README.pdf", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b"x").unwrap();
        w.finish().unwrap();

        let dest = dir.join("out");
        std::fs::create_dir_all(dest.join("a")).unwrap();
        std::fs::write(dest.join("a").join("b.txt"), "old").unwrap();
        extract(&zip_path, &dest).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("a").join("b.txt")).unwrap(), "new");
        assert!(!dest.join("README.pdf").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
