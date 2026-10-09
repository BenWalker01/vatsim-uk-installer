//! Import AeroNav GNG navigation data into the controller pack.

use std::path::{Path, PathBuf};

pub const AERONAV_URL: &str = "https://files.aero-nav.com/EGXX";

const GNG_FILES: [(&str, &str); 6] = [
    ("ICAO_Aircraft.txt", "icao_aircraft.txt"),
    ("ICAO_Airlines.txt", "icao_airlines.txt"),
    ("ICAO_Airports.txt", "icao_airports.txt"),
    ("airway.txt", "airway.txt"),
    ("icao.txt", "icao.txt"),
    ("isec.txt", "isec.txt"),
];

pub fn missing_files(pack_dir: &Path) -> Vec<&'static str> {
    let data_dir = pack_dir.join("Data").join("Datafiles");
    GNG_FILES
        .iter()
        .filter_map(|(target, _)| (!data_dir.join(target).is_file()).then_some(*target))
        .collect()
}

/// Import recognized GNG files without extracting arbitrary archive paths.
pub fn import_gng_zip(zip_path: &Path, pack_dir: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        zip_path.is_file(),
        "GNG ZIP not found: {}",
        zip_path.display()
    );

    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    let entries: Vec<(String, String)> = (0..archive.len())
        .map(|index| {
            let entry = archive.by_index(index)?;
            let name = entry.name().to_owned();
            let basename = name
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&name)
                .to_lowercase();
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
            std::io::Read::read_to_end(&mut entry, &mut contents)?;
            extracted.push((target, contents));
        }
    }
    drop(archive);

    anyhow::ensure!(
        extracted.iter().any(|(target, _)| {
            matches!(*target, "ICAO_Airports.txt" | "airway.txt" | "icao.txt")
        }),
        "ZIP does not look like a valid GNG navdata package."
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

    std::fs::remove_file(zip_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("vuk-gng-{name}-{}", std::process::id()))
    }

    #[test]
    fn imports_required_files_from_nested_zip_paths_and_copies_airlines_to_vsmr() {
        let root = test_dir("import");
        let zip_path = root.join("gng.zip");
        std::fs::create_dir_all(&root).unwrap();
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        for (target, source) in GNG_FILES {
            zip.start_file(
                format!("GNG/Data/{source}"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(target.as_bytes()).unwrap();
        }
        zip.finish().unwrap();

        let pack_dir = root.join("UK");
        let vsmr_dir = pack_dir.join("Data").join("Plugin").join("vSMR");
        std::fs::create_dir_all(&vsmr_dir).unwrap();
        import_gng_zip(&zip_path, &pack_dir).unwrap();

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
        assert!(!zip_path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_unrelated_archives_without_writing_partial_files() {
        let root = test_dir("incomplete");
        let zip_path = root.join("gng.zip");
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
        assert!(import_gng_zip(&zip_path, &pack_dir).is_err());
        assert!(!pack_dir.join("Data").exists());
        assert!(zip_path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn accepts_archives_with_at_least_one_core_navdata_file() {
        let root = test_dir("partial");
        let zip_path = root.join("gng.zip");
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
        import_gng_zip(&zip_path, &pack_dir).unwrap();
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
