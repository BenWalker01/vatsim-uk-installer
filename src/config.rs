//! Controller pack configuration: stored locally, applied by patching the pack files.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = "controller_pack_config.json";

pub const RATINGS: &[&str] = &[
    "OBS", "S1", "S2", "S3", "C1", "C2 (not used)", "C3", "I1", "I2 (not used)", "I3", "SUP", "ADM",
];

pub const COAST_OPTIONS: &[(&str, &str, &str)] = &[
    ("1", "Blue (default): suitable for NOVA based systems (most APP units)", "9076039"),
    ("2", "Grey: suitable for NODE based systems (STC, LTC, MPC)", "5324604"),
    ("3", "Yellow: high contrast", "32896"),
];

pub const LAND_OPTIONS: &[(&str, &str, &str)] = &[
    ("1", "Mid grey (default)", "3947580"),
    ("2", "Dark grey", "1777181"),
    ("3", "Light grey", "8158332"),
];

/// Embedded preview image for a coastline (`"coastline"`) or land (`"land"`) option.
pub fn preview_bytes(kind: &str, key: &str) -> Option<&'static [u8]> {
    Some(match (kind, key) {
        ("coastline", "1") => include_bytes!("../data/coastline1.png"),
        ("coastline", "2") => include_bytes!("../data/coastline2.png"),
        ("coastline", "3") => include_bytes!("../data/coastline3.png"),
        ("land", "1") => include_bytes!("../data/land1.png"),
        ("land", "2") => include_bytes!("../data/land2.png"),
        ("land", "3") => include_bytes!("../data/land3.png"),
        _ => return None,
    })
}

/// Field names and `y`/`n` flags match the original configurator's JSON so existing files load unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub name: String,
    pub initials: String,
    pub cid: String,
    pub rating: String,
    pub password: String,
    pub cpdlc: String,
    pub discord_presence: String,
    pub realistic_tags: String,
    pub realistic_conversion: String,
    pub coast_choice: String,
    pub land_choice: String,
    pub asel_key: String,
    /// `off`, `radar` (radar displays only) or `on` (SMRs too).
    pub rdf_mode: String,
    pub advanced_config: bool,
}

pub const RDF_OPTIONS: &[(&str, &str)] = &[
    ("off", "Off (default, realistic)"),
    ("radar", "Radar displays only"),
    ("on", "Radar displays and SMRs"),
];

impl Default for Config {
    fn default() -> Self {
        Config {
            name: String::new(),
            initials: String::new(),
            cid: String::new(),
            rating: "0".into(),
            password: String::new(),
            cpdlc: String::new(),
            discord_presence: "n".into(),
            realistic_tags: "y".into(),
            realistic_conversion: "y".into(),
            coast_choice: "1".into(),
            land_choice: "1".into(),
            asel_key: String::new(),
            rdf_mode: "off".into(),
            advanced_config: false,
        }
    }
}

pub fn is_valid_cid(cid: &str) -> bool {
    cid.chars().all(|c| c.is_ascii_digit()) && (6..=7).contains(&cid.len())
}

impl Config {
    /// Returns the first validation problem, if any.
    pub fn validate(&self) -> Option<&'static str> {
        if self.name.trim().is_empty() {
            Some("Name is required.")
        } else if !(2..=3).contains(&self.initials.trim().chars().count()) {
            Some("Initials must be 2-3 letters.")
        } else if !is_valid_cid(&self.cid) {
            Some("CID must be a 6 or 7 digit number.")
        } else if self.password.is_empty() {
            Some("Password is required.")
        } else {
            None
        }
    }
}

/// Local storage: `%APPDATA%\vatsim-uk-installer\controller_pack_config.json`.
pub fn local_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("vatsim-uk-installer").join(CONFIG_FILE))
}

fn read(path: &Path) -> Option<Config> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Load the local config; if none exists (first run) fall back to the one the old configurator
/// left in the pack directory.
pub fn load(pack_dir: Option<&Path>) -> Config {
    local_path()
        .and_then(|p| read(&p))
        .or_else(|| pack_dir.and_then(|d| read(&d.join(CONFIG_FILE))))
        .unwrap_or_default()
}

/// Whether a config exists locally or in the pack.
pub fn exists(pack_dir: Option<&Path>) -> bool {
    local_path().is_some_and(|p| p.exists()) || pack_dir.is_some_and(|d| d.join(CONFIG_FILE).exists())
}

pub fn save(cfg: &Config) -> anyhow::Result<()> {
    let p = local_path().ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(p, serde_json::to_string_pretty(cfg)?)?;
    Ok(())
}

/// Scan code << 16 for a Windows virtual key, as stored in the `.prf` `AselKey` setting.
#[cfg(windows)]
pub fn asel_from_vk(vk: u32) -> Option<String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::MapVirtualKeyA;
    let scan = unsafe { MapVirtualKeyA(vk, 0) };
    (scan != 0).then(|| (scan << 16).to_string())
}

/// Human-readable name of a stored ASEL value (scan code << 16, i.e. the Win32 key-name lParam).
#[cfg(windows)]
pub fn asel_name(code: &str) -> Option<String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyNameTextA;
    let lparam: i32 = code.parse::<u32>().ok()? as i32;
    let mut buf = [0u8; 64];
    let n = unsafe { GetKeyNameTextA(lparam, buf.as_mut_ptr(), buf.len() as i32) };
    (n > 0).then(|| String::from_utf8_lossy(&buf[..n as usize]).into_owned())
}

#[cfg(not(windows))]
pub fn asel_name(_code: &str) -> Option<String> {
    None
}

/// The first virtual key currently held down (keyboard keys only), if any.
#[cfg(windows)]
pub fn pressed_vk() -> Option<u32> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    (8u32..=254)
        .filter(|vk| !matches!(vk, 0x10..=0x12 | 0x5B | 0x5C))
        .find(|&vk| (unsafe { GetAsyncKeyState(vk as i32) } as u16) & 0x8000 != 0)
}

#[cfg(not(windows))]
pub fn asel_from_vk(_vk: u32) -> Option<String> {
    None
}

#[cfg(not(windows))]
pub fn pressed_vk() -> Option<u32> {
    None
}

fn read_text(path: &Path) -> anyhow::Result<String> {
    Ok(std::fs::read_to_string(path)?.replace("\r\n", "\n").replace('\r', "\n"))
}

fn write_text(path: &Path, text: &str) -> anyhow::Result<()> {
    std::fs::write(path, text.replace('\n', "\r\n"))?;
    Ok(())
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn ends_with_ci(s: &str, suffix: &str) -> bool {
    s.to_ascii_lowercase().ends_with(&suffix.to_ascii_lowercase())
}

fn patch_prf(path: &Path, cfg: &Config) -> anyhow::Result<()> {
    let text = read_text(path)?;
    const KEYS: [&str; 5] = ["realname", "certificate", "rating", "callsign", "password"];
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| !KEYS.iter().any(|k| l.starts_with(&format!("LastSession\t{k}"))))
        .map(String::from)
        .collect();
    lines.push(String::new());
    lines.push(format!("LastSession\trealname\t{}", cfg.name));
    lines.push(format!("LastSession\tcertificate\t{}", cfg.cid));
    lines.push(format!("LastSession\trating\t{}", cfg.rating));
    lines.push(format!("LastSession\tcallsign\t{}_OBS", cfg.initials));
    lines.push(format!("LastSession\tpassword\t{}", cfg.password));
    write_text(path, &(lines.join("\n") + "\n"))
}

fn patch_asel(path: &Path, asel_key: &str) -> anyhow::Result<()> {
    if asel_key.is_empty() {
        return Ok(());
    }
    let text = read_text(path)?;
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| !l.starts_with("Settings\tAselKey"))
        .map(String::from)
        .collect();
    lines.push(String::new());
    lines.push(format!("Settings\tAselKey\t{asel_key}"));
    write_text(path, &(lines.join("\n") + "\n"))
}

fn discord_dll_rel(prf: &Path) -> String {
    let dir = prf.parent().unwrap_or(Path::new("."));
    for (depth, root) in dir.ancestors().enumerate() {
        if root.join("Data").join("Plugin").exists() {
            return format!("\\{}Data\\Plugin\\DiscordEuroscope.dll", "..\\".repeat(depth));
        }
    }
    r"\..\Data\Plugin\DiscordEuroscope.dll".into()
}

fn patch_discord(path: &Path, enabled: bool) -> anyhow::Result<()> {
    let text = read_text(path)?;
    let mut lines: Vec<String> = text.split('\n').map(String::from).collect();
    let has = lines.iter().any(|l| l.contains("DiscordEuroscope.dll"));
    if !enabled {
        if has {
            lines.retain(|l| !l.contains("DiscordEuroscope.dll"));
            write_text(path, &(lines.join("\n").trim_end_matches('\n').to_string() + "\n"))?;
        }
        return Ok(());
    }
    if has {
        return Ok(());
    }
    let (mut last_idx, mut max_num) = (None, 0u32);
    for (i, l) in lines.iter().enumerate() {
        if let Some(rest) = l.strip_prefix("Plugins\tPlugin") {
            if let Some((num, _)) = rest.split_once('\t') {
                if let Ok(n) = num.parse::<u32>() {
                    last_idx = Some(i);
                    max_num = max_num.max(n);
                }
            }
        }
    }
    let new_line = format!("Plugins\tPlugin{}\t{}", max_num + 1, discord_dll_rel(path));
    match last_idx {
        Some(i) => lines.insert(i + 1, new_line),
        None => {
            if lines.last().is_some_and(|l| !l.is_empty()) {
                lines.push(String::new());
            }
            lines.push(new_line);
        }
    }
    write_text(path, &(lines.join("\n").trim_end_matches('\n').to_string() + "\n"))
}

fn patch_plugins(path: &Path, cpdlc: &str) -> anyhow::Result<()> {
    let text = read_text(path)?;
    let line = format!("vSMR Vatsim UK:cpdlc_password:{cpdlc}");
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    if let Some(l) = lines.iter_mut().find(|l| l.starts_with("vSMR Vatsim UK:cpdlc_password:")) {
        *l = line;
    } else if let Some(i) = lines.iter().position(|l| l.trim() == "END") {
        lines.insert(i, line);
    }
    write_text(path, &(lines.join("\n") + "\n"))
}

fn replace_in(path: &Path, from: &str, to: &str) -> anyhow::Result<()> {
    let text = read_text(path)?;
    if text.contains(from) {
        write_text(path, &text.replace(from, to))?;
    }
    Ok(())
}

fn lookup(options: &[(&'static str, &'static str, &'static str)], key: &str) -> &'static str {
    options
        .iter()
        .find(|o| o.0 == key)
        .map(|o| o.2)
        .unwrap_or(options[0].2)
}

fn patch_asr(path: &Path, root: &Path, realistic_tags: bool) -> anyhow::Result<()> {
    let rel = path.strip_prefix(root.join("Data").join("ASR")).unwrap_or(path);
    let mut comps = rel.components();
    let top = match (comps.next(), comps.next()) {
        (Some(c), Some(_)) => c.as_os_str().to_string_lossy().to_lowercase(),
        _ => String::new(),
    };
    if !(top.starts_with("ac_") || ["ltc", "heathrow", "gatwick", "essex"].contains(&top.as_str())) {
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

fn patch_rdf(path: &Path, mode: &str) -> anyhow::Result<()> {
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

fn patch_sct(path: &Path, cfg: &Config) -> anyhow::Result<()> {
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

fn patch_correlation(path: &Path, on: bool) -> anyhow::Result<()> {
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

/// Patch the pack in `root` according to `cfg`. Per-file failures are collected, not fatal.
pub fn apply(cfg: &Config, root: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(root.is_dir(), "Controller pack folder not found: {}", root.display());
    let mut files = Vec::new();
    walk(root, &mut files);
    let mut errors = Vec::new();
    let mut note = |r: anyhow::Result<()>, p: &Path| {
        if let Err(e) = r {
            errors.push(format!("{}: {e}", p.display()));
        }
    };

    for p in &files {
        let name = file_name(p);
        if ends_with_ci(&name, ".prf") {
            note(patch_prf(p, cfg), p);
            note(patch_discord(p, cfg.discord_presence == "y"), p);
        } else if name.ends_with("Plugins.txt") {
            note(patch_plugins(p, &cfg.cpdlc), p);
        } else if name.starts_with("UK") && ends_with_ci(&name, ".ese") {
            note(replace_in(p, "EXAMPLE", &cfg.initials), p);
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
        let dir = root.join("Data").join("Plugin").join(sys);
        let target = dir.join("TopSkyCPDLChoppieCode.txt");
        note(
            std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&target, &cfg.cpdlc)).map_err(Into::into),
            &target,
        );
    }

    if cfg.advanced_config {
        for p in &files {
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

    anyhow::ensure!(errors.is_empty(), "{}", errors.join("; "));
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
        let has = |p: &Path| std::fs::read_to_string(p).unwrap().contains("RDF Plugin for Euroscope:EnableDraw:1");
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
    fn cid_validation() {
        assert!(is_valid_cid("1234567"));
        assert!(!is_valid_cid("12345"));
        assert!(!is_valid_cid("12a456"));
    }

    #[test]
    fn old_json_loads() {
        let c: Config = serde_json::from_str(r#"{"name":"A","cid":"1234567","coast_choice":"2"}"#).unwrap();
        assert_eq!(c.coast_choice, "2");
        assert_eq!(c.land_choice, "1");
    }

    #[test]
    fn apply_patches_prf_and_discord() {
        let dir = std::env::temp_dir().join(format!("ukcfg-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Data").join("Plugin")).unwrap();
        let prf = dir.join("a.prf");
        std::fs::write(&prf, "Plugins\tPlugin1\tx.dll\nLastSession\tpassword\told\n").unwrap();
        let cfg = Config {
            name: "N".into(),
            initials: "AB".into(),
            cid: "1234567".into(),
            password: "pw".into(),
            discord_presence: "y".into(),
            ..Default::default()
        };
        apply(&cfg, &dir).unwrap();
        let out = std::fs::read_to_string(&prf).unwrap();
        assert!(out.contains("LastSession\tcallsign\tAB_OBS"));
        assert!(!out.contains("\told"));
        assert!(out.contains("Plugins\tPlugin2\t\\Data\\Plugin\\DiscordEuroscope.dll"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
