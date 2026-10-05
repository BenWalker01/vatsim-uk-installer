//! EuroScope detection, version check and install.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::download::{self, Shared};

pub const SETUP_GUIDE_URL: &str =
    "https://docs.vatsim.uk/General/Use%20of%20Software/EuroScope%20Setup%20Guide/";

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
        version: file_version(&path),
        path,
    })
}

/// Reads the `FileVersion` string resource (English/Unicode block) from an exe.
#[cfg(windows)]
pub fn file_version(path: &std::path::Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

    unsafe {
        let size = GetFileVersionInfoSizeW(wide.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(wide.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0 {
            return None;
        }

        let query = |sub: &str| -> Option<(*mut core::ffi::c_void, u32)> {
            let q: Vec<u16> = sub.encode_utf16().chain(Some(0)).collect();
            let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
            let mut len: u32 = 0;
            let ok = VerQueryValueW(data.as_ptr().cast(), q.as_ptr(), &mut ptr, &mut len);
            (ok != 0 && !ptr.is_null() && len > 0).then_some((ptr, len))
        };

        // First (language, codepage) pair, else US English / Unicode.
        let block = match query("\\VarFileInfo\\Translation") {
            Some((p, len)) if len >= 4 => {
                let pair = std::slice::from_raw_parts(p as *const u16, 2);
                format!("{:04x}{:04x}", pair[0], pair[1])
            }
            _ => "040904b0".to_string(),
        };

        let (ptr, len) = query(&format!("\\StringFileInfo\\{block}\\FileVersion"))?;
        let chars = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let s = String::from_utf16_lossy(chars);
        Some(s.trim_end_matches('\0').trim().replace(", ", "."))
    }
}

#[cfg(not(windows))]
pub fn file_version(_path: &std::path::Path) -> Option<String> {
    None
}

/// EuroScope must be exactly the required version: newer releases are not supported.
/// Trailing zero components are ignored, so "3.2.3" == "3.2.3.0".
pub fn is_required_version(found: &str, required: &str) -> bool {
    let parse = |s: &str| s.split('.').map(|p| p.trim().parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    let (mut a, mut b) = (parse(found), parse(required));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a == b
}

/// Download the MSI and run it with the interactive Windows Installer UI.
pub fn install(url: &str, shared: &Shared) -> anyhow::Result<()> {
    download::set_message(shared, "Downloading EuroScope installer");
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
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])?;
        shared.lock().unwrap().done += n as u64;
    }
    file.flush()?;
    drop(file);

    download::set_message(shared, "Running EuroScope installer");
    let status = Command::new("msiexec.exe").arg("/i").arg(&path).status()?;
    if !status.success() {
        anyhow::bail!("EuroScope installer exited with {status}");
    }
    Ok(())
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
            "vatsim-uk-euroscope-{}-{id}.msi",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!("could not create a temporary EuroScope installer file")
}

#[cfg(test)]
mod tests {
    use super::is_required_version;

    #[test]
    #[cfg(windows)]
    fn reads_file_version() {
        let v = super::file_version(std::path::Path::new(r"C:\Windows\System32\notepad.exe"));
        assert!(v.is_some_and(|v| v.starts_with("10.")));
    }

    #[test]
    fn version_compare() {
        assert!(is_required_version("3.2.3.2", "3.2.3.2"));
        assert!(is_required_version("3.2.3", "3.2.3.0"));
        assert!(!is_required_version("3.2.13", "3.2.3.2"));
        assert!(!is_required_version("3.2.1", "3.2.3.2"));
    }
}
