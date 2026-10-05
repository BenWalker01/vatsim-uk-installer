//! EuroScope detection, version check and install.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Detected {
    pub path: PathBuf,
    pub version: Option<String>,
}

const CANDIDATES: &[&str] = &[
    r"C:\Program Files (x86)\EuroScope\EuroScope.exe",
    r"C:\Program Files\EuroScope\EuroScope.exe",
];

pub fn detect() -> Option<Detected> {
    CANDIDATES.iter().map(PathBuf::from).find(|p| p.exists()).map(|path| Detected {
        // TODO: read the file version resource from the exe
        version: None,
        path,
    })
}

/// Numeric dotted-version comparison, e.g. "3.2.3.2" >= "3.2.3".
pub fn meets_minimum(found: &str, minimum: &str) -> bool {
    let parse = |s: &str| s.split('.').map(|p| p.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    let (mut a, mut b) = (parse(found), parse(minimum));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a >= b
}

/// TODO: download installer from the manifest and run it, reporting progress.
pub fn install(_url: &str) -> anyhow::Result<()> {
    anyhow::bail!("EuroScope install not implemented")
}

#[cfg(test)]
mod tests {
    use super::meets_minimum;

    #[test]
    fn version_compare() {
        assert!(meets_minimum("3.2.3.2", "3.2.3"));
        assert!(!meets_minimum("3.2.1", "3.2.3"));
    }
}
