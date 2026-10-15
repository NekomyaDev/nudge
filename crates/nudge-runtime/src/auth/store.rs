//! Isolated credential store for Nudge (~/.nudge/credentials.json).
//!
//! Enforces:
//! - POSIX 0600 file permissions and 0700 directory permissions.
//! - Atomic persistence via temp-file + rename.
//! - Absolute file isolation: never touches foreign configuration paths.
//! - Diagnostic error error[E-auth-0405] on corrupt or unreadable store files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::credential::Credential;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountEntry {
    pub provider: String,
    pub credential: Credential,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialStore {
    #[serde(default)]
    pub providers: HashMap<String, Credential>,
    #[serde(default)]
    pub accounts: HashMap<String, AccountEntry>,
}

impl CredentialStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve the default credential store file path:
    /// NUDGE_CREDENTIALS_PATH env var if set, otherwise ~/.nudge/credentials.json.
    pub fn default_path() -> PathBuf {
        if let Ok(p) = std::env::var("NUDGE_CREDENTIALS_PATH") {
            if !p.trim().is_empty() {
                return PathBuf::from(p);
            }
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".nudge").join("credentials.json")
    }

    /// Load the credential store from the default location.
    pub fn load_default() -> Result<Self, String> {
        let path = Self::default_path();
        Self::load_from(&path)
    }

    /// Load the credential store from a specific path.
    /// If the file does not exist, an empty store is returned.
    /// If the file is unreadable or contains invalid JSON, returns error[E-auth-0405].
    pub fn load_from(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path).map_err(|e| {
            format!(
                "error[E-auth-0405]: credential store at '{}' is corrupted, insecure, or unreadable: {e}",
                path.display()
            )
        })?;

        serde_json::from_str(&content).map_err(|e| {
            format!(
                "error[E-auth-0405]: credential store at '{}' is corrupted, insecure, or unreadable: {e}",
                path.display()
            )
        })
    }

    /// Save the credential store to the default location with mode 0600.
    pub fn save_default(&self) -> Result<(), String> {
        let path = Self::default_path();
        self.save_to(&path)
    }

    /// Save the credential store to a specific path atomically with restricted permissions.
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!("error[E-auth-0405]: failed to create credential store directory: {e}")
                })?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ =
                        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
                }
            }
        }

        let json = serde_json::to_string_pretty(self).map_err(|e| {
            format!("error[E-auth-0405]: failed to serialize credential store: {e}")
        })?;

        // Write atomically via temporary file
        let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&tmp_path, json)
            .map_err(|e| format!("error[E-auth-0405]: failed to write credentials: {e}"))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600));
        }

        std::fs::rename(&tmp_path, path).map_err(|e| {
            format!("error[E-auth-0405]: failed to persist credential store atomically: {e}")
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }

        Ok(())
    }

    pub fn get_account(&self, account_name: &str) -> Option<&AccountEntry> {
        self.accounts.get(account_name)
    }

    pub fn set_account(
        &mut self,
        account_name: impl Into<String>,
        provider: impl Into<String>,
        credential: Credential,
        scopes: Vec<String>,
    ) {
        self.accounts.insert(
            account_name.into(),
            AccountEntry {
                provider: provider.into(),
                credential,
                scopes,
            },
        );
    }

    pub fn get_provider(&self, provider_name: &str) -> Option<&Credential> {
        self.providers.get(provider_name)
    }

    pub fn set_provider(&mut self, provider_name: impl Into<String>, credential: Credential) {
        self.providers.insert(provider_name.into(), credential);
    }
}
