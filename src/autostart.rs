#[cfg(target_os = "windows")]
use windows::core::{w, PCWSTR};
#[cfg(target_os = "windows")]
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_ALL_ACCESS, KEY_READ, REG_SZ, REG_VALUE_TYPE, RRF_RT_REG_SZ,
};

#[cfg(target_os = "windows")]
const REG_RUN_PATH: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
#[cfg(target_os = "windows")]
const APP_NAME: PCWSTR = w!("Ameath");

/// Checks if Ameath is currently set to run on startup by verifying the registry key.
pub fn is_autostart_enabled() -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        let mut data_type = REG_VALUE_TYPE::default();
        let mut data_size = 0u32;

        // Query just to check existence and size
        let res = RegGetValueW(
            HKEY_CURRENT_USER,
            REG_RUN_PATH,
            APP_NAME,
            RRF_RT_REG_SZ,
            Some(&mut data_type),
            None,
            Some(&mut data_size),
        );

        res.is_ok()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Enables or disables running Ameath on system startup by modifying the Windows Registry.
pub fn set_autostart(enable: bool) {
    #[cfg(target_os = "windows")]
    unsafe {
        if enable {
            // Get current executable path
            let current_exe = match std::env::current_exe() {
                Ok(path) => path,
                Err(e) => {
                    tracing::error!("Failed to get current executable path for autostart: {}", e);
                    return;
                }
            };

            // Format as a proper string and wrap in quotes to handle spaces in path.
            let exe_str = current_exe.to_string_lossy();
            let exe_command = format!("\"{}\"", exe_str);

            // Convert to UTF-16 for Windows API
            let mut utf16_cmd: Vec<u16> = exe_command.encode_utf16().collect();
            utf16_cmd.push(0); // Null terminator

            let mut hkey = HKEY::default();

            // Open or create the Run key
            let res = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                REG_RUN_PATH,
                0,
                None,
                windows::Win32::System::Registry::REG_OPTION_NON_VOLATILE,
                KEY_ALL_ACCESS,
                None,
                &mut hkey,
                None,
            );

            if res.is_ok() {
                let set_res = RegSetValueExW(
                    hkey,
                    APP_NAME,
                    0,
                    REG_SZ,
                    Some(std::slice::from_raw_parts(
                        utf16_cmd.as_ptr() as *const u8,
                        utf16_cmd.len() * 2,
                    )),
                );

                if set_res.is_err() {
                    tracing::error!("Failed to set autostart registry value.");
                } else {
                    tracing::info!("Auto-start enabled (Registry value set).");
                }
                let _ = RegCloseKey(hkey);
            } else {
                tracing::error!("Failed to open registry key for autostart.");
            }
        } else {
            // Disable by deleting the value
            let mut hkey = HKEY::default();
            let res = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                REG_RUN_PATH,
                0,
                None,
                windows::Win32::System::Registry::REG_OPTION_NON_VOLATILE,
                KEY_ALL_ACCESS,
                None,
                &mut hkey,
                None,
            );

            if res.is_ok() {
                let del_res = RegDeleteValueW(hkey, APP_NAME);
                if del_res.is_ok() {
                    tracing::info!("Auto-start disabled (Registry value deleted).");
                } else {
                    // Could throw if key doesn't exist, which is fine (already disabled).
                    tracing::debug!(
                        "Failed to delete autostart registry value, or it didn't exist."
                    );
                }
                let _ = RegCloseKey(hkey);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    fn test_autostart_toggle() {
        // Warning: This test modifies the actual registry, but cleans up after itself.
        let initial_state = is_autostart_enabled();

        // 1. Enable
        set_autostart(true);
        assert!(
            is_autostart_enabled(),
            "Autostart should be enabled after set_autostart(true)"
        );

        // 2. Disable
        set_autostart(false);
        assert!(
            !is_autostart_enabled(),
            "Autostart should be disabled after set_autostart(false)"
        );

        // 3. Restore to original state
        set_autostart(initial_state);
        assert_eq!(
            is_autostart_enabled(),
            initial_state,
            "Autostart state should be restored"
        );
    }
}
