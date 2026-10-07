//! Visual C++ Redistributable detection and install.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use sha2::{Digest, Sha256};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

use crate::download::{self, Shared};

pub const DOWNLOADS_URL: &str = "https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170#latest-microsoft-visual-c-redistributable-version";
// SHA-256 of the x86 redistributable served by Microsoft's latest-version URL on 2026-10-06.
const EXPECTED_SHA256: &str = "0c09f2611660441084ce0df425c51c11e147e6447963c3690f97e0b25c55ed64";

/// Checks the registry for the x86 VC++ 2015-2022 runtime (EuroScope is 32-bit).
pub fn is_installed() -> bool {
    let mut command = Command::new("reg");
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .args([
            "query",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x86",
            "/v",
            "Installed",
        ])
        .output()
        .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("0x1"))
        .unwrap_or(false)
}

/// Download the x86 redistributable and run it silently.
pub fn install(url: &str, shared: &Shared) -> anyhow::Result<()> {
    download::set_message(shared, "Downloading Visual C++ Redistributable (x86)");
    let response = download::agent().get(url).call()?;
    if !response.status().is_success() {
        anyhow::bail!("download failed: HTTP {} for {url}", response.status());
    }

    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    shared.lock().unwrap().total = total;

    let (path, mut file) = create_temp_installer()?;
    let _cleanup = TempInstaller(path.clone());
    let mut reader = response.into_body().into_reader();
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        file.write_all(&buffer[..n])?;
        shared.lock().unwrap().done += n as u64;
    }
    file.flush()?;
    drop(file);

    let actual = hex::encode(hasher.finalize());
    if actual != EXPECTED_SHA256 {
        anyhow::bail!(
            "Visual C++ Redistributable checksum mismatch: expected {EXPECTED_SHA256}, got {actual}"
        );
    }

    download::set_message(shared, "Installing Visual C++ Redistributable (x86)");
    let status = Command::new(&path)
        .args(["/install", "/quiet", "/norestart"])
        .status()?;
    match status.code() {
        Some(0 | 3010) => Ok(()),
        _ => anyhow::bail!("Visual C++ Redistributable installer exited with {status}"),
    }
}

struct TempInstaller(PathBuf);

impl Drop for TempInstaller {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn create_temp_installer() -> anyhow::Result<(PathBuf, std::fs::File)> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir();
    for _ in 0..100 {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(
            "vatsim-uk-vcredist-{}-{id}.exe",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!("could not create a temporary VC++ redistributable installer file")
}
