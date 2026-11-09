//! Deep-link registration and URL delivery.
//!
//! Registration records schemes and validates incoming URLs in-process.
//! OS-level scheme registration requires macOS bundle metadata or a Windows
//! installer registration and is deferred to Phase 4. On Linux, call
//! [`crate::AppContext::deliver_open_url`] from tests or an external launcher.

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use anyhow::{bail, Result};

/// In-process deep-link scheme registry.
#[derive(Clone, Default)]
pub struct DeepLinkRegistry {
    schemes: Arc<Mutex<HashSet<String>>>,
}

impl DeepLinkRegistry {
    /// Creates an empty deep-link registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a custom URL scheme such as `myapp`.
    ///
    /// This records the scheme for validation and handler dispatch only; it
    /// does not register the scheme with macOS Launch Services or the Windows
    /// registry. Those require packaged app/installer metadata and are deferred
    /// to Phase 4. Linux does not receive an OS-level scheme handler either;
    /// call [`crate::AppContext::deliver_open_url`] manually when testing.
    pub fn register(&self, scheme: impl Into<String>) -> Result<()> {
        let scheme = normalize_scheme(&scheme.into())?;
        self.schemes
            .lock()
            .expect("deep-link registry lock poisoned")
            .insert(scheme);
        Ok(())
    }

    /// Returns whether a scheme has been registered in this process.
    pub fn contains(&self, scheme: &str) -> bool {
        self.schemes
            .lock()
            .expect("deep-link registry lock poisoned")
            .contains(&scheme.to_ascii_lowercase())
    }

    /// Validates an incoming URL and returns its scheme.
    pub fn validate(&self, url: &str) -> Result<String> {
        let scheme = url
            .split_once(':')
            .map(|(scheme, _)| scheme.to_ascii_lowercase())
            .ok_or_else(|| anyhow::anyhow!("deep link is missing a URL scheme"))?;
        if !self.contains(&scheme) {
            bail!("deep-link scheme `{scheme}` is not registered");
        }
        Ok(scheme)
    }

    /// Returns all registered schemes.
    pub fn schemes(&self) -> Vec<String> {
        let mut schemes = self
            .schemes
            .lock()
            .expect("deep-link registry lock poisoned")
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        schemes.sort();
        schemes
    }
}

fn normalize_scheme(scheme: &str) -> Result<String> {
    let scheme = scheme.trim_end_matches(':').to_ascii_lowercase();
    if scheme.is_empty()
        || !scheme
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic())
        || !scheme.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
    {
        bail!("invalid URL scheme `{scheme}`");
    }
    Ok(scheme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_registered_deep_links() {
        let registry = DeepLinkRegistry::new();
        registry.register("myapp").expect("register scheme");
        assert_eq!(
            registry.validate("myapp://open?id=1").expect("valid URL"),
            "myapp"
        );
        assert!(registry.validate("other://open").is_err());
        assert!(registry.register("not a scheme").is_err());
    }
}
