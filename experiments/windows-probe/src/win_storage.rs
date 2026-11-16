//! Current-user Windows profile storage. No plaintext passwords or custom crypto.
use rustremote_core::profiles::{MAX_PROFILE_BYTES, Workspace};
use std::{
    ffi::{OsStr, c_void},
    fs::File,
    io::{Read, Write},
    mem::size_of,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::Mutex,
};
use windows_sys::Win32::{
    Foundation::{
        ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, GENERIC_READ, GENERIC_WRITE,
        INVALID_HANDLE_VALUE, LocalFree,
    },
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SE_FILE_OBJECT, SetSecurityInfo,
        },
        Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
        DACL_SECURITY_INFORMATION, EqualSid, GetSecurityDescriptorControl,
        GetSecurityDescriptorDacl, GetSecurityDescriptorOwner, GetTokenInformation,
        OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
        PSID, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CREATE_NEW, CreateDirectoryW, CreateFileW,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FILE_TYPE_DISK, GetFileInformationByHandle, GetFileType,
        MOVEFILE_WRITE_THROUGH, MoveFileExW, OPEN_ALWAYS, OPEN_EXISTING, READ_CONTROL,
        ReplaceFileW, WRITE_DAC,
    },
    System::{
        Com::{CoCreateGuid, CoTaskMemFree},
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
    UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath},
};
use zeroize::{Zeroize, Zeroizing};

const MAX_STORED: usize = MAX_PROFILE_BYTES + 65536;
const HEADER: &[u8] = b"RRW1";

pub(crate) fn wide(value: impl AsRef<OsStr>) -> Result<Vec<u16>, String> {
    let value: Vec<u16> = value.as_ref().encode_wide().collect();
    if value.contains(&0) {
        return Err("Embedded NUL in Windows path".into());
    }
    Ok(value.into_iter().chain([0]).collect())
}

fn error(step: &str) -> String {
    format!(
        "{step} (Windows error {})",
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    )
}

struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

unsafe fn bounded_wide(pointer: *const u16) -> Result<Vec<u16>, String> {
    if pointer.is_null() {
        return Err("Windows returned a null string".into());
    }
    for len in 0..32768 {
        // OS-allocated NUL-terminated strings, bounded before making a Rust slice.
        if unsafe { *pointer.add(len) } == 0 {
            return Ok(unsafe { std::slice::from_raw_parts(pointer, len) }.to_vec());
        }
    }
    Err("Windows string exceeds its budget".into())
}

pub fn new_reference() -> Result<String, String> {
    let mut guid = windows_sys::core::GUID::from_u128(0);
    if unsafe { CoCreateGuid(&mut guid) } < 0 {
        return Err("Cannot create credential reference".into());
    }
    Ok(format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7]
    ))
}

pub fn directory() -> Result<PathBuf, String> {
    let mut pointer = null_mut();
    if unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, null_mut(), &mut pointer) } < 0 {
        return Err("Windows LocalAppData is unavailable".into());
    }
    let value = unsafe { bounded_wide(pointer) };
    unsafe {
        CoTaskMemFree(pointer.cast());
    }
    let path = PathBuf::from(std::ffi::OsString::from_wide(&value?)).join("RustRemote");
    if !path.is_absolute() {
        return Err("Windows configuration directory is not absolute".into());
    }
    Ok(path)
}

struct PrivateSecurity {
    allocation: Local,
    owner: PSID,
    dacl: *mut windows_sys::Win32::Security::ACL,
}
impl PrivateSecurity {
    fn current() -> Result<Self, String> {
        let mut raw_token = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw_token) } == 0 {
            return Err(error("Open process token"));
        }
        let token = unsafe { OwnedHandle::from_raw_handle(raw_token) };
        let mut length = 0;
        unsafe {
            GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut length);
        }
        if length == 0 || length > 65536 {
            return Err("Invalid Windows token size".into());
        }
        // Word-aligned storage for TOKEN_USER and its variable-length SID.
        let mut storage = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                storage.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        } == 0
        {
            return Err(error("Read process SID"));
        }
        let user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
        let mut text = null_mut();
        if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 {
            return Err(error("Convert process SID"));
        }
        let text_allocation = Local(text.cast());
        let sid = String::from_utf16(&unsafe { bounded_wide(text)? })
            .map_err(|_| "Invalid Windows SID")?;
        drop(text_allocation);
        let sddl = wide(format!("O:{sid}D:P(A;;FA;;;SY)(A;;FA;;;{sid})"))?;
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
        {
            return Err(error("Create private security descriptor"));
        }
        let allocation = Local(descriptor);
        let (mut owner, mut dacl, mut present, mut defaulted) = (null_mut(), null_mut(), 0, 0);
        if unsafe { GetSecurityDescriptorOwner(descriptor, &mut owner, &mut defaulted) } == 0
            || unsafe {
                GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted)
            } == 0
            || owner.is_null()
            || present == 0
            || dacl.is_null()
        {
            return Err("Private Windows DACL unavailable".into());
        }
        Ok(Self {
            allocation,
            owner,
            dacl,
        })
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.allocation.0,
            bInheritHandle: 0,
        }
    }

    fn secure(&self, handle: &OwnedHandle, directory: bool) -> Result<(), String> {
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        if unsafe { GetFileInformationByHandle(handle.as_raw_handle(), &mut info) } == 0 {
            return Err(error("Read private file attributes"));
        }
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
            || (!directory && info.nNumberOfLinks != 1)
            || unsafe { GetFileType(handle.as_raw_handle()) } != FILE_TYPE_DISK
        {
            return Err("Reparse points and non-regular profile objects are refused".into());
        }
        let mut owner = null_mut();
        let mut descriptor = null_mut();
        let result = unsafe {
            GetSecurityInfo(
                handle.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                null_mut(),
                null_mut(),
                null_mut(),
                &mut descriptor,
            )
        };
        let _allocation = Local(descriptor);
        if result != 0 || owner.is_null() || unsafe { EqualSid(owner, self.owner) } == 0 {
            return Err("Profile object belongs to another Windows identity".into());
        }
        if unsafe {
            SetSecurityInfo(
                handle.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                self.dacl,
                null_mut(),
            )
        } != 0
        {
            return Err("Cannot protect the Windows profile DACL".into());
        }
        let mut descriptor = null_mut();
        let mut dacl = null_mut();
        if unsafe {
            GetSecurityInfo(
                handle.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut descriptor,
            )
        } != 0
        {
            return Err("Cannot verify private Windows DACL".into());
        }
        let _allocation = Local(descriptor);
        let mut control = 0;
        let mut revision = 0;
        if dacl.is_null()
            || unsafe { (*dacl).AceCount } != 2
            || unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
            || control & SE_DACL_PROTECTED == 0
        {
            return Err("Private Windows DACL verification failed".into());
        }
        Ok(())
    }
}

fn handle(
    path: &Path,
    access: u32,
    share: u32,
    disposition: u32,
    security: &PrivateSecurity,
) -> Result<OwnedHandle, String> {
    let path = wide(path)?;
    let attributes = security.attributes();
    let raw = unsafe {
        CreateFileW(
            path.as_ptr(),
            access | READ_CONTROL | WRITE_DAC,
            share,
            &attributes,
            disposition,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            null_mut(),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        return Err(error("Open private profile object"));
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

pub(crate) fn crypt(bytes: &[u8], protect: bool) -> Result<Zeroizing<Vec<u8>>, String> {
    if bytes.len() > MAX_STORED {
        return Err("DPAPI input exceeds profile budget".into());
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let success = unsafe {
        if protect {
            CryptProtectData(
                &input,
                null(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                null_mut(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if success == 0 {
        return Err("Windows DPAPI refused the data (wrong identity or damaged file)".into());
    }
    let _allocation = Local(output.pbData.cast());
    if output.pbData.is_null() || output.cbData as usize > MAX_STORED {
        return Err("DPAPI output exceeds profile budget".into());
    }
    let native = unsafe { std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize) };
    let result = Zeroizing::new(native.to_vec());
    native.zeroize();
    Ok(result)
}

pub struct Store {
    root: PathBuf,
    _lock: OwnedHandle,
    _directory: OwnedHandle,
    access: Mutex<()>,
}
impl Store {
    pub fn open() -> Result<Self, String> {
        Self::at(directory()?)
    }

    pub(crate) fn at(root: PathBuf) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err("Profile directory must be absolute".into());
        }
        let security = PrivateSecurity::current()?;
        let path = wide(&root)?;
        if unsafe { CreateDirectoryW(path.as_ptr(), &security.attributes()) } == 0
            && std::io::Error::last_os_error().raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32)
        {
            return Err(error("Create private profile directory"));
        }
        let dir = handle(
            &root,
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            OPEN_EXISTING,
            &security,
        )?;
        security.secure(&dir, true)?;
        let lock = handle(
            &root.join("profiles.lock"),
            GENERIC_READ | GENERIC_WRITE,
            0,
            OPEN_ALWAYS,
            &security,
        )?;
        security.secure(&lock, false)?;
        Ok(Self {
            root,
            _lock: lock,
            _directory: dir,
            access: Mutex::new(()),
        })
    }

    pub fn load(&self) -> Result<(Workspace, bool), String> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| "Profile operation interrupted")?;
        let path = self.root.join("profiles.rstore");
        let security = PrivateSecurity::current()?;
        let path_wide = wide(&path)?;
        // Distinguish an absent file from denied access; never treat unreadable data as empty.
        let raw = unsafe {
            CreateFileW(
                path_wide.as_ptr(),
                GENERIC_READ | READ_CONTROL | WRITE_DAC,
                FILE_SHARE_READ,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            return if std::io::Error::last_os_error().raw_os_error()
                == Some(ERROR_FILE_NOT_FOUND as i32)
            {
                Ok((Workspace::default(), false))
            } else {
                Err(error("Read profiles"))
            };
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        security.secure(&handle, false)?;
        let mut data = Zeroizing::new(Vec::new());
        File::from(handle)
            .take((MAX_STORED + 1) as u64)
            .read_to_end(&mut data)
            .map_err(|_| "Cannot read profile data")?;
        if data.len() > MAX_STORED || data.len() < 5 || &data[..4] != HEADER {
            return Err("Unsupported or oversized Windows profile store".into());
        }
        match data[4] {
            b'P' => Ok((Workspace::decode(&data[5..])?, false)),
            b'E' => Ok((Workspace::decode(&crypt(&data[5..], false)?)?, true)),
            _ => Err("Unsupported profile encryption mode".into()),
        }
    }

    pub fn save(&self, workspace: &Workspace, encrypted: bool) -> Result<(), String> {
        let _guard = self
            .access
            .lock()
            .map_err(|_| "Profile operation interrupted")?;
        workspace.validate()?;
        let json =
            Zeroizing::new(serde_json::to_vec(workspace).map_err(|_| "Cannot encode profiles")?);
        if json.len() > MAX_PROFILE_BYTES {
            return Err("Profiles exceed 2 MiB".into());
        }
        let payload = if encrypted { crypt(&json, true)? } else { json };
        if payload.len() + 5 > MAX_STORED {
            return Err("Stored profile exceeds its budget".into());
        }
        let security = PrivateSecurity::current()?;
        let temporary = self
            .root
            .join(format!(".profiles-{}.tmp", new_reference()?));
        let target = self.root.join("profiles.rstore");
        let backup = self
            .root
            .join(format!(".profiles-{}.backup", new_reference()?));
        let mut created = false;
        let result = (|| {
            let temporary_handle = handle(&temporary, GENERIC_WRITE, 0, CREATE_NEW, &security)?;
            created = true;
            security.secure(&temporary_handle, false)?;
            let mut file = File::from(temporary_handle);
            file.write_all(HEADER)
                .and_then(|_| file.write_all(if encrypted { b"E" } else { b"P" }))
                .and_then(|_| file.write_all(&payload))
                .and_then(|_| file.sync_all())
                .map_err(|_| "Cannot flush profile save")?;
            drop(file);
            // Refuse an existing reparse point and secure the original before metadata merge.
            let exists = match std::fs::symlink_metadata(&target) {
                Ok(_) => {
                    let h = handle(
                        &target,
                        GENERIC_READ,
                        FILE_SHARE_READ,
                        OPEN_EXISTING,
                        &security,
                    )?;
                    security.secure(&h, false)?;
                    true
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(_) => return Err("Cannot inspect existing profiles".into()),
            };
            let (target_w, temporary_w, backup_w) =
                (wide(&target)?, wide(&temporary)?, wide(&backup)?);
            let success = unsafe {
                if exists {
                    ReplaceFileW(
                        target_w.as_ptr(),
                        temporary_w.as_ptr(),
                        backup_w.as_ptr(),
                        0,
                        null(),
                        null(),
                    )
                } else {
                    MoveFileExW(
                        temporary_w.as_ptr(),
                        target_w.as_ptr(),
                        MOVEFILE_WRITE_THROUGH,
                    )
                }
            };
            if success == 0 {
                // Retain the private backup for recovery; never delete or overwrite it on failure.
                if !target.exists() && backup.exists() {
                    unsafe {
                        MoveFileExW(backup_w.as_ptr(), target_w.as_ptr(), MOVEFILE_WRITE_THROUGH);
                    }
                }
                return Err(error(
                    "Replace profiles (private backup retained if available)",
                ));
            }
            if exists {
                let _ = std::fs::remove_file(&backup);
            }
            Ok(())
        })();
        if result.is_err() && created {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
}

pub fn self_test() -> Result<String, String> {
    use windows_sys::Win32::Security::Credentials::*;
    let temp = tempfile::tempdir().map_err(|_| "Storage lab unavailable")?;
    let path = temp.path().join("Private profiles — été");
    let store = Store::at(path.clone())?;
    if Store::at(path.clone()).is_ok() {
        return Err("Profile lock did not exclude a second opener".into());
    }
    let mut data = Workspace::default();
    data.add_folder("Windows/Production")?;
    store.save(&data, false)?;
    if store.load()?.1 {
        return Err("Plain store mode mismatch".into());
    }
    store.save(&data, true)?;
    let bytes = std::fs::read(path.join("profiles.rstore")).map_err(|_| "Lab read failed")?;
    if bytes
        .windows(b"Production".len())
        .any(|s| s == b"Production")
    {
        return Err("DPAPI profile metadata leaked plaintext".into());
    }
    let (loaded, encrypted) = store.load()?;
    if loaded.folders != data.folders || !encrypted {
        return Err("DPAPI roundtrip mismatch".into());
    }
    let mut corrupt = bytes;
    *corrupt.last_mut().ok_or("Empty DPAPI output")? ^= 0x80;
    std::fs::write(path.join("profiles.rstore"), corrupt).map_err(|_| "Lab tampering failed")?;
    if store.load().is_ok() {
        return Err("Tampered DPAPI data was accepted".into());
    }
    store.save(&data, true)?;
    let link = temp.path().join("linked-profile");
    std::fs::hard_link(path.join("profiles.rstore"), &link)
        .map_err(|_| "Lab hard link unavailable")?;
    if store.load().is_ok() {
        return Err("Hard-linked profile was accepted".into());
    }
    std::fs::remove_file(&link).map_err(|_| "Lab hard-link cleanup failed")?;
    store.load()?;
    drop(store);
    let reopened = Store::at(path)?;
    reopened.load()?;
    drop(reopened);
    // A synthetic credential is scoped to a fresh GUID and deleted before exit.
    let reference = new_reference()?;
    let target = wide(format!("com.rustremote.windows.lab:{reference}"))?;
    let mut payload = Zeroizing::new(new_reference()?.into_bytes());
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_ptr().cast_mut(),
        CredentialBlobSize: payload.len() as u32,
        CredentialBlob: payload.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    if unsafe { CredWriteW(&credential, 0) } == 0 {
        return Err(error("Credential Manager lab write"));
    }
    let mut pointer = null_mut();
    let read = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut pointer) };
    let mut matches = false;
    if read != 0 && !pointer.is_null() {
        let credential = unsafe { &*pointer };
        if !credential.CredentialBlob.is_null()
            && credential.CredentialBlobSize as usize == payload.len()
        {
            let blob = unsafe {
                std::slice::from_raw_parts_mut(
                    credential.CredentialBlob,
                    credential.CredentialBlobSize as usize,
                )
            };
            matches = blob == payload.as_slice();
            blob.zeroize();
        }
        unsafe {
            CredFree(pointer.cast());
        }
    }
    let deleted = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if !matches || deleted == 0 {
        return Err("Credential Manager lab roundtrip/cleanup failed".into());
    }
    Ok("WINDOWS_STORAGE_OK: user-owned protected ACL, exclusive lock, Unicode paths, plain/DPAPI save, tamper/hard-link rejection, reopen, Credential Manager roundtrip/cleanup".into())
}

/// New transfer files/directories use the same protected current-user ACL policy.
pub(crate) fn private_file(path: &Path) -> Result<File, String> {
    let security = PrivateSecurity::current()?;
    let file = handle(path, GENERIC_READ | GENERIC_WRITE, 0, CREATE_NEW, &security)?;
    security.secure(&file, false)?;
    Ok(File::from(file))
}
pub(crate) fn private_directory(path: &Path) -> Result<(), String> {
    let security = PrivateSecurity::current()?;
    let name = wide(path)?;
    if unsafe { CreateDirectoryW(name.as_ptr(), &security.attributes()) } == 0 {
        return Err(error("Create private transfer directory"));
    }
    let folder = handle(
        path,
        GENERIC_READ,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        OPEN_EXISTING,
        &security,
    )?;
    security.secure(&folder, true)
}

/// Validate, never rewrite, the ACL of a user-selected private key.
pub(crate) fn validate_key_acl(file: &File) -> Result<(), String> {
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, GetAce, IsValidSid, IsWellKnownSid, WinBuiltinAdministratorsSid,
        WinLocalSystemSid,
    };
    let security = PrivateSecurity::current()?;
    let (mut owner, mut dacl, mut descriptor) = (null_mut(), null_mut(), null_mut());
    if unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    } != 0
    {
        return Err("Cannot verify private-key ACL".into());
    }
    let _allocation = Local(descriptor);
    let trusted = |sid: PSID| unsafe {
        !sid.is_null()
            && IsValidSid(sid) != 0
            && (EqualSid(sid, security.owner) != 0
                || IsWellKnownSid(sid, WinLocalSystemSid) != 0
                || IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0)
    };
    if !trusted(owner) || dacl.is_null() {
        return Err("Private key owner or DACL is unsafe".into());
    }
    let count = unsafe { (*dacl).AceCount };
    if count > 64 {
        return Err("Private-key ACL exceeds its budget".into());
    }
    for index in 0..u32::from(count) {
        let mut pointer = null_mut();
        if unsafe { GetAce(dacl, index, &mut pointer) } == 0 || pointer.is_null() {
            return Err("Invalid private-key ACE".into());
        }
        let header = unsafe { &*pointer.cast::<windows_sys::Win32::Security::ACE_HEADER>() };
        if header.AceType == 1 {
            continue;
        } // Deny entries cannot grant another identity access.
        if header.AceType != 0
            || usize::from(header.AceSize) < std::mem::offset_of!(ACCESS_ALLOWED_ACE, SidStart) + 8
        {
            return Err("Unsupported private-key access rule".into());
        }
        let ace = unsafe { &*pointer.cast::<ACCESS_ALLOWED_ACE>() };
        let sid = std::ptr::addr_of!(ace.SidStart).cast_mut().cast();
        if !trusted(sid) {
            return Err(
                "Private key is accessible to another identity; correct its ACL before connecting"
                    .into(),
            );
        }
    }
    Ok(())
}
