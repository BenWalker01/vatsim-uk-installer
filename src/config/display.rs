use super::text_files::{read_text, write_text};
use super::*;

pub(super) fn lookup(
    options: &[(&'static str, &'static str, &'static str)],
    key: &str,
) -> &'static str {
    options
        .iter()
        .find(|o| o.0 == key)
        .map(|o| o.2)
        .unwrap_or(options[0].2)
}

pub(super) fn patch_asr(path: &Path, root: &Path, realistic_tags: bool) -> anyhow::Result<()> {
    let rel = path
        .strip_prefix(root.join("Data").join("ASR"))
        .unwrap_or(path);
    let mut comps = rel.components();
    let top = match (comps.next(), comps.next()) {
        (Some(c), Some(_)) => c.as_os_str().to_string_lossy().to_lowercase(),
        _ => String::new(),
    };
    if !(top.starts_with("ac_") || ["ltc", "heathrow", "gatwick", "essex"].contains(&top.as_str()))
    {
        return Ok(());
    }
    let text = read_text(path)?;
    let lines: Vec<String> = text
        .lines()
        .map(|l| {
            if l.starts_with("TAGFAMILY:") && (l.contains("NODE") || l.contains("AC")) {
                if !realistic_tags && !l.contains("-Easy") {
                    return format!("{}-Easy", l.trim());
                } else if realistic_tags && l.contains("-Easy") {
                    return l.replace("-Easy", "");
                }
            }
            l.to_string()
        })
        .collect();
    write_text(path, &(lines.join("\n") + "\n"))
}

pub(super) fn patch_rdf(path: &Path, mode: &str) -> anyhow::Result<()> {
    const RDF_LINE: &str = "PLUGIN:RDF Plugin for Euroscope:EnableDraw:1";
    let text = read_text(path)?;
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let has = lines.iter().any(|l| l.contains(RDF_LINE));
    let is_smr = lines.iter().any(|l| l.contains("DisplayTypeName:SMR"));
    let want = match mode {
        "on" => true,
        "radar" => !is_smr,
        _ => false,
    };
    if want == has {
        return Ok(());
    }
    if want {
        lines.push(RDF_LINE.into());
    } else {
        lines.retain(|l| !l.contains(RDF_LINE));
    }
    write_text(path, &(lines.join("\n") + "\n"))
}

pub(super) fn patch_smr_north_up(path: &Path) -> anyhow::Result<()> {
    let text = read_text(path)?;
    let mut is_smr = false;
    let mut has_rotation = false;
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.contains("DisplayTypeName:SMR") {
            is_smr = true;
        }
        if line.starts_with("DisplayRotation:") {
            if !has_rotation {
                lines.push("DisplayRotation:0.00000".to_string());
                has_rotation = true;
            }
        } else {
            lines.push(line.to_string());
        }
    }
    if !is_smr {
        return Ok(());
    }
    if !has_rotation {
        lines.push("DisplayRotation:0.00000".into());
    }
    write_text(path, &(lines.join("\n") + "\n"))
}

pub(super) fn patch_sct(path: &Path, cfg: &Config) -> anyhow::Result<()> {
    let text = read_text(path)?;
    let lines: Vec<String> = text
        .lines()
        .map(|l| {
            if l.starts_with("#define coast ") {
                format!("#define coast {}", lookup(COAST_OPTIONS, &cfg.coast_choice))
            } else if l.starts_with("#define land ") {
                format!("#define land {}", lookup(LAND_OPTIONS, &cfg.land_choice))
            } else {
                l.to_string()
            }
        })
        .collect();
    write_text(path, &(lines.join("\n") + "\n"))
}

/// Set each list column's text size (the final field of each `m_Column` line).
pub(super) fn patch_list_font(path: &Path, size: &str) -> anyhow::Result<()> {
    let text = read_text(path)?;
    let mut changed = false;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let mut parts: Vec<&str> = line.split(':').collect();
            if parts.first() == Some(&"m_Column")
                && parts.len() > 1
                && parts.last().is_some_and(|value| value.parse::<f32>().is_ok())
                && parts.last() != Some(&size)
            {
                *parts.last_mut().unwrap() = size;
                changed = true;
                return parts.join(":");
            }
            line.to_string()
        })
        .collect();
    if changed {
        write_text(path, &(lines.join("\n") + "\n"))?;
    }
    Ok(())
}

pub(super) fn patch_correlation(path: &Path, on: bool) -> anyhow::Result<()> {
    let text = read_text(path)?;
    let mut modified = false;
    let lines: Vec<String> = text
        .lines()
        .map(|l| {
            if l.trim().starts_with("m_CorrelationMode:") {
                modified = true;
                format!("m_CorrelationMode:{}", if on { 1 } else { 0 })
            } else {
                l.to_string()
            }
        })
        .collect();
    if modified {
        write_text(path, &(lines.join("\n") + "\n"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rdf_modes() {
        let dir = std::env::temp_dir().join(format!("ukrdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let radar = dir.join("r.asr");
        let smr = dir.join("s.asr");
        std::fs::write(&radar, "DisplayTypeName:Radar\n").unwrap();
        std::fs::write(&smr, "DisplayTypeName:SMR\n").unwrap();
        let has = |p: &Path| {
            std::fs::read_to_string(p)
                .unwrap()
                .contains("RDF Plugin for Euroscope:EnableDraw:1")
        };
        patch_rdf(&radar, "radar").unwrap();
        patch_rdf(&smr, "radar").unwrap();
        assert!(has(&radar) && !has(&smr));
        patch_rdf(&smr, "on").unwrap();
        assert!(has(&smr));
        patch_rdf(&radar, "off").unwrap();
        patch_rdf(&smr, "off").unwrap();
        assert!(!has(&radar) && !has(&smr));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn north_up_sets_every_smr_rotation_without_touching_other_displays() {
        let dir = std::env::temp_dir().join(format!("ukrotation-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let defined = dir.join("defined.asr");
        let missing = dir.join("missing.asr");
        let radar = dir.join("radar.asr");
        std::fs::write(
            &defined,
            "DisplayTypeName:SMR\nDisplayRotation:90.00000\nDisplayRotation:180.00000\n",
        )
        .unwrap();
        std::fs::write(&missing, "DisplayTypeName:SMR\n").unwrap();
        std::fs::write(&radar, "DisplayTypeName:Radar\nDisplayRotation:90.00000\n").unwrap();

        patch_smr_north_up(&defined).unwrap();
        patch_smr_north_up(&missing).unwrap();
        patch_smr_north_up(&radar).unwrap();

        let defined_text = read_text(&defined).unwrap();
        assert_eq!(
            defined_text
                .lines()
                .filter(|line| *line == "DisplayRotation:0.00000")
                .count(),
            1
        );
        assert_eq!(
            read_text(&missing).unwrap(),
            "DisplayTypeName:SMR\nDisplayRotation:0.00000\n"
        );
        assert_eq!(
            read_text(&radar).unwrap(),
            "DisplayTypeName:Radar\nDisplayRotation:90.00000\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_font_patches_only_m_column_size_fields() {
        let dir = std::env::temp_dir().join(format!("uklistfont-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Lists_SMR_RECAT.txt");
        std::fs::write(
            &file,
            "m_Column:SPad:7:1:19:29:29:1::::4:0.0\nm_Column:Std:5:1:110:9008:9007:1:UK Controller Plugin:UK Controller Plugin:UK Controller Plugin:4:15.0\nOther:unchanged\n",
        )
        .unwrap();

        patch_list_font(&file, "6.5").unwrap();

        assert_eq!(
            read_text(&file).unwrap(),
            "m_Column:SPad:7:1:19:29:29:1::::4:6.5\nm_Column:Std:5:1:110:9008:9007:1:UK Controller Plugin:UK Controller Plugin:UK Controller Plugin:4:6.5\nOther:unchanged\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
