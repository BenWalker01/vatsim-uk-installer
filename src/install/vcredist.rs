//! Visual C++ Redistributable detection and install.

use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

use crate::common::download::{self, Shared};

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
    let installer =
        download::download_to_temp(url, EXPECTED_SHA256, "vatsim-uk-vcredist", "exe", shared)?;

    download::set_message(shared, "Installing Visual C++ Redistributable (x86)");
    let status = Command::new(installer.path())
        .args(["/install", "/quiet", "/norestart"])
        .status()?;
    match status.code() {
        Some(0 | 3010) => Ok(()),
        _ => anyhow::bail!("Visual C++ Redistributable installer exited with {status}"),
    }
}
