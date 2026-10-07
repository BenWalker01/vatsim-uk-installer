//! Checks GitHub Releases for a newer installer build and replaces the running exe.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use serde::Deserialize;

use crate::download::{self, Shared};

const LATEST_URL: &str =
    "https://api.github.com/repos/BenWalker01/vatsim-uk-installer/releases/latest";
const ASSET_NAME: &str = "vatsim-uk-installer.exe";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub sha256: String,
}

fn parse_version(s: &str) -> Option<Vec<u32>> {
    s.trim()
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse().ok())
        .collect()
}

fn is_newer(candidate: &str, current: &str) -> bool {
    let (Some(mut a), Some(mut b)) = (parse_version(candidate), parse_version(current)) else {
        return false;
    };
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

fn parse_release(body: &str, current: &str) -> anyhow::Result<Option<Release>> {
    #[derive(Deserialize)]
    struct Rel {
        tag_name: String,
        assets: Vec<Ast>,
    }
    #[derive(Deserialize)]
    struct Ast {
        name: String,
        digest: Option<String>,
        browser_download_url: String,
    }

    let rel: Rel = serde_json::from_str(body)?;
    if !is_newer(&rel.tag_name, current) {
        return Ok(None);
    }
    let release = rel
        .assets
        .iter()
        .find(|a| a.name == ASSET_NAME)
        .and_then(|a| {
            Some(Release {
                version: rel.tag_name.trim_start_matches('v').to_owned(),
                url: a.browser_download_url.clone(),
                sha256: a.digest.as_deref()?.strip_prefix("sha256:")?.to_owned(),
            })
        });
    Ok(release)
}

/// Returns the newest release if it is newer than this build and has a hashed exe asset.
pub fn check() -> anyhow::Result<Option<Release>> {
    let mut resp = download::agent()
        .get(LATEST_URL)
        .header("Accept", "application/vnd.github+json")
        .call()?;
    if !resp.status().is_success() {
        anyhow::bail!("GitHub returned HTTP {}", resp.status());
    }
    let body = resp.body_mut().read_to_string()?;
    parse_release(&body, env!("CARGO_PKG_VERSION"))
}

fn sibling(exe: &Path, ext: &str) -> PathBuf {
    let mut name = exe.as_os_str().to_owned();
    name.push(ext);
    PathBuf::from(name)
}

/// Download, verify and swap in the new exe, then relaunch it. Only returns on failure.
pub fn apply(release: &Release, shared: &Shared) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let new = sibling(&exe, ".new");
    let old = sibling(&exe, ".old");

    download::set_message(shared, format!("Downloading installer {}", release.version));
    download::download(&release.url, &release.sha256, &new, shared)?;

    // A running exe can be renamed but not overwritten.
    let _ = std::fs::remove_file(&old);
    if let Err(e) = std::fs::rename(&exe, &old) {
        let _ = std::fs::remove_file(&new);
        return Err(e.into());
    }
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe);
        let _ = std::fs::remove_file(&new);
        return Err(e.into());
    }

    download::set_message(shared, "Restarting");
    Command::new(&exe).spawn()?;
    std::process::exit(0);
}

/// Remove the previous exe left behind by a self-update.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(sibling(&exe, ".old"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0.1", "1.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.2.0"));
        assert!(!is_newer("nightly", "0.1.0"));
    }

    #[test]
    fn needs_newer_tag_and_hashed_asset() {
        let body = |tag: &str, digest: &str| {
            format!(
                r#"{{"tag_name":"{tag}","assets":[{{"name":"vatsim-uk-installer.exe","digest":{digest},"browser_download_url":"u"}}]}}"#
            )
        };
        let r = parse_release(&body("v0.2.0", r#""sha256:ab""#), "0.1.0").unwrap();
        assert_eq!(
            r,
            Some(Release {
                version: "0.2.0".into(),
                url: "u".into(),
                sha256: "ab".into()
            })
        );
        assert!(parse_release(&body("v0.1.0", r#""sha256:ab""#), "0.1.0").unwrap().is_none());
        assert!(parse_release(&body("v0.2.0", "null"), "0.1.0").unwrap().is_none());
    }
}
