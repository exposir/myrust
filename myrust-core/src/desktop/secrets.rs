//! Platform secret storage backed by the operating system credential store.
//!
//! macOS uses Keychain, Windows uses Credential Manager, and Linux uses the
//! D-Bus Secret Service. There is no in-memory fallback: a missing or locked
//! backend returns an error instead of silently losing persistence.

use std::sync::{Arc, Mutex};

use thiserror::Error;

/// Native secure-storage backend selected for the current platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretBackend {
    /// macOS Keychain.
    MacOsKeychain,
    /// Windows Credential Manager.
    WindowsCredentialManager,
    /// Linux Secret Service over D-Bus.
    LinuxSecretService,
    /// No persistent secure-storage backend is implemented.
    Unsupported,
}

/// Error returned by [`SecretStore`] operations.
#[derive(Debug, Error)]
pub enum SecretError {
    /// A service, account, or target name is empty or otherwise invalid.
    #[error("invalid secret-store name {field}: {reason}")]
    InvalidName {
        /// Invalid field.
        field: &'static str,
        /// Validation failure.
        reason: &'static str,
    },
    /// The current platform has no secure-storage backend.
    #[error("secure secret storage is not implemented on this platform")]
    UnsupportedPlatform,
    /// Serialized access to a store was interrupted by a poisoned lock.
    #[error("secret-store operation lock is poisoned")]
    LockPoisoned,
    /// The platform credential backend failed.
    #[error(transparent)]
    Backend(#[from] keyring::Error),
}

/// A namespaced native credential store.
///
/// Entries are identified by `(service, account, optional target)`. Operations
/// are serialized within a store because some platform Secret Service backends
/// do not tolerate concurrent requests on the same credential.
#[derive(Debug, Clone)]
pub struct SecretStore {
    service: String,
    target: Option<String>,
    operation_lock: Arc<Mutex<()>>,
}

impl SecretStore {
    /// Creates a store for an application service name.
    pub fn new(service: impl Into<String>) -> Result<Self, SecretError> {
        let service = service.into();
        validate_name("service", &service)?;
        ensure_supported()?;
        Ok(Self {
            service,
            target: None,
            operation_lock: Arc::new(Mutex::new(())),
        })
    }

    /// Creates a store with an additional backend target discriminator.
    pub fn with_target(
        service: impl Into<String>,
        target: impl Into<String>,
    ) -> Result<Self, SecretError> {
        let service = service.into();
        let target = target.into();
        validate_name("service", &service)?;
        validate_name("target", &target)?;
        ensure_supported()?;
        Ok(Self {
            service,
            target: Some(target),
            operation_lock: Arc::new(Mutex::new(())),
        })
    }

    /// Returns the native backend selected for this build.
    pub fn backend(&self) -> SecretBackend {
        secret_backend()
    }

    /// Stores or replaces a binary secret for an account.
    pub fn save(&self, account: &str, secret: &[u8]) -> Result<(), SecretError> {
        validate_name("account", account)?;
        let _guard = self
            .operation_lock
            .lock()
            .map_err(|_| SecretError::LockPoisoned)?;
        self.entry(account)?.set_secret(secret)?;
        Ok(())
    }

    /// Loads a binary secret, returning `None` when the account has no entry.
    pub fn load(&self, account: &str) -> Result<Option<Vec<u8>>, SecretError> {
        validate_name("account", account)?;
        let _guard = self
            .operation_lock
            .lock()
            .map_err(|_| SecretError::LockPoisoned)?;
        match self.entry(account)?.get_secret() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Deletes an account secret, returning whether an entry existed.
    pub fn delete(&self, account: &str) -> Result<bool, SecretError> {
        validate_name("account", account)?;
        let _guard = self
            .operation_lock
            .lock()
            .map_err(|_| SecretError::LockPoisoned)?;
        match self.entry(account)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry, SecretError> {
        match &self.target {
            Some(target) => Ok(keyring::Entry::new_with_target(
                target,
                &self.service,
                account,
            )?),
            None => Ok(keyring::Entry::new(&self.service, account)?),
        }
    }
}

/// Returns the native secure-storage backend selected for this build.
pub const fn secret_backend() -> SecretBackend {
    #[cfg(target_os = "macos")]
    {
        SecretBackend::MacOsKeychain
    }
    #[cfg(target_os = "windows")]
    {
        SecretBackend::WindowsCredentialManager
    }
    #[cfg(target_os = "linux")]
    {
        SecretBackend::LinuxSecretService
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        SecretBackend::Unsupported
    }
}

fn validate_name(field: &'static str, value: &str) -> Result<(), SecretError> {
    if value.trim().is_empty() {
        return Err(SecretError::InvalidName {
            field,
            reason: "must not be empty",
        });
    }
    if value.len() > 255 {
        return Err(SecretError::InvalidName {
            field,
            reason: "must not exceed 255 bytes",
        });
    }
    if value.chars().any(char::is_control) {
        return Err(SecretError::InvalidName {
            field,
            reason: "must not contain control characters",
        });
    }
    Ok(())
}

fn ensure_supported() -> Result<(), SecretError> {
    if secret_backend() == SecretBackend::Unsupported {
        Err(SecretError::UnsupportedPlatform)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_validated_before_backend_access() {
        let empty = validate_name("service", "").expect_err("empty service");
        assert!(matches!(
            empty,
            SecretError::InvalidName {
                field: "service",
                ..
            }
        ));
        assert!(validate_name("account", &"x".repeat(256)).is_err());
        assert!(validate_name("target", "line\nbreak").is_err());
        assert!(validate_name("account", "default").is_ok());
    }

    #[test]
    fn backend_selection_is_explicit() {
        #[cfg(target_os = "macos")]
        assert_eq!(secret_backend(), SecretBackend::MacOsKeychain);
        #[cfg(target_os = "windows")]
        assert_eq!(secret_backend(), SecretBackend::WindowsCredentialManager);
        #[cfg(target_os = "linux")]
        assert_eq!(secret_backend(), SecretBackend::LinuxSecretService);
    }

    #[test]
    fn store_construction_validates_without_touching_the_backend() {
        assert!(SecretStore::new("").is_err());
        assert!(SecretStore::with_target("app", "").is_err());
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let store = SecretStore::new("myrust-test").expect("valid store");
            assert_ne!(store.backend(), SecretBackend::Unsupported);
        }
    }
}
