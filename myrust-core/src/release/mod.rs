//! Release, distribution, signing, and update support.

/// Platform package-description generation.
pub mod packager;
/// Code-signing and notarization interfaces.
pub mod signing;
/// Application update checks, downloads, swaps, and rollback.
pub mod updater;

pub use packager::{PackageConfig, PackageOptions, PackageOutput, PackageTarget, Packager};
pub use signing::{SigningConfig, SigningPlatform};
pub use updater::{
    DeltaPatch, DeltaUpdateInfo, DeltaUpdateManifest, Ed25519SignatureVerifier, HttpUpdateSource,
    LocalUpdateSource, SignatureVerifier, UnimplementedSignatureVerifier, UpdateInfo, UpdateSource,
    Updater,
};
