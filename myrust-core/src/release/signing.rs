//! Code-signing and notarization interfaces.
//!
//! No platform signing tool is invoked in batch D. Every operation returns an
//! explicit error explaining the required external toolchain.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

/// Target platform for signing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigningPlatform {
    /// Apple codesign and notarization.
    MacOs,
    /// Windows Authenticode signing.
    Windows,
    /// Linux package or repository signing.
    Linux,
}

/// Signing configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningConfig {
    /// Target platform.
    pub platform: SigningPlatform,
    /// Signing identity, certificate path, or key id.
    pub identity: String,
    /// Artifact to sign.
    pub artifact: PathBuf,
    /// Optional output path.
    pub output: Option<PathBuf>,
}

impl SigningConfig {
    /// Signs an artifact using the platform toolchain.
    ///
    /// Always returns an error in batch D. macOS requires `codesign`,
    /// Windows requires `signtool` or `rcodesign`, and Linux requires the
    /// project's chosen package-signing tool.
    pub fn sign(&self) -> Result<()> {
        bail!(
            "code signing is not implemented in Phase 2 batch D for {:?}; use codesign, signtool, or rcodesign",
            self.platform
        )
    }

    /// Submits a macOS artifact for notarization.
    ///
    /// Always returns an error in batch D; requires `xcrun notarytool` and
    /// valid Apple credentials.
    pub fn notarize(&self) -> Result<()> {
        if self.platform != SigningPlatform::MacOs {
            bail!("notarization is only applicable to macOS artifacts");
        }
        bail!(
            "notarization is not implemented in Phase 2 batch D; use xcrun notarytool with valid Apple credentials"
        )
    }

    /// Returns the artifact path.
    pub fn artifact(&self) -> &Path {
        &self.artifact
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_never_silently_succeeds() {
        let config = SigningConfig {
            platform: SigningPlatform::Windows,
            identity: "cert".to_string(),
            artifact: PathBuf::from("app.exe"),
            output: None,
        };
        assert!(config.sign().is_err());
        assert!(config.notarize().is_err());
    }
}
