//! Zipped pack backups stored under `%APPDATA%\vatsim-uk-installer\backups`.
//! File names are `UK_<pack tag>_<unix seconds>.zip`.

use crate::download::{self, Shared};
use std::path::{Path, PathBuf};

pub const DEFAULT_KEEP: usize = 5;

#[derive(Debug, Clone)]
pub struct Backup {
    pub path: PathBuf,
    pub tag: String,
    pub secs: u64,
    pub size: u64,
}

impl Backup {
    pub fn name(&self) -> String {
        format!("UK_{}", self.tag)
    }

    pub fn date(&self) -> String {
        format_utc(self.secs)
    }
}

pub fn dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("vatsim-uk-installer").join("backups"))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `YYYY-MM-DD HH:MM UTC` from unix seconds (proleptic Gregorian civil-from-days).
fn format_utc(secs: u64) -> String {
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60
    )
}

fn parse_name(file: &str) -> Option<(String, u64)> {
    let stem = file.strip_prefix("UK_")?.strip_suffix(".zip")?;
    let (tag, secs) = stem.rsplit_once('_')?;
    Some((tag.to_string(), secs.parse().ok()?))
}

/// All backups, newest first.
pub fn list() -> Vec<Backup> {
    let Some(rd) = dir().and_then(|d| std::fs::read_dir(d).ok()) else {
        return Vec::new();
    };
    let mut out: Vec<Backup> = rd
        .flatten()
        .filter_map(|e| {
            let (tag, secs) = parse_name(&e.file_name().to_string_lossy())?;
            Some(Backup {
                path: e.path(),
                tag,
                secs,
                size: e.metadata().ok()?.len(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.secs.cmp(&a.secs));
    out
}

/// Zip the pack at `pack_dir` into the backup folder, then prune to `keep` backups.
pub fn create(
    pack_dir: &Path,
    tag: Option<&str>,
    keep: usize,
    shared: &Shared,
) -> anyhow::Result<PathBuf> {
    download::set_message(shared, "Backing up your existing pack");
    let root = dir().ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    std::fs::create_dir_all(&root)?;
    let dest = root.join(format!(
        "UK_{}_{}.zip",
        tag.unwrap_or("unknown"),
        now_secs()
    ));
    if let Err(e) = write_zip(pack_dir, &dest) {
        let _ = std::fs::remove_file(&dest);
        return Err(e);
    }
    prune(keep)?;
    Ok(dest)
}

fn write_zip(src: &Path, dest: &Path) -> anyhow::Result<()> {
    let mut zip = zip::ZipWriter::new(std::io::BufWriter::with_capacity(
        1 << 20,
        std::fs::File::create(dest)?,
    ));
    // Fastest deflate: much quicker than the default level for a modest size cost.
    let opts = zip::write::SimpleFileOptions::default().compression_level(Some(1));
    add_dir(&mut zip, src, src, opts)?;
    zip.finish()?;
    Ok(())
}

fn add_dir(
    zip: &mut zip::ZipWriter<std::io::BufWriter<std::fs::File>>,
    dir: &Path,
    root: &Path,
    opts: zip::write::SimpleFileOptions,
) -> anyhow::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        let name = p.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
        if p.is_dir() {
            zip.add_directory(format!("{name}/"), opts)?;
            add_dir(zip, &p, root, opts)?;
        } else {
            zip.start_file(name, opts)?;
            std::io::copy(&mut std::io::BufReader::new(std::fs::File::open(&p)?), zip)?;
        }
    }
    Ok(())
}

/// Delete the oldest backups beyond `keep` (at least one is always kept).
pub fn prune(keep: usize) -> anyhow::Result<()> {
    for b in list().into_iter().skip(keep.max(1)) {
        std::fs::remove_file(b.path)?;
    }
    Ok(())
}

pub fn delete(b: &Backup) -> anyhow::Result<()> {
    std::fs::remove_file(&b.path)?;
    Ok(())
}

/// File paths inside the backup (directories omitted).
pub fn contents(b: &Backup) -> anyhow::Result<Vec<String>> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&b.path)?)?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let e = archive.by_index(i)?;
        if !e.is_dir() {
            out.push(e.name().to_string());
        }
    }
    out.sort();
    Ok(out)
}

/// Replace the pack at `pack_dir` with the backup. Extracts beside it first so a failed
/// restore leaves the current pack untouched.
pub fn restore(b: &Backup, pack_dir: &Path, shared: &Shared) -> anyhow::Result<()> {
    download::set_message(shared, format!("Restoring {}", b.name()));
    let staging = pack_dir.with_file_name("UK_restoring");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    if let Err(e) = extract(&b.path, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    if pack_dir.exists() {
        std::fs::remove_dir_all(pack_dir)?;
    }
    std::fs::rename(&staging, pack_dir)?;
    Ok(())
}

fn extract(zip_path: &Path, dest: &Path) -> anyhow::Result<()> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            anyhow::bail!("unsafe path in backup: {}", entry.name());
        };
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

    #[test]
    fn parses_names_and_dates() {
        assert_eq!(
            parse_name("UK_2026_09a_1760000000.zip"),
            Some(("2026_09a".into(), 1760000000))
        );
        assert_eq!(parse_name("other.zip"), None);
        assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_utc(1_760_000_000), "2025-10-09 08:53 UTC");
    }

    #[test]
    fn zip_round_trip() {
        let base = std::env::temp_dir().join(format!("vuk-bk-{}", std::process::id()));
        let src = base.join("UK");
        std::fs::create_dir_all(src.join("Data")).unwrap();
        std::fs::write(src.join("Data").join("a.txt"), "hi").unwrap();
        let zip = base.join("UK_t_1.zip");
        write_zip(&src, &zip).unwrap();
        let b = Backup {
            path: zip,
            tag: "t".into(),
            secs: 1,
            size: 0,
        };
        assert_eq!(contents(&b).unwrap(), ["Data/a.txt"]);
        std::fs::write(src.join("Data").join("a.txt"), "changed").unwrap();
        restore(&b, &src, &Shared::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(src.join("Data").join("a.txt")).unwrap(),
            "hi"
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
}
