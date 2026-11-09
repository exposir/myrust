//! Three-stage application updater.
//!
//! `check` reads a JSON manifest, `download` stores and unpacks the archive,
//! and `install` swaps the current installation directory while retaining a
//! rollback copy. Signature verification is a trait boundary; no production
//! signing algorithm is implemented in this batch, so the default verifier
//! rejects signed manifests instead of pretending verification succeeded.
//!
//! Delta updates and Sparkle-style update UI are not implemented.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use semver::Version;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

/// Update source abstraction.
pub trait UpdateSource: Send + Sync {
    /// Fetches the JSON update manifest.
    fn manifest(&self) -> Result<Vec<u8>>;

    /// Downloads the update archive named by the manifest.
    fn download(&self, url: &str) -> Result<Vec<u8>>;
}

/// HTTP(S) update source.
pub struct HttpUpdateSource {
    manifest_url: String,
    client: reqwest::blocking::Client,
}

impl HttpUpdateSource {
    /// Creates an HTTP update source.
    pub fn new(manifest_url: impl Into<String>) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .build()
            .context("failed to create update HTTP client")?;
        Ok(Self {
            manifest_url: manifest_url.into(),
            client,
        })
    }
}

impl UpdateSource for HttpUpdateSource {
    fn manifest(&self) -> Result<Vec<u8>> {
        let response = self
            .client
            .get(&self.manifest_url)
            .send()
            .with_context(|| format!("failed to fetch {}", self.manifest_url))?
            .error_for_status()
            .context("update manifest returned an error status")?;
        Ok(response
            .bytes()
            .context("failed to read update manifest")?
            .to_vec())
    }

    fn download(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("failed to download {url}"))?
            .error_for_status()
            .context("update archive returned an error status")?;
        Ok(response
            .bytes()
            .context("failed to read update archive")?
            .to_vec())
    }
}

/// Local application-provided update source.
pub struct LocalUpdateSource {
    manifest_path: PathBuf,
    archive_root: PathBuf,
}

impl LocalUpdateSource {
    /// Creates a local source rooted at a manifest and archive directory.
    pub fn new(manifest_path: impl Into<PathBuf>, archive_root: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: manifest_path.into(),
            archive_root: archive_root.into(),
        }
    }
}

impl UpdateSource for LocalUpdateSource {
    fn manifest(&self) -> Result<Vec<u8>> {
        fs::read(&self.manifest_path)
            .with_context(|| format!("failed to read {}", self.manifest_path.display()))
    }

    fn download(&self, url: &str) -> Result<Vec<u8>> {
        let path = self.archive_root.join(url);
        fs::read(&path).with_context(|| format!("failed to read {}", path.display()))
    }
}

/// Parsed update manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateInfo {
    /// New semantic version.
    pub version: String,
    /// Release notes in Markdown.
    pub notes: String,
    /// Archive signature.
    pub signature: String,
    /// Archive size in bytes.
    pub size: u64,
    /// Archive URL or local source identifier.
    pub url: String,
    /// Optional SHA-256 archive digest as lowercase hex.
    #[serde(default)]
    pub sha256: Option<String>,
}

/// Signature verification boundary.
pub trait SignatureVerifier: Send + Sync {
    /// Verifies the archive bytes and signature.
    fn verify(&self, bytes: &[u8], signature: &str) -> Result<()>;
}

/// Default verifier that always rejects a signed update.
///
/// No production signature algorithm is wired in batch D. This verifier
/// returns an explicit error so a signed manifest cannot be installed as if
/// verification had succeeded.
#[derive(Debug, Default)]
pub struct UnimplementedSignatureVerifier;

impl SignatureVerifier for UnimplementedSignatureVerifier {
    fn verify(&self, _bytes: &[u8], _signature: &str) -> Result<()> {
        bail!(
            "update signature verification is not implemented in Phase 2 batch D; provide a SignatureVerifier backed by the release signing key"
        )
    }
}

/// Prepared update archive and staging directory.
#[derive(Debug)]
pub struct DownloadedUpdate {
    info: UpdateInfo,
    staging: TempDir,
}

impl DownloadedUpdate {
    /// Returns update metadata.
    pub fn info(&self) -> &UpdateInfo {
        &self.info
    }
}

/// Three-stage updater.
pub struct Updater {
    source: Arc<dyn UpdateSource>,
    current_version: Version,
    verifier: Arc<dyn SignatureVerifier>,
}

impl Updater {
    /// Creates an updater with the current application version.
    pub fn new(
        source: Arc<dyn UpdateSource>,
        current_version: impl AsRef<str>,
        verifier: Arc<dyn SignatureVerifier>,
    ) -> Result<Self> {
        let current_version = Version::parse(current_version.as_ref())
            .context("invalid current application version")?;
        Ok(Self {
            source,
            current_version,
            verifier,
        })
    }

    /// Checks whether a newer version is available.
    pub fn check(&self) -> Result<Option<UpdateInfo>> {
        let manifest = self.source.manifest()?;
        let info: UpdateInfo =
            serde_json::from_slice(&manifest).context("invalid update manifest JSON")?;
        let version = Version::parse(&info.version).context("invalid update version")?;
        if version <= self.current_version {
            return Ok(None);
        }
        Ok(Some(info))
    }

    /// Downloads, verifies, and unpacks an update archive.
    pub fn download(&self, info: &UpdateInfo) -> Result<DownloadedUpdate> {
        let archive = self.source.download(&info.url)?;
        if info.size != 0 && archive.len() as u64 != info.size {
            bail!(
                "update archive size mismatch: expected {}, got {}",
                info.size,
                archive.len()
            );
        }
        if let Some(expected) = &info.sha256 {
            use sha2::{Digest, Sha256};
            let actual = Sha256::digest(&archive)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            if !actual.eq_ignore_ascii_case(expected) {
                bail!("update archive SHA-256 mismatch: expected {expected}, got {actual}");
            }
        }
        self.verifier.verify(&archive, &info.signature)?;
        let staging = tempfile::Builder::new()
            .prefix("myrust-update-")
            .tempdir()
            .context("failed to create update staging directory")?;
        unpack_archive(&archive, staging.path(), &info.url)?;
        Ok(DownloadedUpdate {
            info: info.clone(),
            staging,
        })
    }

    /// Installs a downloaded update by swapping the target directory.
    ///
    /// The old directory is retained as `<target>.rollback`. If replacement
    /// fails, the old directory is restored. Files absent from the new
    /// version disappear with the old directory. The rollback copy is retained
    /// after success for manual recovery.
    ///
    /// This swaps a directory tree, not a running executable. Callers must
    /// schedule relaunch after the process exits and must not run this while
    /// the target contains files that are still open by the current process.
    pub fn install(&self, update: &DownloadedUpdate, target: &Path) -> Result<()> {
        if !target.is_dir() {
            bail!("update target is not a directory: {}", target.display());
        }
        if let Some(installed) = update.info.version.strip_prefix('v') {
            let installed =
                Version::parse(installed).context("invalid installed update version")?;
            if installed <= self.current_version {
                bail!(
                    "refusing to install non-newer update {} over {}",
                    update.info.version,
                    self.current_version
                );
            }
        } else {
            let installed =
                Version::parse(&update.info.version).context("invalid installed update version")?;
            if installed <= self.current_version {
                bail!(
                    "refusing to install non-newer update {} over {}",
                    update.info.version,
                    self.current_version
                );
            }
        }
        let staged_root = find_staged_root(update.staging.path())?;
        let rollback = target.with_extension("rollback");
        if rollback.exists() {
            fs::remove_dir_all(&rollback)
                .with_context(|| format!("failed to clear {}", rollback.display()))?;
        }
        copy_directory(target, &rollback)
            .with_context(|| format!("failed to create rollback copy for {}", target.display()))?;

        fs::remove_dir_all(target)
            .with_context(|| format!("failed to clear {}", target.display()))?;
        if let Err(error) = move_directory_contents(&staged_root, target) {
            let _ = fs::remove_dir_all(target);
            let _ = copy_directory(&rollback, target);
            return Err(error).context("update swap failed and was rolled back");
        }
        Ok(())
    }
}

fn unpack_archive(bytes: &[u8], destination: &Path, name: &str) -> Result<()> {
    if name.ends_with(".zip") || looks_like_zip(bytes) {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("invalid ZIP archive")?;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index)?;
            let Some(path) = file.enclosed_name() else {
                bail!("update ZIP contains an unsafe path");
            };
            let output = destination.join(path);
            if file.is_dir() {
                fs::create_dir_all(&output)?;
            } else {
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut target = fs::File::create(&output)?;
                std::io::copy(&mut file, &mut target)?;
            }
        }
        return Ok(());
    }

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") || looks_like_gzip(bytes) {
        let decoder = GzDecoder::new(bytes);
        let mut archive = tar::Archive::new(decoder);
        for entry in archive.entries().context("invalid tar.gz archive")? {
            let mut entry = entry?;
            let path = entry
                .path()
                .context("tar entry has an invalid path")?
                .into_owned();
            if path.is_absolute()
                || path
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
            {
                bail!("update tar contains an unsafe path");
            }
            entry
                .unpack(destination.join(path))
                .context("failed to unpack tar entry")?;
        }
        return Ok(());
    }

    bail!("unsupported update archive format; expected .zip, .tar.gz, or .tgz")
}

fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

fn looks_like_gzip(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1f, 0x8b])
}

fn find_staged_root(staging: &Path) -> Result<PathBuf> {
    fs::read_dir(staging)?;
    Ok(staging.to_path_buf())
}

fn move_directory_contents(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        fs::rename(entry.path(), destination)?;
    }
    Ok(())
}

fn copy_directory(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[derive(Clone)]
    struct BytesSource {
        manifest: Vec<u8>,
        archive: Vec<u8>,
    }

    impl UpdateSource for BytesSource {
        fn manifest(&self) -> Result<Vec<u8>> {
            Ok(self.manifest.clone())
        }

        fn download(&self, _url: &str) -> Result<Vec<u8>> {
            Ok(self.archive.clone())
        }
    }

    struct AllowAll;
    impl SignatureVerifier for AllowAll {
        fn verify(&self, _bytes: &[u8], _signature: &str) -> Result<()> {
            Ok(())
        }
    }

    fn zip_archive(files: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            for (path, contents) in files {
                writer.start_file(*path, options).expect("start zip file");
                writer
                    .write_all(contents.as_bytes())
                    .expect("write zip file");
            }
            writer.finish().expect("finish zip");
        }
        buffer.into_inner()
    }

    #[test]
    fn check_and_install_replaces_old_files_and_rolls_back() {
        let archive = zip_archive(&[("bin/app", "new")]);
        let manifest = serde_json::to_vec(&UpdateInfo {
            version: "2.0.0".to_string(),
            notes: "new".to_string(),
            signature: "test".to_string(),
            size: archive.len() as u64,
            url: "app.zip".to_string(),
            sha256: None,
        })
        .expect("manifest");
        let source = Arc::new(BytesSource { manifest, archive });
        let updater = Updater::new(source, "1.0.0", Arc::new(AllowAll)).expect("updater");
        let info = updater.check().expect("check").expect("update");
        let downloaded = updater.download(&info).expect("download");

        let root = tempfile::tempdir().expect("temp root");
        let target = root.path().join("app");
        fs::create_dir_all(target.join("bin")).expect("create target");
        fs::write(target.join("bin/app"), "old").expect("old file");
        fs::write(target.join("obsolete.txt"), "old").expect("obsolete file");
        updater.install(&downloaded, &target).expect("install");
        assert_eq!(
            fs::read_to_string(target.join("bin/app")).expect("new file"),
            "new"
        );
        assert!(!target.join("obsolete.txt").exists());
        assert!(target.with_extension("rollback").exists());
    }

    #[test]
    fn default_verifier_rejects_signed_update() {
        let updater = Updater::new(
            Arc::new(BytesSource {
                manifest: Vec::new(),
                archive: Vec::new(),
            }),
            "1.0.0",
            Arc::new(UnimplementedSignatureVerifier),
        )
        .expect("updater");
        let info = UpdateInfo {
            version: "2.0.0".to_string(),
            notes: String::new(),
            signature: "sig".to_string(),
            size: 0,
            url: "app.zip".to_string(),
            sha256: None,
        };
        let error = updater.download(&info).expect_err("verification must fail");
        assert!(error.to_string().contains("not implemented"));
    }
}
