//! HTTP downloads with progress reporting and SHA-256 verification.

use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

const USER_AGENT: &str = concat!("vatsim-uk-installer/", env!("CARGO_PKG_VERSION"));

/// Progress shared between a worker thread and the UI.
#[derive(Default, Clone)]
pub struct Report {
    pub message: String,
    pub done: u64,
    pub total: Option<u64>,
}

pub type Shared = Arc<Mutex<Report>>;

pub fn set_message(shared: &Shared, message: impl Into<String>) {
    let mut r = shared.lock().unwrap();
    r.message = message.into();
    r.done = 0;
    r.total = None;
}

pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(USER_AGENT)
        .timeout_global(Some(Duration::from_secs(600)))
        .timeout_connect(Some(Duration::from_secs(10)))
        // Callers inspect the status themselves (e.g. 304 Not Modified).
        .http_status_as_error(false)
        .build()
        .into()
}

/// A downloaded file in the temp directory, deleted when dropped.
pub struct TempDownload(PathBuf);

impl TempDownload {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDownload {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Download `url` to a uniquely named temp file called `<prefix>-<unique>.<extension>`.
pub fn download_to_temp(
    url: &str,
    sha256: &str,
    prefix: &str,
    extension: &str,
    shared: &Shared,
) -> anyhow::Result<TempDownload> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("{prefix}-{}-{id}.{extension}", std::process::id()));
    let temp = TempDownload(path);
    download(url, sha256, temp.path(), shared)?;
    Ok(temp)
}

/// Download `url` to `dest`, verifying `sha256` (hex, optionally `sha256:`-prefixed).
pub fn download(url: &str, sha256: &str, dest: &Path, shared: &Shared) -> anyhow::Result<()> {
    let resp = agent().get(url).call()?;
    if !resp.status().is_success() {
        anyhow::bail!("download failed: HTTP {} for {url}", resp.status());
    }
    let total = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    shared.lock().unwrap().total = total;

    let mut reader = resp.into_body().into_reader();
    let mut file = std::fs::File::create(dest)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        done += n as u64;
        shared.lock().unwrap().done = done;
    }
    file.flush()?;

    let actual = hex::encode(hasher.finalize());
    let expected = sha256.trim_start_matches("sha256:");
    if !actual.eq_ignore_ascii_case(expected) {
        let _ = std::fs::remove_file(dest);
        anyhow::bail!("checksum mismatch for {url}: expected {expected}, got {actual}");
    }
    Ok(())
}
