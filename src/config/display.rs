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

/// Set the text size (4th `:`-separated field) of the font-related SIMBOLOGY.txt entries.
pub(super) fn patch_symbology_font(path: &Path, size: &str) -> anyhow::Result<()> {
    let text = read_text(path)?;
    let mut changed = false;
    let lines: Vec<String> = text
        .lines()
        .map(|l| {
            let mut parts: Vec<&str> = l.split(':').collect();
            if parts.len() >= 4
                && SYMBOLOGY_FONT_ENTRIES.contains(&format!("{}:{}", parts[0], parts[1]).as_str())
                && parts[3] != size
            {
                parts[3] = size;
                changed = true;
                return parts.join(":");
            }
            l.to_string()
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
    fn font_size_patches_only_font_entries() {
        let dir = std::env::temp_dir().join(format!("ukfont-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("SIMBOLOGY.txt");
        std::fs::write(&f, "Metar:normal:Arial:3.5:1\nOther:foo:Arial:3.5:1\n").unwrap();
        patch_symbology_font(&f, "4.0").unwrap();
        let out = std::fs::read_to_string(&f).unwrap();
        assert!(out.contains("Metar:normal:Arial:4.0:1") && out.contains("Other:foo:Arial:3.5:1"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
