pub const FALLBACK_MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

#[cfg(not(windows))]
pub fn max_file_bytes(_root: &std::path::Path) -> Result<u64, String> {
    Ok(u64::MAX)
}

#[cfg(windows)]
pub use self::windows::max_file_bytes;

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
    };
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW,
    };

    use super::FALLBACK_MAX_FILE_BYTES;

    const DRIVE_FIXED: u32 = 3;
    const BIT_BUCKET: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket";
    const EXPLORER_POLICIES: &str = r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer";

    fn wide(text: &OsStr) -> Vec<u16> {
        text.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn from_wide(buffer: &[u16]) -> String {
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    fn volume_root(path: &Path) -> Option<Vec<u16>> {
        let path = wide(path.as_os_str());
        let mut buffer = vec![0u16; 1024];
        let ok =
            unsafe { GetVolumePathNameW(path.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) };
        (ok != 0).then(|| {
            let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            buffer.truncate(end);
            buffer.push(0);
            buffer
        })
    }

    fn volume_guid(root: &[u16]) -> Option<String> {
        let mut buffer = vec![0u16; 64];
        let ok = unsafe {
            GetVolumeNameForVolumeMountPointW(
                root.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
            )
        };
        if ok == 0 {
            return None;
        }
        let name = from_wide(&buffer);
        let start = name.find('{')?;
        let end = name.find('}')?;
        name.get(start..=end).map(str::to_owned)
    }

    fn dword(key: &str, value: &str) -> Option<u32> {
        dword_in(HKEY_CURRENT_USER, key, value)
    }

    fn dword_in(hive: HKEY, key: &str, value: &str) -> Option<u32> {
        let key = wide(OsStr::new(key));
        let value = wide(OsStr::new(value));
        let mut data = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            RegGetValueW(
                hive,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut data as *mut u32).cast(),
                &mut size,
            )
        };
        (status == 0).then_some(data)
    }

    pub fn max_file_bytes(root: &Path) -> Result<u64, String> {
        let volume = volume_root(root).ok_or("cannot find the drive of this folder")?;
        if unsafe { GetDriveTypeW(volume.as_ptr()) } != DRIVE_FIXED {
            return Err("this drive has no Recycle Bin (removable or network drive)".into());
        }
        let guid = volume_guid(&volume).ok_or("cannot identify this drive")?;
        let key = format!(r"{BIT_BUCKET}\Volume\{guid}");
        let policy_off = [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
            .into_iter()
            .any(|hive| dword_in(hive, EXPLORER_POLICIES, "NoRecycleFiles") == Some(1));
        if policy_off
            || dword(&key, "NukeOnDelete") == Some(1)
            || dword(BIT_BUCKET, "NukeOnDelete") == Some(1)
        {
            return Err("the Recycle Bin is turned off for this drive".into());
        }
        Ok(dword(&key, "MaxCapacity")
            .map(|megabytes| u64::from(megabytes) * 1024 * 1024)
            .unwrap_or(FALLBACK_MAX_FILE_BYTES))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_temp_folder_has_a_recycle_bin() {
            let dir = std::env::temp_dir();
            assert!(max_file_bytes(&dir).is_ok_and(|limit| limit > 0));
        }
    }
}
