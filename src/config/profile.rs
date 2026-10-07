use super::text_files::{read_text, write_text};
use super::*;

pub(super) fn patch_prf(path: &Path, cfg: &Config) -> anyhow::Result<()> {
    let text = read_text(path)?;
    const KEYS: [&str; 5] = ["realname", "certificate", "rating", "callsign", "password"];
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| {
            !KEYS
                .iter()
                .any(|k| l.starts_with(&format!("LastSession\t{k}")))
        })
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

pub(super) fn patch_asel(path: &Path, asel_key: &str) -> anyhow::Result<()> {
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

pub(super) fn discord_dll_rel(prf: &Path) -> String {
    let dir = prf.parent().unwrap_or(Path::new("."));
    for (depth, root) in dir.ancestors().enumerate() {
        if root.join("Data").join("Plugin").exists() {
            return format!(
                "\\{}Data\\Plugin\\DiscordEuroscope.dll",
                "..\\".repeat(depth)
            );
        }
    }
    r"\..\Data\Plugin\DiscordEuroscope.dll".into()
}

pub(super) fn patch_discord(path: &Path, enabled: bool) -> anyhow::Result<()> {
    let text = read_text(path)?;
    let mut lines: Vec<String> = text.split('\n').map(String::from).collect();
    let has = lines.iter().any(|l| l.contains("DiscordEuroscope.dll"));
    if !enabled {
        if has {
            lines.retain(|l| !l.contains("DiscordEuroscope.dll"));
            write_text(
                path,
                &(lines.join("\n").trim_end_matches('\n').to_string() + "\n"),
            )?;
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
    write_text(
        path,
        &(lines.join("\n").trim_end_matches('\n').to_string() + "\n"),
    )
}

pub(super) fn patch_plugins(path: &Path, cpdlc: &str) -> anyhow::Result<()> {
    let text = read_text(path)?;
    let line = format!("vSMR Vatsim UK:cpdlc_password:{cpdlc}");
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    if let Some(l) = lines
        .iter_mut()
        .find(|l| l.starts_with("vSMR Vatsim UK:cpdlc_password:"))
    {
        *l = line;
    } else if let Some(i) = lines.iter().position(|l| l.trim() == "END") {
        lines.insert(i, line);
    }
    write_text(path, &(lines.join("\n") + "\n"))
}
