//! Secure token storage backed by the Windows Credential Manager.
//!
//! The refresh token is the only long-lived secret. It is stored as a generic
//! credential under `TargetName = "Sonic"`, so Windows protects it with DPAPI
//! scoped to the current user and nothing lands in a plaintext file.
//!
//! If the Credential Manager refuses the write (locked-down policy, unusual
//! service account), we fall back to a DPAPI-encrypted file in the app data dir
//! rather than ever writing plaintext.

use crate::error::{AppError, AppResult};
use std::path::PathBuf;

const TARGET_NAME: &str = "Sonic";
const SERVICE: &str = "Sonic";

/// Token material that survives restarts.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StoredTokens {
    #[serde(default)]
    pub refresh_token: String,
    /// Spotify user id, shown in Settings.
    #[serde(default)]
    pub account_id: String,
    /// Human-readable profile name.
    #[serde(default)]
    pub display_name: String,
    /// Profile image URL, if Spotify returned one.
    #[serde(default)]
    pub avatar_url: String,
    /// `"premium"` or `"free"`.
    #[serde(default)]
    pub product: String,
    /// Issued-at epoch seconds, used to decide when to refresh proactively.
    #[serde(default)]
    pub obtained_at: u64,
}

impl StoredTokens {
    pub fn is_empty(&self) -> bool {
        self.refresh_token.is_empty()
    }
}

/// Persists tokens, preferring the Credential Manager.
pub fn save(tokens: &StoredTokens) -> AppResult<()> {
    let payload = serde_json::to_vec(tokens)
        .map_err(|e| AppError::Local(format!("could not encode tokens: {e}")))?;

    if save_credential(&payload).is_ok() {
        // One copy only: drop any DPAPI fallback from an earlier run.
        let _ = std::fs::remove_file(fallback_path());
        return Ok(());
    }

    save_fallback(&payload)
}

/// Loads tokens from whichever store holds them.
pub fn load() -> AppResult<StoredTokens> {
    if let Ok(bytes) = load_credential() {
        if let Ok(tokens) = serde_json::from_slice::<StoredTokens>(&bytes) {
            if !tokens.is_empty() {
                return Ok(tokens);
            }
        }
    }

    if let Ok(bytes) = load_fallback() {
        if let Ok(tokens) = serde_json::from_slice::<StoredTokens>(&bytes) {
            return Ok(tokens);
        }
    }

    Ok(StoredTokens::default())
}

/// Removes every stored copy.
pub fn clear() -> AppResult<()> {
    let _ = delete_credential();
    let _ = std::fs::remove_file(fallback_path());
    Ok(())
}

fn fallback_path() -> PathBuf {
    crate::config::app_dir().join("tokens.bin")
}

// ---------------------------------------------------------------------------
// Windows Credential Manager
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn save_credential(payload: &[u8]) -> AppResult<()> {
    use windows_sys::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    let target = to_wide(TARGET_NAME);
    let username = to_wide(&format!("{SERVICE} token"));
    let comment = to_wide("Spotify refresh token for Sonic");
    let mut blob = payload.to_vec();

    let credential = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_ptr() as *mut u16,
        Comment: comment.as_ptr() as *mut u16,
        // SAFETY: FILETIME is plain-old-data (two u32s); zero is a valid value.
        LastWritten: unsafe { std::mem::zeroed() },
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: std::ptr::null_mut(),
        UserName: username.as_ptr() as *mut u16,
    };

    // SAFETY: every pointer in `credential` references a buffer that outlives the
    // call, and CredentialBlobSize matches the allocation exactly.
    let ok = unsafe { CredWriteW(&credential, 0) };
    if ok == 0 {
        let err = std::io::Error::last_os_error();
        return Err(AppError::Local(format!(
            "Windows Credential Manager refused the token write: {err}"
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn load_credential() -> AppResult<Vec<u8>> {
    use std::ffi::c_void;
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let target = to_wide(TARGET_NAME);
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();

    // SAFETY: `target` is a NUL-terminated wide string valid for the whole call,
    // and `credential` is a valid out-parameter.
    let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if ok == 0 || credential.is_null() {
        return Err(AppError::Local("no token in Credential Manager".into()));
    }

    let mut result = Vec::new();
    // SAFETY: CredReadW returned a valid CREDENTIALW, which we free afterwards.
    unsafe {
        let blob = (*credential).CredentialBlob;
        let size = (*credential).CredentialBlobSize as usize;
        if !blob.is_null() && size > 0 {
            result = std::slice::from_raw_parts(blob, size).to_vec();
        }
        CredFree(credential as *const c_void);
    }

    if result.is_empty() {
        return Err(AppError::Local("stored token was empty".into()));
    }
    Ok(result)
}

#[cfg(windows)]
fn delete_credential() -> AppResult<()> {
    use windows_sys::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target = to_wide(TARGET_NAME);
    // SAFETY: `target` is a NUL-terminated wide string.
    let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if ok == 0 {
        // A missing credential is not a failure during logout.
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() != Some(-2146893043) {
            return Err(AppError::Local(format!(
                "could not clear the stored token: {err}"
            )));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn save_credential(_payload: &[u8]) -> AppResult<()> {
    Err(AppError::Local("credential store unavailable".into()))
}
#[cfg(not(windows))]
fn load_credential() -> AppResult<Vec<u8>> {
    Err(AppError::Local("credential store unavailable".into()))
}
#[cfg(not(windows))]
fn delete_credential() -> AppResult<()> {
    Err(AppError::Local("credential store unavailable".into()))
}

// ---------------------------------------------------------------------------
// DPAPI fallback
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn save_fallback(payload: &[u8]) -> AppResult<()> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: payload.len() as u32,
        pbData: payload.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let _ = &mut input;

    // SAFETY: `input` describes a live buffer; `output` receives a LocalAlloc'd
    // blob that we copy out and then free.
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        let err = std::io::Error::last_os_error();
        return Err(AppError::Local(format!(
            "could not encrypt the Spotify token: {err}"
        )));
    }

    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(output.pbData as *mut core::ffi::c_void) };

    std::fs::write(fallback_path(), bytes)
        .map_err(|e| AppError::Local(format!("could not write the token file: {e}")))
}

#[cfg(windows)]
fn load_fallback() -> AppResult<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

    let bytes = std::fs::read(fallback_path())
        .map_err(|e| AppError::Local(format!("could not read the token file: {e}")))?;

    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();

    // SAFETY: as in save_fallback; the output blob is copied then freed.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(AppError::Local(
            "could not decrypt the stored Spotify token".into(),
        ));
    }

    let slice =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(output.pbData as *mut core::ffi::c_void) };
    Ok(slice)
}

#[cfg(not(windows))]
fn save_fallback(_payload: &[u8]) -> AppResult<()> {
    Err(AppError::Local("token storage unavailable".into()))
}
#[cfg(not(windows))]
fn load_fallback() -> AppResult<Vec<u8>> {
    Err(AppError::Local("token storage unavailable".into()))
}
