The updater keeps the original three-stage API: `check`, `download`, `install`. It now has a usable signature verifier, delta patches, stricter archive path checks, and rollback. `UpdateInfo` keeps its existing fields; delta metadata is a separate type and does not break existing manifests.

```rust
use std::{path::Path, sync::Arc};
use myrust_core::{
    Ed25519SignatureVerifier, LocalUpdateSource, Updater,
};

let source = Arc::new(LocalUpdateSource::new("update.json", "releases"));
let verifier = Arc::new(
    Ed25519SignatureVerifier::from_minisign_public_key(
        std::fs::read_to_string("release.pub")?,
    )?,
);
let updater = Updater::new(source, "1.2.3", verifier)?;
if let Some(info) = updater.check()? {
    let downloaded = updater.download(&info)?;
    updater.install(&downloaded, Path::new("installed-app"))?;
}
```

## Signature formats

`Ed25519SignatureVerifier::new` takes a raw 32-byte public key and verifies a base64-encoded 64-byte signature over the archive bytes. `from_minisign_public_key` accepts a minisign public-key file or its base64 line and verifies standard minisign signature files:

- `Ed` signs the file bytes directly.
- `ED` signs BLAKE2b-512 of the file bytes.
- The 8-byte key id must match the configured public key.
- A trusted comment, when present, must have a valid global signature. The verifier accepts the standard trusted-comment form and the signature-bound form used by newer minisign variants.

No signature means no update: `UnimplementedSignatureVerifier` remains the explicit fail-closed default. Size and SHA-256 checks run before signature verification. Signature verification still runs even when `sha256` is omitted.

## Delta manifest

`DeltaUpdateManifest` contains the normal update object plus a `delta` object:

```json
{
  "version": "2.0.0",
  "notes": "Release notes",
  "signature": "base64 full-archive signature",
  "size": 123456,
  "url": "app-2.0.0.zip",
  "sha256": "full archive digest",
  "delta": {
    "from_version": "1.0.0",
    "to_version": "2.0.0",
    "old_sha256": "installed archive digest",
    "new_sha256": "reconstructed archive digest",
    "url": "1.0.0-to-2.0.0.myrust-delta",
    "signature": "base64 delta signature",
    "size": 4567,
    "sha256": "delta transport digest"
  }
}
```

`Updater::check_delta` validates that `from_version` equals the installed version and that `to_version` equals the update version. `download_delta(info, delta, installed_archive_bytes)` then:

1. Verifies the installed archive SHA-256.
2. Downloads and size/hash-checks the patch.
3. Verifies the patch signature.
4. Applies and self-checks the patch.
5. Verifies the reconstructed full archive SHA-256 and full archive signature.
6. Unpacks it into a staging directory.

The full-archive signature is deliberately required. A signed delta alone is not enough to authorize an installed tree.

## Patch format

The format is deterministic little-endian binary data:

```text
magic      8 bytes   "MYRDLT01"
old_hash  32 bytes   SHA-256 of the installed archive
new_hash  32 bytes   SHA-256 of the reconstructed archive
old_size   8 bytes   u64
new_size   8 bytes   u64
ops        4 bytes   u32 operation count

operation 0: u8 tag 0, u64 source offset, u64 length   copy old -> output
operation 1: u8 tag 1, u64 byte length, bytes           insert literal
```

`DeltaPatch::create` uses 1 KiB old-archive blocks, a rolling hash index, and exact byte checks. `DeltaPatch::apply` rejects unknown tags, out-of-range copies, output overflow, truncation, trailing bytes, and input/output digest mismatches. Hash checks are inside the patch as well as in the manifest, so changing either side fails rather than silently installing different bytes.

## Install and rollback

`install` still swaps a directory tree and retains `<target>.rollback`. It refuses non-newer versions and filesystem roots. The rollback copy is created before the target is cleared; if moving staged files fails, the old tree is copied back and the error is returned.

## Path safety and remaining gaps

ZIP entries use `enclosed_name` plus an explicit component check. Tar entries reject absolute paths, parent traversal, backslashes, symlinks, hard links, devices, and other special entries. `LocalUpdateSource` rejects parent paths before joining its archive root.

The updater does not decompress archives with a byte-budget or a subprocess sandbox, and it swaps directories rather than a running executable. Callers must schedule relaunch after exit. Delta creation is a simple block matcher, not an optimal binary differ. Sparkle-style UI, background download scheduling, and platform credential storage remain outside this core API.
