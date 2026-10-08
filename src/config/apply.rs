use super::display::*;
use super::profile::*;
use super::text_files::*;
use super::*;
use crate::common::download;

/// Patch the pack in `root` according to `cfg`. Per-file failures are collected, not fatal.
pub fn apply(cfg: &Config, root: &Path, shared: &download::Shared) -> anyhow::Result<()> {
    anyhow::ensure!(
        root.is_dir(),
        "Controller pack folder not found: {}",
        root.display()
    );
    download::set_message(shared, "Applying configuration");
    let mut files = Vec::new();
    walk(root, &mut files);
    {
        let passes = if cfg.advanced_config { 2 } else { 1 };
        let rotation_pass = usize::from(cfg.north_up_smrs);
        shared.lock().unwrap().total = Some((files.len() * (passes + rotation_pass) + 5) as u64);
    }
    let step = || shared.lock().unwrap().done += 1;
    let mut errors = Vec::new();
    let mut note = |r: anyhow::Result<()>, p: &Path| {
        if let Err(e) = r {
            errors.push(format!("{}: {e}", p.display()));
        }
    };

    for p in &files {
        step();
        let name = file_name(p);
        if ends_with_ci(&name, ".prf") {
            note(patch_prf(p, cfg), p);
            note(patch_discord(p, cfg.discord_presence == "y"), p);
        } else if name.ends_with("Plugins.txt") {
            note(patch_plugins(p, &cfg.cpdlc), p);
        } else if name.starts_with("UK") && ends_with_ci(&name, ".ese") {
            note(replace_in(p, "EXAMPLE", &cfg.initials), p);
        } else if cfg.font_size != "default"
            && name.starts_with("Lists")
            && ends_with_ci(&name, ".txt")
            && p.parent()
                .is_some_and(|d| d.ends_with(Path::new("Data").join("Settings")))
        {
            let size = format!("{:.1}", font_size_value(&cfg.font_size));
            note(patch_list_font(p, &size), p);
        } else if name.ends_with("Profiles.txt") {
            note(
                replace_in(
                    p,
                    "Submit feedback at vats.im/atcfb",
                    &format!("Submit feedback at vatsim.uk/atcfb?cid={}", cfg.cid),
                ),
                p,
            );
        }
    }
    for sys in ["TopSky_iTEC", "TopSky_NERC", "TopSky_NODE", "TopSky_NOVA"] {
        step();
        let dir = root.join("Data").join("Plugin").join(sys);
        let target = dir.join("TopSkyCPDLChoppieCode.txt");
        note(
            std::fs::create_dir_all(&dir)
                .and_then(|_| std::fs::write(&target, &cfg.cpdlc))
                .map_err(Into::into),
            &target,
        );
    }

    if cfg.advanced_config {
        for p in &files {
            step();
            let name = file_name(p);
            if ends_with_ci(&name, ".asr") {
                note(patch_asr(p, root, cfg.realistic_tags == "y"), p);
                note(patch_rdf(p, &cfg.rdf_mode), p);
            }
            if name.starts_with("UK_") && ends_with_ci(&name, ".sct") {
                note(patch_sct(p, cfg), p);
            }
            let in_settings = p
                .parent()
                .is_some_and(|d| d.ends_with(Path::new("Data").join("Settings")));
            if in_settings && ends_with_ci(&name, ".txt") && !name.ends_with("_SMR.txt") {
                note(patch_correlation(p, cfg.realistic_conversion == "y"), p);
            }
            if ends_with_ci(&name, ".prf") {
                note(patch_asel(p, &cfg.asel_key), p);
            }
        }
    }

    // Last, so saved screen positions win over anything the passes above touched.
    drop(note);
    step();
    if let Err(e) = crate::pack::layout::apply(root) {
        errors.push(format!("saved screen layout: {e}"));
    }

    if cfg.north_up_smrs {
        for p in &files {
            step();
            if ends_with_ci(&file_name(p), ".asr") {
                if let Err(e) = patch_smr_north_up(p) {
                    errors.push(format!("{}: {e}", p.display()));
                }
            }
        }
    }

    anyhow::ensure!(errors.is_empty(), "{}", errors.join("; "));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_patches_prf_and_discord() {
        let dir = std::env::temp_dir().join(format!("ukcfg-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Data").join("Plugin")).unwrap();
        let settings = dir.join("Data").join("Settings");
        std::fs::create_dir_all(&settings).unwrap();
        let prf = dir.join("a.prf");
        std::fs::write(
            &prf,
            "Plugins\tPlugin1\tx.dll\nLastSession\tpassword\told\n",
        )
        .unwrap();
        let list = settings.join("Lists_SMR_RECAT.txt");
        std::fs::write(&list, "m_Column:SPad:7:1:19:29:29:1::::4:0.0\n").unwrap();
        let cfg = Config {
            name: "N".into(),
            initials: "AB".into(),
            cid: "1234567".into(),
            password: "pw".into(),
            discord_presence: "y".into(),
            font_size: "6.5".into(),
            ..Default::default()
        };
        apply(&cfg, &dir, &download::Shared::default()).unwrap();
        let out = std::fs::read_to_string(&prf).unwrap();
        assert!(out.contains("LastSession\tcallsign\tAB_OBS"));
        assert!(!out.contains("\told"));
        assert!(out.contains("Plugins\tPlugin2\t\\Data\\Plugin\\DiscordEuroscope.dll"));
        assert!(std::fs::read_to_string(&list)
            .unwrap()
            .contains("m_Column:SPad:7:1:19:29:29:1::::4:6.5"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
