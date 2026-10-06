//! Screen layout persistence: diff the pack's ASR files against the pristine ASRs of the same
//! pack version, store what the user changed, and re-apply it after the pack is updated or reconfigured.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const LAYOUT_FILE: &str = "asr_layout.json";

/// ASR path (relative to `Data\ASR`, `/`-separated) -> contents.
pub type Baseline = BTreeMap<String, String>;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileChange {
    pub removed: Vec<String>,
    pub added: Vec<String>,
}

/// ASR path -> the lines the user changed.
pub type Layout = BTreeMap<String, FileChange>;

fn store_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("vatsim-uk-installer"))
}

fn store_path(name: &str) -> Option<PathBuf> {
    store_dir().map(|d| d.join(name))
}

fn save_json<T: Serialize>(name: &str, value: &T) -> anyhow::Result<()> {
    let p = store_path(name).ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(p, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

fn load_json<T: for<'de> Deserialize<'de>>(name: &str) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(store_path(name)?).ok()?).ok()
}

pub fn load_layout() -> Layout {
    load_json(LAYOUT_FILE).unwrap_or_default()
}

pub fn clear_layout() -> anyhow::Result<()> {
    if let Some(p) = store_path(LAYOUT_FILE) {
        if p.exists() {
            std::fs::remove_file(p)?;
        }
    }
    Ok(())
}

pub fn norm(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn collect_asrs(dir: &Path, root: &Path, out: &mut Baseline) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_asrs(&p, root, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("asr")) {
            if let (Ok(rel), Ok(text)) = (p.strip_prefix(root), std::fs::read_to_string(&p)) {
                let key = rel.to_string_lossy().replace('\\', "/");
                out.insert(key, norm(&text));
            }
        }
    }
}

fn read_asrs(pack: &Path) -> Baseline {
    let root = pack.join("Data").join("ASR");
    let mut out = Baseline::new();
    collect_asrs(&root, &root, &mut out);
    out
}

/// Lines the installer's own config step rewrites; they are not part of the user's layout.
fn is_config_line(l: &str) -> bool {
    l.starts_with("TAGFAMILY:") || l.contains("PLUGIN:RDF Plugin for Euroscope:EnableDraw")
}

fn line_set(text: &str) -> BTreeSet<&str> {
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty() && !is_config_line(l))
        .collect()
}

fn diff(baseline: &Baseline, current: &Baseline) -> Layout {
    let mut out = Layout::new();
    for (name, now) in current {
        let Some(before) = baseline.get(name) else { continue };
        let (a, b) = (line_set(before), line_set(now));
        let change = FileChange {
            removed: a.difference(&b).map(|s| s.to_string()).collect(),
            added: b.difference(&a).map(|s| s.to_string()).collect(),
        };
        if !change.removed.is_empty() || !change.added.is_empty() {
            out.insert(name.clone(), change);
        }
    }
    out
}

/// Diff the pack's ASRs against `pristine` (the same pack version, unmodified) and store the result,
/// replacing any earlier layout. Returns the number of files with changes.
pub fn save_changes(pack: &Path, pristine: &Baseline) -> anyhow::Result<usize> {
    anyhow::ensure!(!pristine.is_empty(), "The reference pack contained no ASR files.");
    let changes = diff(pristine, &read_asrs(pack));
    let count = changes.len();
    save_json(LAYOUT_FILE, &changes)?;
    Ok(count)
}

fn apply_change(text: &str, change: &FileChange) -> String {
    let mut lines: Vec<String> = norm(text)
        .lines()
        .filter(|l| !change.removed.iter().any(|r| r == l.trim_end()))
        .map(String::from)
        .collect();
    for add in &change.added {
        if !lines.iter().any(|l| l.trim_end() == add) {
            lines.push(add.clone());
        }
    }
    lines.join("\r\n") + "\r\n"
}

/// Re-apply the saved layout to the pack's ASR files. Missing files are skipped.
pub fn apply(pack: &Path) -> anyhow::Result<usize> {
    apply_layout(pack, &load_layout())
}

fn apply_layout(pack: &Path, layout: &Layout) -> anyhow::Result<usize> {
    let root = pack.join("Data").join("ASR");
    let mut applied = 0;
    for (name, change) in layout {
        let path = root.join(name);
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        std::fs::write(&path, apply_change(&text, change))?;
        applied += 1;
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> Baseline {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn diff_and_apply_round_trip() {
        let before = map(&[("a.asr", "WINDOWAREA:1:2\nFOO:1\n"), ("b.asr", "X:1\n")]);
        let after = map(&[("a.asr", "WINDOWAREA:5:6\r\nFOO:1\r\n"), ("b.asr", "X:1\n")]);
        let layout = diff(&before, &after);
        assert_eq!(layout.len(), 1);
        let fresh = apply_change("WINDOWAREA:1:2\nFOO:1\n", &layout["a.asr"]);
        assert!(fresh.contains("WINDOWAREA:5:6") && !fresh.contains("WINDOWAREA:1:2") && fresh.contains("FOO:1"));
        // Idempotent.
        assert_eq!(apply_change(&fresh, &layout["a.asr"]), fresh);
    }

    #[test]
    fn config_lines_are_ignored() {
        let before = map(&[("a.asr", "TAGFAMILY:NODE\nP:1\n")]);
        let after = map(&[("a.asr", "TAGFAMILY:NODE-Easy\nP:1\nPLUGIN:RDF Plugin for Euroscope:EnableDraw:1\n")]);
        assert!(diff(&before, &after).is_empty());
    }
}
