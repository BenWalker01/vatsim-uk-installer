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
    #[cfg(windows)]
    fn reads_file_version() {
        let v = super::file_version(std::path::Path::new(r"C:\Windows\System32\notepad.exe"));
        assert!(v.is_some_and(|v| v.starts_with("10.")));
    }

    #[test]
    fn version_compare() {
        assert!(meets_minimum("3.2.3.2", "3.2.3"));
        assert!(!meets_minimum("3.2.1", "3.2.3"));
    }
}
