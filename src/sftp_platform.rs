//! Local filesystem boundary for the shared SFTP backend. Remote paths stay POSIX.
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io,
    path::Path,
};

pub fn metadata(path: &Path) -> io::Result<Metadata> {
    #[cfg(windows)]
    validate_local(path)?;
    fs::symlink_metadata(path)
}
pub fn read(path: &Path, append: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).append(append);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        validate_local(path)?;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(io::Error::other("Regular local file required"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other("Reparse points are refused"));
        }
    }
    Ok(file)
}
pub fn create(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
    }
    #[cfg(windows)]
    {
        validate_local(path)?;
        crate::win_storage::private_file(path).map_err(io::Error::other)
    }
}
pub fn mkdir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(windows)]
    {
        validate_local(path)?;
        crate::win_storage::private_directory(path).map_err(io::Error::other)
    }
}
pub fn publish(from: &Path, to: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let from = std::ffi::CString::new(from.as_os_str().as_bytes())
            .map_err(|_| "Invalid local path")?;
        let to =
            std::ffi::CString::new(to.as_os_str().as_bytes()).map_err(|_| "Invalid local path")?;
        if unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } != 0
        {
            return Err(
                "Destination existante ou publication atomique impossible ; aucun écrasement"
                    .into(),
            );
        }
    }
    #[cfg(windows)]
    {
        validate_local(from).map_err(|_| "Unsafe local source")?;
        validate_local(to).map_err(|_| "Unsafe local destination")?;
        let from = crate::win_storage::wide(from)?;
        let to = crate::win_storage::wide(to)?;
        // No REPLACE_EXISTING: file and directory collisions remain untouched.
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(
                "Destination exists or atomic publication failed; nothing overwritten".into(),
            );
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    return Err("Atomic publication is unsupported on this platform".into());
    Ok(())
}
#[cfg(windows)]
pub fn validate_local(path: &Path) -> io::Result<()> {
    use std::{os::windows::fs::MetadataExt, path::Component};
    if !path.is_absolute() {
        return Err(io::Error::other("Absolute local path required"));
    }
    for component in path.components() {
        if let Component::Normal(value) = component {
            let value = value
                .to_str()
                .ok_or_else(|| io::Error::other("Unicode local path required"))?;
            let stem = value.split('.').next().unwrap_or("").to_ascii_uppercase();
            let reserved = matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            ) || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.len() == 4
                && matches!(stem.as_bytes()[3], b'1'..=b'9'));
            if reserved
                || value.ends_with(['.', ' '])
                || value
                    .chars()
                    .any(|c| c.is_control() || "<>:\"|?*".contains(c))
            {
                return Err(io::Error::other(
                    "Windows devices, streams and aliased names are refused",
                ));
            }
        }
        if component == Component::ParentDir {
            return Err(io::Error::other("Local parent traversal refused"));
        }
    }
    for parent in path.ancestors() {
        match fs::symlink_metadata(parent) {
            Ok(meta) if meta.file_attributes() & 0x400 != 0 => {
                return Err(io::Error::other("Local reparse points are refused"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
