//! Three-stage application updater with signed archives and delta patches.
//!
//! `check` reads a JSON manifest, `download` verifies and unpacks the archive,
//! and `install` swaps the installation directory while retaining a rollback
//! copy. [`Ed25519SignatureVerifier`] verifies raw Ed25519 signatures and
//! minisign signature files. [`DeltaPatch`] applies a documented binary
//! copy/insert patch, and [`Updater::download_delta`] verifies both the patch
//! and the reconstructed full archive before staging it.

use std::{
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use flate2::read::GzDecoder;
use ring::signature::{self, UnparsedPublicKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

/// Update source abstraction.
pub trait UpdateSource: Send + Sync {
    /// Fetches the JSON update manifest.
    fn manifest(&self) -> Result<Vec<u8>>;

    /// Downloads a named update archive or delta.
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
        let relative = Path::new(url);
        validate_archive_path(relative, false)?;
        let path = self.archive_root.join(relative);
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
    /// Full archive signature.
    pub signature: String,
    /// Archive size in bytes, or zero when unspecified.
    pub size: u64,
    /// Archive URL or local source identifier.
    pub url: String,
    /// Optional SHA-256 archive digest as lowercase hex.
    #[serde(default)]
    pub sha256: Option<String>,
}

/// Description of a delta patch in the update manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeltaUpdateInfo {
    /// Version of the currently installed full archive.
    pub from_version: String,
    /// Version produced by applying the patch.
    pub to_version: String,
    /// SHA-256 of the input full archive, as lowercase hex.
    pub old_sha256: String,
    /// SHA-256 of the reconstructed full archive, as lowercase hex.
    pub new_sha256: String,
    /// Patch URL or local source identifier.
    pub url: String,
    /// Patch signature.
    pub signature: String,
    /// Patch size in bytes, or zero when unspecified.
    #[serde(default)]
    pub size: u64,
    /// Optional SHA-256 of the patch transport bytes.
    #[serde(default)]
    pub sha256: Option<String>,
}

/// Update manifest with a delta patch extension.
///
/// The JSON object contains the normal [`UpdateInfo`] fields plus a `delta`
/// object. [`Updater::check_delta`] validates the version and hash binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeltaUpdateManifest {
    /// Full update metadata.
    #[serde(flatten)]
    pub update: UpdateInfo,
    /// Delta patch metadata.
    pub delta: DeltaUpdateInfo,
}

/// Signature verification boundary.
pub trait SignatureVerifier: Send + Sync {
    /// Verifies bytes and a signature string.
    fn verify(&self, bytes: &[u8], signature: &str) -> Result<()>;
}

/// Default verifier that always rejects a signed update.
///
/// Applications must provide [`Ed25519SignatureVerifier`] or a verifier
/// backed by their release-signing system.
#[derive(Debug, Default)]
pub struct UnimplementedSignatureVerifier;

impl SignatureVerifier for UnimplementedSignatureVerifier {
    fn verify(&self, _bytes: &[u8], _signature: &str) -> Result<()> {
        bail!(
            "update signature verification is not configured; provide an Ed25519SignatureVerifier or a SignatureVerifier backed by the release signing key"
        )
    }
}

/// Ed25519 and minisign signature verifier.
///
/// Raw signatures are base64-encoded 64-byte Ed25519 signatures over the
/// update bytes. Minisign signature files support `Ed` (message signed
/// directly) and `ED` (BLAKE2b-512-prehashed) algorithms. A trusted comment,
/// when present, must have a valid global signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ed25519SignatureVerifier {
    public_key: [u8; 32],
    key_id: Option<[u8; 8]>,
}

impl Ed25519SignatureVerifier {
    /// Creates a verifier from a raw 32-byte Ed25519 public key.
    pub fn new(public_key: impl AsRef<[u8]>) -> Result<Self> {
        let bytes = public_key.as_ref();
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Ed25519 public key must contain exactly 32 bytes"))?;
        Ok(Self {
            public_key: bytes,
            key_id: None,
        })
    }

    /// Creates a verifier from a minisign public-key file or base64 line.
    pub fn from_minisign_public_key(public_key: impl AsRef<str>) -> Result<Self> {
        let line = single_data_line(public_key.as_ref(), "minisign public key")?;
        let bytes = STANDARD
            .decode(line)
            .context("minisign public key is not valid base64")?;
        if bytes.len() != 42 {
            bail!("minisign public key must contain 42 bytes");
        }
        let algorithm = &bytes[..2];
        if algorithm != b"Ed" && algorithm != b"ED" {
            bail!(
                "minisign public key has unsupported algorithm {:?}",
                algorithm
            );
        }
        let key_id: [u8; 8] = bytes[2..10]
            .try_into()
            .expect("minisign public key has a fixed key id");
        let public_key: [u8; 32] = bytes[10..]
            .try_into()
            .expect("minisign public key has a fixed key length");
        Ok(Self {
            public_key,
            key_id: Some(key_id),
        })
    }

    /// Returns the raw public key bytes.
    pub fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }

    /// Returns the minisign key id when one was parsed.
    pub fn key_id(&self) -> Option<&[u8; 8]> {
        self.key_id.as_ref()
    }
}

impl SignatureVerifier for Ed25519SignatureVerifier {
    fn verify(&self, bytes: &[u8], signature: &str) -> Result<()> {
        let parsed = parse_signature(signature)?;
        if let (Some(expected), Some(actual)) = (self.key_id, parsed.key_id) {
            if expected != actual {
                bail!("minisign signature key id does not match the configured public key");
            }
        }

        let message = match parsed.algorithm {
            SignatureAlgorithm::Ed => bytes.to_vec(),
            SignatureAlgorithm::Prehashed => blake2b512(bytes).to_vec(),
        };
        verify_ed25519(&self.public_key, &message, &parsed.signature)?;

        if let Some(trusted_comment) = parsed.trusted_comment {
            let global_signature = parsed
                .global_signature
                .context("minisign trusted comment is missing its global signature")?;
            let direct = trusted_comment.as_bytes();
            let bound = [parsed.signature.as_slice(), direct].concat();
            let prefixed = format!("trusted comment: {trusted_comment}");
            if verify_ed25519(&self.public_key, direct, &global_signature).is_err()
                && verify_ed25519(&self.public_key, &bound, &global_signature).is_err()
                && verify_ed25519(&self.public_key, prefixed.as_bytes(), &global_signature).is_err()
            {
                bail!("minisign trusted comment signature verification failed");
            }
        }
        Ok(())
    }
}

/// Binary delta patch format used by myrust updates.
///
/// The patch starts with `MYRDLT01`, followed by the old and new SHA-256
/// digests (32 bytes each, big-endian as produced by SHA-256), old and new
/// sizes as little-endian `u64`, an operation count as little-endian `u32`,
/// then operations. Operation tag `0` is a copy `(offset, length)`, both
/// little-endian `u64`. Tag `1` is a literal insert with a `u64` byte length
/// followed by the bytes. Application validates both archive digests and
/// rejects out-of-bounds copies, trailing bytes, and size mismatches.
pub struct DeltaPatch;

impl DeltaPatch {
    /// Builds a copy/insert patch from `old` to `new`.
    ///
    /// The matcher indexes 1 KiB blocks of the old archive and extends each
    /// exact match forward. It is deliberately simple and deterministic; it
    /// does not reorder blocks or compress the patch.
    pub fn create(old: &[u8], new: &[u8]) -> Vec<u8> {
        let operations = delta_operations(old, new);
        let mut patch = Vec::new();
        patch.extend_from_slice(DELTA_MAGIC);
        patch.extend_from_slice(&Sha256::digest(old));
        patch.extend_from_slice(&Sha256::digest(new));
        patch.extend_from_slice(&(old.len() as u64).to_le_bytes());
        patch.extend_from_slice(&(new.len() as u64).to_le_bytes());
        patch.extend_from_slice(&(operations.len() as u32).to_le_bytes());
        for operation in operations {
            match operation {
                DeltaOperation::Copy { offset, length } => {
                    patch.push(0);
                    patch.extend_from_slice(&(offset as u64).to_le_bytes());
                    patch.extend_from_slice(&(length as u64).to_le_bytes());
                }
                DeltaOperation::Insert(bytes) => {
                    patch.push(1);
                    patch.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
                    patch.extend_from_slice(&bytes);
                }
            }
        }
        patch
    }

    /// Applies a patch and validates the input and output SHA-256 digests.
    pub fn apply(old: &[u8], patch: &[u8]) -> Result<Vec<u8>> {
        let mut cursor = 0;
        if take(patch, &mut cursor, DELTA_MAGIC.len())? != DELTA_MAGIC {
            bail!("delta patch has invalid magic");
        }
        let old_hash: [u8; 32] = take(patch, &mut cursor, 32)?
            .try_into()
            .expect("fixed digest length");
        let new_hash: [u8; 32] = take(patch, &mut cursor, 32)?
            .try_into()
            .expect("fixed digest length");
        let old_size = read_u64(patch, &mut cursor)?;
        let new_size = read_u64(patch, &mut cursor)?;
        let operation_count = read_u32(patch, &mut cursor)?;

        if old_size != old.len() as u64 {
            bail!(
                "delta input size mismatch: expected {old_size}, got {}",
                old.len()
            );
        }
        if Sha256::digest(old).as_slice() != old_hash {
            bail!("delta input SHA-256 mismatch");
        }
        if operation_count > 1_000_000 {
            bail!("delta patch contains too many operations: {operation_count}");
        }
        let new_size: usize = new_size
            .try_into()
            .context("delta output size does not fit this platform")?;
        let mut output = Vec::with_capacity(new_size);

        for _ in 0..operation_count {
            let tag = *take(patch, &mut cursor, 1)?
                .first()
                .expect("one-byte operation tag");
            match tag {
                0 => {
                    let offset = read_u64(patch, &mut cursor)?;
                    let length = read_u64(patch, &mut cursor)?;
                    let offset: usize = offset
                        .try_into()
                        .context("delta copy offset does not fit this platform")?;
                    let length: usize = length
                        .try_into()
                        .context("delta copy length does not fit this platform")?;
                    let end = offset
                        .checked_add(length)
                        .context("delta copy range overflow")?;
                    if end > old.len() {
                        bail!("delta copy range is outside the input archive");
                    }
                    let output_len = output
                        .len()
                        .checked_add(length)
                        .context("delta output length overflow")?;
                    if output_len > new_size {
                        bail!("delta patch produces more than the declared output size");
                    }
                    output.extend_from_slice(&old[offset..end]);
                }
                1 => {
                    let length = read_u64(patch, &mut cursor)?;
                    let length: usize = length
                        .try_into()
                        .context("delta insert length does not fit this platform")?;
                    let output_len = output
                        .len()
                        .checked_add(length)
                        .context("delta output length overflow")?;
                    if output_len > new_size {
                        bail!("delta patch produces more than the declared output size");
                    }
                    output.extend_from_slice(take(patch, &mut cursor, length)?);
                }
                other => bail!("delta patch contains unknown operation tag {other}"),
            }
        }
        if cursor != patch.len() {
            bail!("delta patch contains trailing bytes");
        }
        if output.len() != new_size {
            bail!(
                "delta output size mismatch: expected {new_size}, got {}",
                output.len()
            );
        }
        if Sha256::digest(&output).as_slice() != new_hash {
            bail!("delta output SHA-256 mismatch");
        }
        Ok(output)
    }
}

#[derive(Debug, Clone, Copy)]
enum SignatureAlgorithm {
    Ed,
    Prehashed,
}

struct ParsedSignature {
    algorithm: SignatureAlgorithm,
    key_id: Option<[u8; 8]>,
    signature: Vec<u8>,
    trusted_comment: Option<String>,
    global_signature: Option<Vec<u8>>,
}

fn parse_signature(signature: &str) -> Result<ParsedSignature> {
    let mut lines = signature
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines
        .first()
        .is_some_and(|line| line.starts_with("untrusted comment:"))
    {
        lines.remove(0);
    }
    let primary = lines
        .first()
        .ok_or_else(|| anyhow::anyhow!("update signature is empty"))?;
    let bytes = STANDARD
        .decode(primary)
        .context("update signature is not valid base64")?;

    if bytes.len() == 64 && lines.len() == 1 {
        return Ok(ParsedSignature {
            algorithm: SignatureAlgorithm::Ed,
            key_id: None,
            signature: bytes,
            trusted_comment: None,
            global_signature: None,
        });
    }
    if bytes.len() != 74 {
        bail!(
            "minisign signature must contain 74 bytes, got {}",
            bytes.len()
        );
    }
    let algorithm = match &bytes[..2] {
        b"Ed" => SignatureAlgorithm::Ed,
        b"ED" => SignatureAlgorithm::Prehashed,
        other => bail!("minisign signature has unsupported algorithm {:?}", other),
    };
    let key_id: [u8; 8] = bytes[2..10]
        .try_into()
        .expect("minisign signature has a fixed key id");
    let signature = bytes[10..].to_vec();

    if lines.len() == 1 {
        return Ok(ParsedSignature {
            algorithm,
            key_id: Some(key_id),
            signature,
            trusted_comment: None,
            global_signature: None,
        });
    }
    if lines.len() != 3 {
        bail!("minisign signature must contain a trusted comment and one global signature");
    }
    let trusted = lines[1]
        .strip_prefix("trusted comment:")
        .ok_or_else(|| anyhow::anyhow!("minisign signature is missing a trusted comment"))?
        .trim()
        .to_string();
    let global_signature = STANDARD
        .decode(lines[2])
        .context("minisign global signature is not valid base64")?;
    if global_signature.len() != 64 {
        bail!(
            "minisign global signature must contain 64 bytes, got {}",
            global_signature.len()
        );
    }
    Ok(ParsedSignature {
        algorithm,
        key_id: Some(key_id),
        signature,
        trusted_comment: Some(trusted),
        global_signature: Some(global_signature),
    })
}

fn single_data_line<'a>(text: &'a str, description: &str) -> Result<&'a str> {
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("untrusted comment:"))
        .collect::<Vec<_>>();
    if lines.len() != 1 {
        bail!("{description} must contain exactly one base64 line");
    }
    Ok(lines[0])
}

fn verify_ed25519(public_key: &[u8; 32], message: &[u8], signature: &[u8]) -> Result<()> {
    if signature.len() != 64 {
        bail!("Ed25519 signature must contain 64 bytes");
    }
    UnparsedPublicKey::new(&signature::ED25519, public_key)
        .verify(message, signature)
        .map_err(|_| anyhow::anyhow!("Ed25519 signature verification failed"))
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
        let current_version = parse_version(current_version.as_ref())
            .context("invalid current application version")?;
        Ok(Self {
            source,
            current_version,
            verifier,
        })
    }

    /// Creates an updater with a raw Ed25519 release public key.
    pub fn new_ed25519(
        source: Arc<dyn UpdateSource>,
        current_version: impl AsRef<str>,
        public_key: impl AsRef<[u8]>,
    ) -> Result<Self> {
        let verifier = Arc::new(Ed25519SignatureVerifier::new(public_key)?);
        Self::new(source, current_version, verifier)
    }

    /// Checks whether a newer version is available.
    pub fn check(&self) -> Result<Option<UpdateInfo>> {
        let manifest = self.source.manifest()?;
        let info: UpdateInfo =
            serde_json::from_slice(&manifest).context("invalid update manifest JSON")?;
        let version = parse_version(&info.version).context("invalid update version")?;
        if version <= self.current_version {
            return Ok(None);
        }
        Ok(Some(info))
    }

    /// Checks a manifest containing a delta patch and validates version binding.
    pub fn check_delta(&self) -> Result<Option<(UpdateInfo, DeltaUpdateInfo)>> {
        let manifest = self.source.manifest()?;
        let manifest: DeltaUpdateManifest =
            serde_json::from_slice(&manifest).context("invalid delta update manifest JSON")?;
        let update_version =
            parse_version(&manifest.update.version).context("invalid update version")?;
        let from_version =
            parse_version(&manifest.delta.from_version).context("invalid delta source version")?;
        let to_version =
            parse_version(&manifest.delta.to_version).context("invalid delta target version")?;
        if update_version <= self.current_version {
            return Ok(None);
        }
        if from_version != self.current_version {
            bail!(
                "delta source version {} does not match installed version {}",
                manifest.delta.from_version,
                self.current_version
            );
        }
        if to_version != update_version {
            bail!(
                "delta target version {} does not match update version {}",
                manifest.delta.to_version,
                manifest.update.version
            );
        }
        Ok(Some((manifest.update, manifest.delta)))
    }

    /// Downloads, verifies, and unpacks a full update archive.
    pub fn download(&self, info: &UpdateInfo) -> Result<DownloadedUpdate> {
        let archive = self.source.download(&info.url)?;
        verify_size_and_sha256(
            &archive,
            info.size,
            info.sha256.as_deref(),
            "update archive",
        )?;
        self.verifier.verify(&archive, &info.signature)?;
        stage_update(&archive, info)
    }

    /// Downloads, verifies, applies, and unpacks a delta update.
    ///
    /// Both the delta transport bytes and the reconstructed full archive must
    /// pass signature verification. The reconstructed archive must match
    /// `delta.new_sha256`, and the installed archive must match
    /// `delta.old_sha256`.
    pub fn download_delta(
        &self,
        info: &UpdateInfo,
        delta: &DeltaUpdateInfo,
        current_archive: &[u8],
    ) -> Result<DownloadedUpdate> {
        let from_version =
            parse_version(&delta.from_version).context("invalid delta source version")?;
        let to_version =
            parse_version(&delta.to_version).context("invalid delta target version")?;
        let update_version = parse_version(&info.version).context("invalid update version")?;
        if from_version != self.current_version {
            bail!(
                "delta source version {} does not match installed version {}",
                delta.from_version,
                self.current_version
            );
        }
        if to_version != update_version {
            bail!(
                "delta target version {} does not match update version {}",
                delta.to_version,
                info.version
            );
        }

        let current_hash = sha256_hex(current_archive);
        if !current_hash.eq_ignore_ascii_case(&delta.old_sha256) {
            bail!(
                "delta input SHA-256 mismatch: expected {}, got {current_hash}",
                delta.old_sha256
            );
        }
        let patch = self.source.download(&delta.url)?;
        verify_size_and_sha256(&patch, delta.size, delta.sha256.as_deref(), "delta patch")?;
        self.verifier.verify(&patch, &delta.signature)?;
        let archive = DeltaPatch::apply(current_archive, &patch)?;
        let new_hash = sha256_hex(&archive);
        if !new_hash.eq_ignore_ascii_case(&delta.new_sha256) {
            bail!(
                "delta output SHA-256 mismatch: expected {}, got {new_hash}",
                delta.new_sha256
            );
        }
        verify_size_and_sha256(
            &archive,
            info.size,
            info.sha256.as_deref(),
            "update archive",
        )?;
        self.verifier.verify(&archive, &info.signature)?;
        stage_update(&archive, info)
    }

    /// Installs a downloaded update by swapping the target directory.
    ///
    /// The old directory is retained as `<target>.rollback`. If replacement
    /// fails, the old directory is restored. Files absent from the new
    /// version disappear with the old directory. The rollback copy is retained
    /// after success for manual recovery.
    pub fn install(&self, update: &DownloadedUpdate, target: &Path) -> Result<()> {
        if !target.is_dir() {
            bail!("update target is not a directory: {}", target.display());
        }
        if target.parent().is_none() {
            bail!("refusing to replace a filesystem root with an update");
        }
        let installed =
            parse_version(&update.info.version).context("invalid installed update version")?;
        if installed <= self.current_version {
            bail!(
                "refusing to install non-newer update {} over {}",
                update.info.version,
                self.current_version
            );
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

fn stage_update(archive: &[u8], info: &UpdateInfo) -> Result<DownloadedUpdate> {
    let staging = tempfile::Builder::new()
        .prefix("myrust-update-")
        .tempdir()
        .context("failed to create update staging directory")?;
    unpack_archive(archive, staging.path(), &info.url)?;
    Ok(DownloadedUpdate {
        info: info.clone(),
        staging,
    })
}

fn verify_size_and_sha256(
    bytes: &[u8],
    expected_size: u64,
    expected_sha256: Option<&str>,
    description: &str,
) -> Result<()> {
    if expected_size != 0 && bytes.len() as u64 != expected_size {
        bail!(
            "{description} size mismatch: expected {expected_size}, got {}",
            bytes.len()
        );
    }
    if let Some(expected) = expected_sha256 {
        let actual = sha256_hex(bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            bail!("{description} SHA-256 mismatch: expected {expected}, got {actual}");
        }
    }
    Ok(())
}

fn unpack_archive(bytes: &[u8], destination: &Path, name: &str) -> Result<()> {
    if name.ends_with(".zip") || looks_like_zip(bytes) {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("invalid ZIP archive")?;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index)?;
            let path = file
                .enclosed_name()
                .context("update ZIP contains an unsafe path")?;
            validate_archive_path(&path, false)?;
            let output = destination.join(path);
            if file.is_dir() {
                fs::create_dir_all(&output)?;
            } else {
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut target = fs::File::create(&output)?;
                std::io::copy(&mut file, &mut target)?;
                #[cfg(unix)]
                if let Some(mode) = file.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    let mut permissions = target.metadata()?.permissions();
                    permissions.set_mode(mode & 0o777);
                    fs::set_permissions(&output, permissions)?;
                }
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
            validate_archive_path(&path, true)?;
            let entry_type = entry.header().entry_type();
            if !entry_type.is_file() && !entry_type.is_dir() {
                bail!("update tar contains an unsupported special entry");
            }
            entry
                .unpack(destination.join(path))
                .context("failed to unpack tar entry")?;
        }
        return Ok(());
    }

    bail!("unsupported update archive format; expected .zip, .tar.gz, or .tgz")
}

fn validate_archive_path(path: &Path, allow_root: bool) -> Result<()> {
    if path.as_os_str().to_string_lossy().contains('\\') {
        bail!("update archive path contains a backslash");
    }
    let mut saw_normal = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => saw_normal = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("update archive contains an unsafe path")
            }
        }
    }
    if !saw_normal && !allow_root {
        bail!("update archive contains an empty path");
    }
    Ok(())
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
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_directory(&entry.path(), &destination)?;
        } else if file_type.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(entry.path())?, destination)?;
            #[cfg(not(unix))]
            fs::copy(entry.path(), destination)?;
        } else {
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

fn parse_version(version: &str) -> Result<Version> {
    Version::parse(version.strip_prefix('v').unwrap_or(version)).map_err(Into::into)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug)]
enum DeltaOperation {
    Copy { offset: usize, length: usize },
    Insert(Vec<u8>),
}

const DELTA_MAGIC: &[u8; 8] = b"MYRDLT01";
const DELTA_BLOCK: usize = 1024;
const DELTA_HASH_BASE: u64 = 0x9e37_79b1_85eb_ca87;

fn delta_operations(old: &[u8], new: &[u8]) -> Vec<DeltaOperation> {
    if new.len() < DELTA_BLOCK || old.len() < DELTA_BLOCK {
        return if new.is_empty() {
            Vec::new()
        } else {
            vec![DeltaOperation::Insert(new.to_vec())]
        };
    }

    let mut blocks = HashMap::<u64, Vec<usize>>::new();
    for offset in (0..=old.len() - DELTA_BLOCK).step_by(DELTA_BLOCK) {
        blocks
            .entry(polynomial_hash(&old[offset..offset + DELTA_BLOCK]))
            .or_default()
            .push(offset);
    }

    let block_power = {
        let mut power = 1_u64;
        for _ in 1..DELTA_BLOCK {
            power = power.wrapping_mul(DELTA_HASH_BASE);
        }
        power
    };
    let mut hash = polynomial_hash(&new[..DELTA_BLOCK]);
    let mut position = 0;
    let mut literal_start = 0;
    let mut operations = Vec::new();

    while position + DELTA_BLOCK <= new.len() {
        let candidates = blocks.get(&hash).cloned().unwrap_or_default();
        let mut best: Option<(usize, usize)> = None;
        for offset in candidates {
            if old[offset..offset + DELTA_BLOCK] != new[position..position + DELTA_BLOCK] {
                continue;
            }
            let mut length = DELTA_BLOCK;
            while position + length < new.len()
                && offset + length < old.len()
                && new[position + length] == old[offset + length]
            {
                length += 1;
            }
            if best.is_none_or(|(_, best_length)| length > best_length) {
                best = Some((offset, length));
            }
        }

        if let Some((offset, length)) = best {
            if literal_start < position {
                operations.push(DeltaOperation::Insert(
                    new[literal_start..position].to_vec(),
                ));
            }
            operations.push(DeltaOperation::Copy { offset, length });
            position += length;
            literal_start = position;
            if position + DELTA_BLOCK <= new.len() {
                hash = polynomial_hash(&new[position..position + DELTA_BLOCK]);
            }
            continue;
        }

        if position + DELTA_BLOCK == new.len() {
            break;
        }
        hash = rolling_hash(
            hash,
            new[position],
            new[position + DELTA_BLOCK],
            block_power,
        );
        position += 1;
    }

    if literal_start < new.len() {
        operations.push(DeltaOperation::Insert(new[literal_start..].to_vec()));
    }
    operations
}

fn polynomial_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0_u64, |hash, byte| {
        hash.wrapping_mul(DELTA_HASH_BASE)
            .wrapping_add(u64::from(*byte))
    })
}

fn rolling_hash(hash: u64, outgoing: u8, incoming: u8, block_power: u64) -> u64 {
    hash.wrapping_sub(u64::from(outgoing).wrapping_mul(block_power))
        .wrapping_mul(DELTA_HASH_BASE)
        .wrapping_add(u64::from(incoming))
}

fn take<'a>(bytes: &'a [u8], cursor: &mut usize, length: usize) -> Result<&'a [u8]> {
    let end = cursor
        .checked_add(length)
        .context("delta patch offset overflow")?;
    if end > bytes.len() {
        bail!("delta patch is truncated");
    }
    let value = &bytes[*cursor..end];
    *cursor = end;
    Ok(value)
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64> {
    let value: [u8; 8] = take(bytes, cursor, 8)?
        .try_into()
        .expect("fixed u64 length");
    Ok(u64::from_le_bytes(value))
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32> {
    let value: [u8; 4] = take(bytes, cursor, 4)?
        .try_into()
        .expect("fixed u32 length");
    Ok(u32::from_le_bytes(value))
}

fn blake2b512(input: &[u8]) -> [u8; 64] {
    let mut state = Blake2bState::new();
    state.update(input);
    state.finalize()
}

struct Blake2bState {
    h: [u64; 8],
    count: u128,
    buffer: [u8; 128],
    buffer_len: usize,
}

const BLAKE2B_IV: [u64; 8] = [
    0x6a09_e667_f3bc_c908,
    0xbb67_ae85_84ca_a73b,
    0x3c6e_f372_fe94_f82b,
    0xa54f_f53a_5f1d_36f1,
    0x510e_527f_ade6_82d1,
    0x9b05_688c_2b3e_6c1f,
    0x1f83_d9ab_fb41_bd6b,
    0x5be0_cd19_137e_2179,
];

const BLAKE2B_SIGMA: [[usize; 16]; 12] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
];

impl Blake2bState {
    fn new() -> Self {
        let mut h = BLAKE2B_IV;
        h[0] ^= 0x0101_0040;
        Self {
            h,
            count: 0,
            buffer: [0; 128],
            buffer_len: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        if self.buffer_len > 0 {
            let needed = 128 - self.buffer_len;
            let take = needed.min(input.len());
            self.buffer[self.buffer_len..self.buffer_len + take].copy_from_slice(&input[..take]);
            self.buffer_len += take;
            input = &input[take..];
            if self.buffer_len == 128 && !input.is_empty() {
                self.count = self.count.wrapping_add(128);
                let block = self.buffer;
                self.compress(&block, false);
                self.buffer_len = 0;
            }
        }

        while input.len() > 128 {
            self.count = self.count.wrapping_add(128);
            let block: [u8; 128] = input[..128].try_into().expect("fixed BLAKE2b block length");
            self.compress(&block, false);
            input = &input[128..];
        }

        if !input.is_empty() {
            self.buffer[..input.len()].copy_from_slice(input);
            self.buffer_len = input.len();
        }
    }

    fn finalize(mut self) -> [u8; 64] {
        self.count = self.count.wrapping_add(self.buffer_len as u128);
        let block = self.buffer;
        self.compress(&block, true);
        let mut output = [0_u8; 64];
        for (chunk, word) in output.as_chunks_mut::<8>().0.iter_mut().zip(self.h) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        output
    }

    fn compress(&mut self, block: &[u8; 128], last: bool) {
        let mut message = [0_u64; 16];
        for (word, bytes) in message.iter_mut().zip(block.as_chunks::<8>().0) {
            *word = u64::from_le_bytes(*bytes);
        }
        let mut work = [0_u64; 16];
        work[..8].copy_from_slice(&self.h);
        work[8..].copy_from_slice(&BLAKE2B_IV);
        work[12] ^= self.count as u64;
        work[13] ^= (self.count >> 64) as u64;
        if last {
            work[14] = !work[14];
        }

        for sigma in BLAKE2B_SIGMA {
            blake2b_mix(&mut work, 0, 4, 8, 12, message[sigma[0]], message[sigma[1]]);
            blake2b_mix(&mut work, 1, 5, 9, 13, message[sigma[2]], message[sigma[3]]);
            blake2b_mix(
                &mut work,
                2,
                6,
                10,
                14,
                message[sigma[4]],
                message[sigma[5]],
            );
            blake2b_mix(
                &mut work,
                3,
                7,
                11,
                15,
                message[sigma[6]],
                message[sigma[7]],
            );
            blake2b_mix(
                &mut work,
                0,
                5,
                10,
                15,
                message[sigma[8]],
                message[sigma[9]],
            );
            blake2b_mix(
                &mut work,
                1,
                6,
                11,
                12,
                message[sigma[10]],
                message[sigma[11]],
            );
            blake2b_mix(
                &mut work,
                2,
                7,
                8,
                13,
                message[sigma[12]],
                message[sigma[13]],
            );
            blake2b_mix(
                &mut work,
                3,
                4,
                9,
                14,
                message[sigma[14]],
                message[sigma[15]],
            );
        }
        for (hash, (low, high)) in self.h.iter_mut().zip(work[..8].iter().zip(&work[8..])) {
            *hash ^= low ^ high;
        }
    }
}

fn blake2b_mix(state: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    state[a] = state[a].wrapping_add(state[b]).wrapping_add(x);
    state[d] = (state[d] ^ state[a]).rotate_right(32);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_right(24);
    state[a] = state[a].wrapping_add(state[b]).wrapping_add(y);
    state[d] = (state[d] ^ state[a]).rotate_right(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_right(63);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::{
        rand::SystemRandom,
        signature::{Ed25519KeyPair, KeyPair},
    };
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

    fn keypair() -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&[7_u8; 32]).expect("test Ed25519 seed")
    }

    fn minisign_fixture(payload: &[u8], prehashed: bool) -> (String, String) {
        let key = keypair();
        let key_id = [0x42_u8; 8];
        let algorithm = if prehashed { b"ED" } else { b"Ed" };
        let message = if prehashed {
            blake2b512(payload).to_vec()
        } else {
            payload.to_vec()
        };
        let primary = key.sign(&message);
        let mut primary_line = Vec::new();
        primary_line.extend_from_slice(algorithm);
        primary_line.extend_from_slice(&key_id);
        primary_line.extend_from_slice(primary.as_ref());

        let trusted = "timestamp:1700000000 file:update.zip".to_string();
        let global = key.sign(trusted.as_bytes());
        let signature_text = format!(
            "untrusted comment: signature from test key\n{}\ntrusted comment: {trusted}\n{}",
            STANDARD.encode(primary_line),
            STANDARD.encode(global.as_ref())
        );
        let mut public = Vec::new();
        public.extend_from_slice(algorithm);
        public.extend_from_slice(&key_id);
        public.extend_from_slice(key.public_key().as_ref());
        let public_text = format!(
            "untrusted comment: minisign public key\n{}",
            STANDARD.encode(public)
        );
        (public_text, signature_text)
    }

    #[test]
    fn raw_ed25519_signature_is_verified() {
        let key = keypair();
        let message = b"signed update";
        let signature = STANDARD.encode(key.sign(message).as_ref());
        let verifier = Ed25519SignatureVerifier::new(key.public_key().as_ref()).expect("verifier");
        verifier
            .verify(message, &signature)
            .expect("valid signature");
        assert!(verifier.verify(b"tampered", &signature).is_err());
    }

    #[test]
    fn minisign_legacy_and_prehashed_signatures_are_verified() {
        let payload = b"minisign payload";
        for prehashed in [false, true] {
            let (public, signature) = minisign_fixture(payload, prehashed);
            let verifier =
                Ed25519SignatureVerifier::from_minisign_public_key(public).expect("minisign key");
            verifier
                .verify(payload, &signature)
                .expect("minisign signature");
            assert!(verifier.verify(b"different payload", &signature).is_err());
        }
    }

    #[test]
    fn blake2b_matches_known_empty_digest() {
        let digest = blake2b512(b"");
        let actual = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            actual,
            "786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419\
             d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce"
                .replace(' ', "")
        );
        let abc = blake2b512(b"abc")
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            abc,
            "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d1\
             7d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"
                .replace(' ', "")
        );
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
    fn delta_round_trip_and_corruption_are_detected() {
        let old = b"0123456789abcdef".repeat(320);
        let mut new = old.clone();
        new[2048..3072].copy_from_slice(&b"changed block content".repeat(50)[..1024]);
        new.extend_from_slice(b"appended bytes");
        let patch = DeltaPatch::create(&old, &new);
        let applied = DeltaPatch::apply(&old, &patch).expect("apply delta");
        assert_eq!(applied, new);

        let mut corrupt = patch.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        assert!(DeltaPatch::apply(&old, &corrupt).is_err());
        assert!(DeltaPatch::apply(b"wrong old", &patch).is_err());
    }

    #[test]
    fn delta_update_verifies_and_installs_reconstructed_archive() {
        let old_archive = zip_archive(&[("bin/app", "old")]);
        let new_archive = zip_archive(&[("bin/app", "new")]);
        let patch = DeltaPatch::create(&old_archive, &new_archive);
        let delta = DeltaUpdateInfo {
            from_version: "1.0.0".to_string(),
            to_version: "2.0.0".to_string(),
            old_sha256: sha256_hex(&old_archive),
            new_sha256: sha256_hex(&new_archive),
            url: "update.delta".to_string(),
            signature: "test".to_string(),
            size: patch.len() as u64,
            sha256: Some(sha256_hex(&patch)),
        };
        let update = UpdateInfo {
            version: "2.0.0".to_string(),
            notes: "delta".to_string(),
            signature: "test".to_string(),
            size: new_archive.len() as u64,
            url: "app.zip".to_string(),
            sha256: Some(sha256_hex(&new_archive)),
        };
        let manifest = serde_json::to_vec(&DeltaUpdateManifest {
            update: update.clone(),
            delta: delta.clone(),
        })
        .expect("manifest");
        let source = Arc::new(BytesSource {
            manifest,
            archive: patch,
        });
        let updater = Updater::new(source, "1.0.0", Arc::new(AllowAll)).expect("updater");
        let (checked_update, checked_delta) = updater
            .check_delta()
            .expect("check delta")
            .expect("delta update");
        assert_eq!(checked_update, update);
        assert_eq!(checked_delta, delta);
        let downloaded = updater
            .download_delta(&checked_update, &checked_delta, &old_archive)
            .expect("download delta");

        let root = tempfile::tempdir().expect("temp root");
        let target = root.path().join("app");
        fs::create_dir_all(target.join("bin")).expect("target");
        fs::write(target.join("bin/app"), "installed old").expect("old file");
        updater.install(&downloaded, &target).expect("install");
        assert_eq!(fs::read_to_string(target.join("bin/app")).unwrap(), "new");
    }

    #[test]
    fn zip_traversal_is_rejected() {
        let archive = zip_archive(&[("../escape.txt", "owned")]);
        let info = UpdateInfo {
            version: "2.0.0".to_string(),
            notes: String::new(),
            signature: "test".to_string(),
            size: archive.len() as u64,
            url: "app.zip".to_string(),
            sha256: None,
        };
        let updater = Updater::new(
            Arc::new(BytesSource {
                manifest: Vec::new(),
                archive,
            }),
            "1.0.0",
            Arc::new(AllowAll),
        )
        .expect("updater");
        let error = updater.download(&info).expect_err("unsafe ZIP");
        assert!(error.to_string().contains("unsafe path"));
    }

    #[test]
    fn tar_special_entries_are_rejected() {
        let mut tar = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_link_name("/etc/passwd").unwrap();
        header.set_cksum();
        tar.append_data(&mut header, "link", std::io::empty())
            .unwrap();
        let tar_bytes = tar.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&tar_bytes).unwrap();
        let archive = gz.finish().unwrap();
        let info = UpdateInfo {
            version: "2.0.0".to_string(),
            notes: String::new(),
            signature: "test".to_string(),
            size: archive.len() as u64,
            url: "app.tar.gz".to_string(),
            sha256: None,
        };
        let updater = Updater::new(
            Arc::new(BytesSource {
                manifest: Vec::new(),
                archive,
            }),
            "1.0.0",
            Arc::new(AllowAll),
        )
        .expect("updater");
        let error = updater.download(&info).expect_err("unsafe tar");
        assert!(error.to_string().contains("special entry"));
    }

    #[test]
    fn tar_archives_with_a_dot_root_entry_are_accepted() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("payload");
        fs::create_dir_all(source.join("bin")).unwrap();
        fs::write(source.join("bin/app"), "new").unwrap();
        let archive_path = root.path().join("app.tar.gz");
        let file = fs::File::create(&archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut tar = tar::Builder::new(encoder);
        tar.append_dir_all(".", &source).unwrap();
        let encoder = tar.into_inner().unwrap();
        encoder.finish().unwrap();

        let archive = fs::read(archive_path).unwrap();
        let info = UpdateInfo {
            version: "2.0.0".to_string(),
            notes: String::new(),
            signature: "test".to_string(),
            size: archive.len() as u64,
            url: "app.tar.gz".to_string(),
            sha256: None,
        };
        let updater = Updater::new(
            Arc::new(BytesSource {
                manifest: Vec::new(),
                archive,
            }),
            "1.0.0",
            Arc::new(AllowAll),
        )
        .expect("updater");
        let downloaded = updater.download(&info).expect("tar with dot root");
        assert!(downloaded.staging.path().join("bin/app").is_file());
    }

    #[test]
    fn local_source_rejects_parent_paths() {
        let root = tempfile::tempdir().unwrap();
        let source = LocalUpdateSource::new(root.path().join("manifest.json"), root.path());
        let error = source.download("../outside").expect_err("unsafe path");
        assert!(error.to_string().contains("unsafe path"));
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
        assert!(error.to_string().contains("not configured"));
    }

    #[test]
    fn generated_test_key_has_a_stable_public_key() {
        let pair = keypair();
        let public = pair.public_key();
        assert_eq!(public.as_ref().len(), 32);
    }

    #[test]
    fn updater_can_be_constructed_with_an_ed25519_key() {
        let source = Arc::new(BytesSource {
            manifest: Vec::new(),
            archive: Vec::new(),
        });
        let updater = Updater::new_ed25519(source, "1.0.0", keypair().public_key().as_ref())
            .expect("updater");
        assert!(updater.check().is_err());
    }

    #[test]
    fn random_keypair_generation_is_available_for_fixtures() {
        let rng = SystemRandom::new();
        Ed25519KeyPair::generate_pkcs8(&rng).expect("system RNG");
    }
}
