MyRust packages an application from one staged executable and a `PackageConfig`. The existing `Packager::generate` method still writes reviewable metadata for all platforms; `Packager::package` builds the current host and `Packager::package_for` selects a target explicitly. The public `PackageConfig` and `Packager::generate` signatures are unchanged.

```rust
use myrust_core::{
    PackageConfig, PackageOptions, PackageTarget, Packager,
};

let config = PackageConfig {
    app_id: "com.example.demo".into(),
    name: "Demo".into(),
    version: "1.2.3".into(),
    author: "Example".into(),
    identifier: "com.example.demo".into(),
    resources: vec!["assets".into()],
    icons: vec!["icon.png".into()],
};

let output = Packager::package_for(
    &config,
    "target/release/demo",
    "dist",
    PackageTarget::Linux,
    &PackageOptions::default(),
)?;
```

## Linux

Linux packaging stages `usr/bin/<name>`, resources under `usr/share/<app_id>`, a desktop entry, and a self-contained `install.sh`. It creates `<package>-<version>-<arch>.tar.gz` first, then builds a `.deb` with `dpkg-deb --build --root-owner-group`. The Arch directory receives a source tarball, a checksum-bearing `PKGBUILD`, and an install hook. `makepkg` is optional: it does not run by default, and enabling `PackageOptions { run_makepkg: true, .. }` makes a missing `makepkg` an explicit error.

`install.sh` supports both layouts. Inside the tarball it installs the bundled binary and resources under `${PREFIX:-$HOME/.local}`. When run from the skeleton without a staged payload, `install.sh <binary>` preserves the original single-file fallback.

## macOS

`Packager::package_for(..., PackageTarget::MacOs, ...)` copies the executable into `Demo.app/Contents/MacOS`, writes `Info.plist`, copies resources and icons into `Contents/Resources`, then invokes:

```text
hdiutil create -volname Demo -srcfolder Demo.app -ov -format UDZO Demo-1.2.3.dmg
```

DMG generation is macOS-only. On another host the app metadata remains on disk and the call fails with an explicit host/tool error. Signing and notarization are separate steps:

```rust
let signing = SigningConfig {
    platform: SigningPlatform::MacOs,
    identity: "Developer ID Application: Example".into(),
    artifact: "dist/macos/Demo.app".into(),
    output: None,
};
signing.sign()?;
signing.notarize()?;
```

`sign()` prefers `codesign --force --options runtime --timestamp --sign <identity> <artifact>` on macOS and uses `rcodesign --pkey <identity> <artifact>` when signing off a macOS host. `notarize()` is macOS-only and uses `xcrun`; credentials come from `NOTARYTOOL_PROFILE`, or from `APPLE_ID`, `APPLE_TEAM_ID`, and `APPLE_APP_PASSWORD`. It submits with `notarytool --wait` and staples the ticket. Missing credentials or tools fail explicitly. The test suite does not contact Apple's service.

## Windows

Windows packaging stages `windows/app/Demo.exe` plus resources, writes `installer.nsi`, and invokes `makensis`. The generated NSIS script installs under `%LOCALAPPDATA%\Demo`, creates Start Menu and desktop shortcuts, and includes an uninstall section. If `makensis` is absent, the staged tree and script remain and the error names the missing tool.

Windows Authenticode signing tries `signtool sign /fd SHA256 /td SHA256 /f <certificate> <artifact>` for a certificate file, or `/n <subject>` for a certificate-store identity. If `signtool` is unavailable, `rcodesign sign --pkey <identity> <artifact>` is the fallback.

## Failure and safety rules

- Package metadata rejects non-SemVer versions and unsafe identifier/name characters before writing files.
- The skeleton is written before any external tool runs, so a failed tool leaves reviewable output and any completed tarball or app bundle.
- Tool output is captured; a non-zero exit includes stderr (or stdout) and the preserved output directory.
- No packaging command uses a shell, so paths are passed as process arguments.
- Linux and Windows package builds are not verified on macOS or Windows in this repository's CI. Real Apple notarization and Authenticode timestamp verification still require credentials and the corresponding hosts.
