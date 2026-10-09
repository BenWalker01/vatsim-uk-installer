//! Import AeroNav AeroNav navigation data into the controller pack.

use std::{
    fmt,
    io::Read,
    path::{Path},
};

pub const AERONAV_URL: &str = "https://files.aero-nav.com/EGXX";

const GNG_FILES: [(&str, &str); 6] = [
    ("ICAO_Aircraft.txt", "icao_aircraft.txt"),
    ("ICAO_Airlines.txt", "icao_airlines.txt"),
    ("ICAO_Airports.txt", "icao_airports.txt"),
    ("airway.txt", "airway.txt"),
    ("icao.txt", "icao.txt"),
    ("isec.txt", "isec.txt"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AiracCycle {
    pub year: u8,
    pub cycle: u8,
}

impl fmt::Display for AiracCycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}{:02}", self.year, self.cycle)
    }
}

pub fn airac_cycles(pack_dir: &Path) -> anyhow::Result<(usize, Vec<(String, AiracCycle)>)> {
    let data_dir = pack_dir.join("Data").join("Datafiles");
    let mut paths = std::fs::read_dir(data_dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| {
        path.is_file()
            && path.file_name().is_some_and(|name| {
                GNG_FILES
                    .iter()
                    .any(|(target, _)| name.eq_ignore_ascii_case(target))
            })
    });
    paths.sort();

    let file_count = paths.len();
    let mut cycles = Vec::new();
    for path in paths {
        let mut file = std::fs::File::open(&path)?;
        let mut header = [0; 4096];
        let read = std::io::Read::read(&mut file, &mut header)?;
        if let Some(cycle) = parse_airac_cycle(&String::from_utf8_lossy(&header[..read])) {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            cycles.push((name, cycle));
        }
    }
    Ok((file_count, cycles))
}

fn parse_airac_cycle(header: &str) -> Option<AiracCycle> {
    let marker = header.find("AIRAC")?;
    let after_marker = &header[marker + "AIRAC".len()..];
    let bytes = after_marker.as_bytes();
    let digits = bytes.windows(4).enumerate().find_map(|(index, window)| {
        (window.iter().all(|byte| byte.is_ascii_digit())
            && index
                .checked_sub(1)
                .and_then(|previous| bytes.get(previous))
                .is_none_or(|byte| !byte.is_ascii_digit())
            && bytes
                .get(index + 4)
                .is_none_or(|byte| !byte.is_ascii_digit()))
        .then_some(index)
    })?;
    let year = (bytes[digits] - b'0') * 10 + bytes[digits + 1] - b'0';
    let cycle = (bytes[digits + 2] - b'0') * 10 + bytes[digits + 3] - b'0';
    (cycle != 0 && cycle <= 13).then_some(AiracCycle { year, cycle })
}

pub fn matches_pack_cycle(cycle: AiracCycle, version: crate::pack::manifest::PackVersion) -> bool {
    cycle.year == (version.year % 100) as u8 && cycle.cycle == version.cycle
}

pub fn missing_files(pack_dir: &Path) -> Vec<&'static str> {
    let data_dir = pack_dir.join("Data").join("Datafiles");
    GNG_FILES
        .iter()
        .filter_map(|(target, _)| (!data_dir.join(target).is_file()).then_some(*target))
        .collect()
}

/// Import recognized AeroNav files from a ZIP or 7z archive.
pub fn import_gng_archive(archive_path: &Path, pack_dir: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        archive_path.is_file(),
        "AeroNav archive not found: {}",
        archive_path.display()
    );

    let extension = archive_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extracted = if extension == "7z" {
        read_gng_7z(archive_path)?
    } else if extension == "zip" {
        read_gng_zip(archive_path)?
    } else {
        anyhow::bail!("Unsupported AeroNav archive type: .{extension}");
    };

    anyhow::ensure!(
        extracted
            .iter()
            .any(|(target, _)| matches!(*target, "ICAO_Airports.txt" | "airway.txt" | "icao.txt")),
        "Archive does not look like a valid AeroNav navdata package."
    );

    let data_dir = pack_dir.join("Data").join("Datafiles");
    std::fs::create_dir_all(&data_dir)?;
    for (target, contents) in &extracted {
        std::fs::write(data_dir.join(target), contents)?;
    }

    if let Some((_, contents)) = extracted
        .iter()
        .find(|(target, _)| *target == "ICAO_Airlines.txt")
    {
        let vsmr_dir = pack_dir.join("Data").join("Plugin").join("vSMR");
        if vsmr_dir.is_dir() {
            std::fs::write(vsmr_dir.join("ICAO_Airlines.txt"), contents)?;
        }
    }

    Ok(())
}

fn read_gng_zip(path: &Path) -> anyhow::Result<Vec<(&'static str, Vec<u8>)>> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?)?;
    let entries: Vec<(String, String)> = (0..archive.len())
        .map(|index| {
            let entry = archive.by_index(index)?;
            let name = entry.name().to_owned();
            let basename = archive_basename(&name);
            Ok((name, basename))
        })
        .collect::<anyhow::Result<_>>()?;

    let mut extracted = Vec::with_capacity(GNG_FILES.len());
    for (target, accepted) in GNG_FILES {
        let source = entries
            .iter()
            .filter(|(_, basename)| basename == accepted)
            .min_by_key(|(name, _)| name.len())
            .map(|(name, _)| name);
        if let Some(source) = source {
            let mut entry = archive.by_name(source)?;
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents)?;
            extracted.push((target, contents));
        }
    }
    Ok(extracted)
}

fn read_gng_7z(path: &Path) -> anyhow::Result<Vec<(&'static str, Vec<u8>)>> {
    let mut archive = sevenz_rust2::ArchiveReader::new(
        std::fs::File::open(path)?,
        sevenz_rust2::Password::empty(),
    )?;
    let entries: Vec<(String, String)> = archive
        .archive()
        .files
        .iter()
        .filter(|entry| !entry.is_directory && entry.has_stream)
        .map(|entry| (entry.name.clone(), archive_basename(&entry.name)))
        .collect();
    let mut extracted = Vec::with_capacity(GNG_FILES.len());
    for (target, accepted) in GNG_FILES {
        let source = entries
            .iter()
            .filter(|(_, basename)| basename == accepted)
            .min_by_key(|(name, _)| name.len())
            .map(|(name, _)| name);
        if let Some(source) = source {
            extracted.push((target, archive.read_file(source)?));
        }
    }
    Ok(extracted)
}

fn archive_basename(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, path::PathBuf};


    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("vuk-AeroNav-{name}-{}", std::process::id()))
    }

    #[test]
    fn reads_airac_from_header_and_matches_pack_cycle_ignoring_hotfix_suffix() {
        let cycle = parse_airac_cycle(
            ";================================================================;\n; AIRAC 2610/1 ++ File-Publisher: London - EGTT\n",
        )
        .unwrap();
        assert_eq!(cycle.to_string(), "2610");
        assert!(matches_pack_cycle(cycle, "2026_10b".parse().unwrap()));
        assert!(!matches_pack_cycle(cycle, "2026_09".parse().unwrap()));
    }

    #[test]
    fn ignores_missing_malformed_and_unsupported_airac_headers() {
        assert_eq!(parse_airac_cycle("; no cycle here"), None);
        assert_eq!(parse_airac_cycle("; AIRAC 26100/1"), None);
        assert_eq!(parse_airac_cycle("; AIRAC 2614/1"), None);
    }

    #[test]
    fn reads_airac_cycles_from_all_files_in_datafiles() {
        let root = test_dir("all-cycles");
        let data_dir = root.join("Data").join("Datafiles");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(data_dir.join("airway.txt"), "; AIRAC 2610/1\n").unwrap();
        std::fs::write(data_dir.join("icao.txt"), "; AIRAC 2610/1\n").unwrap();
        std::fs::write(data_dir.join("isec.txt"), "; no cycle\n").unwrap();
        std::fs::write(data_dir.join("performance.txt"), "; AIRAC 2501/1\n").unwrap();
        std::fs::create_dir(data_dir.join("subfolder")).unwrap();

        let (count, cycles) = airac_cycles(&root).unwrap();
        assert_eq!(count, 3);
        assert_eq!(
            cycles,
            vec![
                (
                    "airway.txt".to_owned(),
                    AiracCycle {
                        year: 26,
                        cycle: 10
                    }
                ),
                (
                    "icao.txt".to_owned(),
                    AiracCycle {
                        year: 26,
                        cycle: 10
                    }
                ),
            ]
        );
        assert!(!cycles.iter().any(|(name, _)| name == "performance.txt"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn imports_required_files_from_nested_zip_paths_and_copies_airlines_to_vsmr() {
        let root = test_dir("import");
        let zip_path = root.join("AeroNav.zip");
        std::fs::create_dir_all(&root).unwrap();
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        for (target, source) in GNG_FILES {
            zip.start_file(
                format!("AeroNav/Data/{source}"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(target.as_bytes()).unwrap();
        }
        zip.finish().unwrap();

        let pack_dir = root.join("UK");
        let vsmr_dir = pack_dir.join("Data").join("Plugin").join("vSMR");
        std::fs::create_dir_all(&vsmr_dir).unwrap();
        import_gng_archive(&zip_path, &pack_dir).unwrap();

        for (target, _) in GNG_FILES {
            assert_eq!(
                std::fs::read(pack_dir.join("Data").join("Datafiles").join(target)).unwrap(),
                target.as_bytes()
            );
        }
        assert_eq!(
            std::fs::read(vsmr_dir.join("ICAO_Airlines.txt")).unwrap(),
            b"ICAO_Airlines.txt"
        );
        assert!(missing_files(&pack_dir).is_empty());
        assert!(zip_path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn imports_gng_files_from_7z_archive() {
        let root = test_dir("seven-zip-import");
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("gng.7z");
        let mut archive = sevenz_rust2::ArchiveWriter::create(&archive_path).unwrap();
        for (target, source) in GNG_FILES {
            archive
                .push_archive_entry(
                    sevenz_rust2::ArchiveEntry::new_file(&format!("GNG/Data/{source}")),
                    Some(std::io::Cursor::new(target.as_bytes())),
                )
                .unwrap();
        }
        archive.finish().unwrap();

        let pack_dir = root.join("UK");
        import_gng_archive(&archive_path, &pack_dir).unwrap();
        for (target, _) in GNG_FILES {
            assert_eq!(
                std::fs::read(pack_dir.join("Data").join("Datafiles").join(target)).unwrap(),
                target.as_bytes()
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_unrelated_archives_without_writing_partial_files() {
        let root = test_dir("incomplete");
        let zip_path = root.join("AeroNav.zip");
        std::fs::create_dir_all(&root).unwrap();
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        zip.start_file(
            "icao_aircraft.txt",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"aircraft").unwrap();
        zip.finish().unwrap();

        let pack_dir = root.join("UK");
        assert!(import_gng_archive(&zip_path, &pack_dir).is_err());
        assert!(!pack_dir.join("Data").exists());
        assert!(zip_path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn accepts_archives_with_at_least_one_core_navdata_file() {
        let root = test_dir("partial");
        let zip_path = root.join("AeroNav.zip");
        std::fs::create_dir_all(&root).unwrap();
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        zip.start_file(
            "icao_airports.txt",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"airports").unwrap();
        zip.finish().unwrap();

        let pack_dir = root.join("UK");
        import_gng_archive(&zip_path, &pack_dir).unwrap();
        assert_eq!(
            std::fs::read(
                pack_dir
                    .join("Data")
                    .join("Datafiles")
                    .join("ICAO_Airports.txt")
            )
            .unwrap(),
            b"airports"
        );
        assert_eq!(missing_files(&pack_dir).len(), GNG_FILES.len() - 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
