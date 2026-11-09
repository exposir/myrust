//! Release, distribution, signing, and update support.

/// Platform package-description generation.
pub mod packager;
/// Code-signing and notarization interfaces.
pub mod signing;
/// Application update checks, downloads, swaps, and rollback.
pub mod updater;

pub use packager::{PackageConfig, PackageOutput, Packager};
pub use signing::{SigningConfig, SigningPlatform};
pub use updater::{
    HttpUpdateSource, LocalUpdateSource, SignatureVerifier, UnimplementedSignatureVerifier,
    UpdateInfo, UpdateSource, Updater,
};
