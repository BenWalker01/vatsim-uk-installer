use super::*;

pub(super) fn read_text(path: &Path) -> anyhow::Result<String> {
    Ok(std::fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}

pub(super) fn write_text(path: &Path, text: &str) -> anyhow::Result<()> {
    std::fs::write(path, text.replace('\n', "\r\n"))?;
    Ok(())
}

pub(super) fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

pub(super) fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(super) fn ends_with_ci(s: &str, suffix: &str) -> bool {
    s.to_ascii_lowercase()
        .ends_with(&suffix.to_ascii_lowercase())
}

pub(super) fn replace_in(path: &Path, from: &str, to: &str) -> anyhow::Result<()> {
    let text = read_text(path)?;
    if text.contains(from) {
        write_text(path, &text.replace(from, to))?;
    }
    Ok(())
}
