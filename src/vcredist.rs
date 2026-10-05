//! Visual C++ Redistributable detection and install.

use std::process::Command;

/// Checks the registry for the x86 VC++ 2015-2022 runtime (EuroScope is 32-bit).
pub fn is_installed() -> bool {
    Command::new("reg")
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

/// TODO: download `url` and run it with `/install /quiet /norestart`.
pub fn install(_url: &str) -> anyhow::Result<()> {
    anyhow::bail!("VC++ redist install not implemented")
}
