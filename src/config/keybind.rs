/// Scan code << 16 for a Windows virtual key, as stored in the `.prf` `AselKey` setting.
#[cfg(windows)]
pub fn asel_from_vk(vk: u32) -> Option<String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::MapVirtualKeyA;
    let scan = unsafe { MapVirtualKeyA(vk, 0) };
    (scan != 0).then(|| (scan << 16).to_string())
}

/// Human-readable name of a stored ASEL value (scan code << 16, i.e. the Win32 key-name lParam).
#[cfg(windows)]
pub fn asel_name(code: &str) -> Option<String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyNameTextA;
    let lparam: i32 = code.parse::<u32>().ok()? as i32;
    let mut buf = [0u8; 64];
    let n = unsafe { GetKeyNameTextA(lparam, buf.as_mut_ptr(), buf.len() as i32) };
    (n > 0).then(|| String::from_utf8_lossy(&buf[..n as usize]).into_owned())
}

#[cfg(not(windows))]
pub fn asel_name(_code: &str) -> Option<String> {
    None
}

/// The first virtual key currently held down (keyboard keys only), if any.
#[cfg(windows)]
pub fn pressed_vk() -> Option<u32> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    (8u32..=254)
        .filter(|vk| !matches!(vk, 0x10..=0x12 | 0x5B | 0x5C))
        .find(|&vk| (unsafe { GetAsyncKeyState(vk as i32) } as u16) & 0x8000 != 0)
}

#[cfg(not(windows))]
pub fn asel_from_vk(_vk: u32) -> Option<String> {
    None
}

#[cfg(not(windows))]
pub fn pressed_vk() -> Option<u32> {
    None
}
