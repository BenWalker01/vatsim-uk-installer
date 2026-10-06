//! Describes what the installer can fetch: EuroScope/VC++ info plus the controller pack releases.

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// GitHub releases API for the controller pack. Tags look like `2026_10` or `2026_09a`.
pub const RELEASES_URL: &str =
    "https://api.github.com/repos/VATSIM-UK/uk-controller-pack/releases?per_page=100";
pub const EUROSCOPE_DOWNLOAD_URL: &str = "https://euroscope.hu/install/EuroScopeSetup.3.2.3.2.msi";

/// Pack release tag: AIRAC year and cycle (`YYYY_CC`) with an optional hotfix letter (`2026_09a`).
/// Ordering is chronological: `2026_09` < `2026_09a` < `2026_09b` < `2026_10`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackVersion {
    pub year: u16,
    pub cycle: u8,
    pub suffix: Option<char>,
}

impl FromStr for PackVersion {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        let (y, rest) = s
            .split_once('_')
            .ok_or_else(|| anyhow::anyhow!("bad pack version: {s}"))?;
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        let (cycle, suffix) = rest.split_at(digits);
        let mut chars = suffix.chars();
        let suffix = chars.next();
        if chars.next().is_some() || suffix.is_some_and(|c| !c.is_ascii_lowercase()) {
            anyhow::bail!("bad pack version: {s}");
        }
        Ok(PackVersion {
            year: y.parse()?,
            cycle: cycle.parse()?,
            suffix,
        })
    }
}

impl fmt::Display for PackVersion {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}_{:02}", self.year, self.cycle)?;
        if let Some(c) = self.suffix {
            write!(f, "{c}")?;
        }
        Ok(())
    }
}

impl Serialize for PackVersion {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PackVersion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone)]
pub struct Manifest {
    pub euroscope: EuroScopeInfo,
    pub vcredist_url: String,
    /// All pack releases, sorted oldest to newest.
    pub releases: Vec<PackRelease>,
}

#[derive(Debug, Clone)]
pub struct EuroScopeInfo {
    /// Exact version required; newer versions are not supported.
    pub required_version: String,
    pub download_url: String,
}

/// One tagged release. Each has a full pack and a changes-only zip relative to the previous release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackRelease {
    pub version: PackVersion,
    pub full: Asset,
    pub changes_only: Asset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
}

impl Manifest {
    pub fn latest(&self) -> Option<&PackRelease> {
        self.releases.last()
    }

    /// Pack releases come from GitHub (cached on disk, revalidated with an ETag).
    pub fn fetch() -> anyhow::Result<Manifest> {
        let body = fetch_releases_json()?;
        let releases = parse_releases(&body)?;
        Ok(Manifest {
            euroscope: EuroScopeInfo {
                required_version: "3.2.3.2".into(),
                download_url: EUROSCOPE_DOWNLOAD_URL.into(),
            },
            vcredist_url: "https://aka.ms/vs/17/release/vc_redist.x86.exe".into(),
            releases,
        })
    }
}

fn cache_paths() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let dir = dirs::cache_dir()?.join("vatsim-uk-installer");
    std::fs::create_dir_all(&dir).ok()?;
    Some((dir.join("releases.json"), dir.join("releases.etag")))
}

fn fetch_releases_json() -> anyhow::Result<String> {
    let cache = cache_paths();
    let cached = cache
        .as_ref()
        .and_then(|(b, _)| std::fs::read_to_string(b).ok());
    let etag = cache
        .as_ref()
        .and_then(|(_, e)| std::fs::read_to_string(e).ok());

    let mut req = crate::download::agent()
        .get(RELEASES_URL)
        .header("Accept", "application/vnd.github+json");
    if let (Some(etag), Some(_)) = (&etag, &cached) {
        req = req.header("If-None-Match", etag.trim());
    }
    match req.call() {
        Ok(resp) if resp.status().as_u16() == 304 && cached.is_some() => Ok(cached.unwrap()),
        Ok(mut resp) if resp.status().is_success() => {
            let new_etag = resp
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let body = resp.body_mut().read_to_string()?;
            if let Some((b, e)) = cache {
                let _ = std::fs::write(b, &body);
                match new_etag {
                    Some(t) => {
                        let _ = std::fs::write(e, t);
                    }
                    None => {
                        let _ = std::fs::remove_file(e);
                    }
                }
            }
            Ok(body)
        }
        // Rate-limited or offline: fall back to the last good response.
        Ok(resp) => cached.ok_or_else(|| anyhow::anyhow!("GitHub returned HTTP {}", resp.status())),
        Err(e) => cached.ok_or_else(|| e.into()),
    }
}

fn parse_releases(body: &str) -> anyhow::Result<Vec<PackRelease>> {
    #[derive(Deserialize)]
    struct Rel {
        tag_name: String,
        draft: bool,
        prerelease: bool,
        assets: Vec<Ast>,
    }
    #[derive(Deserialize)]
    struct Ast {
        name: String,
        digest: Option<String>,
        browser_download_url: String,
    }

    let mut out = Vec::new();
    for rel in serde_json::from_str::<Vec<Rel>>(body)? {
        if rel.draft || rel.prerelease {
            continue;
        }
        let Ok(version) = rel.tag_name.parse::<PackVersion>() else {
            continue;
        };
        let find = |name: String| {
            rel.assets.iter().find(|a| a.name == name).and_then(|a| {
                Some(Asset {
                    url: a.browser_download_url.clone(),
                    sha256: a.digest.as_deref()?.strip_prefix("sha256:")?.to_owned(),
                })
            })
        };
        let (Some(full), Some(changes_only)) = (
            find(format!("uk_controller_pack_{}.zip", rel.tag_name)),
            find(format!("changes_only_{}.zip", rel.tag_name)),
        ) else {
            continue;
        };
        out.push(PackRelease {
            version,
            full,
            changes_only,
        });
    }
    out.sort_by_key(|r| r.version);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_orders_tags() {
        let v = |s: &str| s.parse::<PackVersion>().unwrap();
        assert!(v("2026_09") < v("2026_09a"));
        assert!(v("2026_09a") < v("2026_09b"));
        assert!(v("2026_09b") < v("2026_10"));
        assert!(v("2025_12") < v("2026_01"));
        assert_eq!(v("2026_09a").to_string(), "2026_09a");
        assert!("garbage".parse::<PackVersion>().is_err());
    }

    #[test]
    #[ignore = "hits the GitHub API"]
    fn fetches_live_releases() {
        let m = Manifest::fetch().unwrap();
        println!(
            "{} releases, latest {}",
            m.releases.len(),
            m.latest().unwrap().version
        );
        assert!(m.latest().is_some());
        // Second call exercises the ETag/304 path.
        assert_eq!(Manifest::fetch().unwrap().releases.len(), m.releases.len());
    }

    #[test]
    fn parses_release_json() {
        let json = r#"[
          {"tag_name":"2026_10","draft":false,"prerelease":false,"assets":[
            {"name":"changes_only_2026_10.zip","digest":"sha256:aa","browser_download_url":"u1"},
            {"name":"uk_controller_pack_2026_10.zip","digest":"sha256:bb","browser_download_url":"u2"}]},
          {"tag_name":"2026_09a","draft":false,"prerelease":false,"assets":[
            {"name":"changes_only_2026_09a.zip","digest":"sha256:cc","browser_download_url":"u3"},
            {"name":"uk_controller_pack_2026_09a.zip","digest":"sha256:dd","browser_download_url":"u4"}]},
          {"tag_name":"2026_08","draft":true,"prerelease":false,"assets":[]}
        ]"#;
        let r = parse_releases(json).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].version.to_string(), "2026_09a");
        assert_eq!(r[1].full.sha256, "bb");
        assert_eq!(r[1].changes_only.url, "u1");
    }
}
