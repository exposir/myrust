//! Platform package-description generation.
//!
//! This module writes metadata and directory skeletons only. It never invokes
//! platform packaging tools and does not produce installable binaries. Phase 4
//! tooling must consume these files to build `.app`, `.deb`, PKGBUILD, MSI, and
//! similar artifacts.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Common application packaging metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageConfig {
    /// Stable application identifier, such as `com.example.app`.
    pub app_id: String,
    /// Human-readable application name.
    pub name: String,
    /// Application version.
    pub version: String,
    /// Application author or vendor.
    pub author: String,
    /// Platform bundle/package identifier.
    pub identifier: String,
    /// Resource files or directories to copy.
    pub resources: Vec<PathBuf>,
    /// Icon files to copy into generated skeletons.
    pub icons: Vec<PathBuf>,
}

/// Paths produced by a package-description run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageOutput {
    /// Generated files.
    pub files: Vec<PathBuf>,
    /// Generated directories.
    pub directories: Vec<PathBuf>,
}

/// Platform package-description generator.
pub struct Packager;

impl Packager {
    /// Generates all platform descriptions under `output_dir`.
    ///
    /// The output is a reviewable skeleton, not an installer. External tools
    /// such as `codesign`, `hdiutil`, `dpkg-deb`, `makepkg`, and Windows
    /// packaging tooling are intentionally not called.
    pub fn generate(config: &PackageConfig, output_dir: impl AsRef<Path>) -> Result<PackageOutput> {
        validate_config(config)?;
        let output_dir = output_dir.as_ref();
        fs::create_dir_all(output_dir)
            .with_context(|| format!("failed to create {}", output_dir.display()))?;

        let mut output = PackageOutput {
            files: Vec::new(),
            directories: Vec::new(),
        };
        output.directories.push(output_dir.to_path_buf());

        write_macos(config, output_dir, &mut output)?;
        write_windows(config, output_dir, &mut output)?;
        write_debian(config, output_dir, &mut output)?;
        write_arch(config, output_dir, &mut output)?;
        write_linux_installer(config, output_dir, &mut output)?;

        Ok(output)
    }
}

fn validate_config(config: &PackageConfig) -> Result<()> {
    for (field, value) in [
        ("app_id", &config.app_id),
        ("name", &config.name),
        ("version", &config.version),
        ("author", &config.author),
        ("identifier", &config.identifier),
    ] {
        if value.trim().is_empty() {
            bail!("PackageConfig::{field} must not be empty");
        }
    }
    if !config.identifier.contains('.') {
        bail!("PackageConfig::identifier must be a reverse-DNS-style identifier");
    }
    Ok(())
}

fn write_macos(
    config: &PackageConfig,
    output_dir: &Path,
    output: &mut PackageOutput,
) -> Result<()> {
    let app_name = format!("{}.app", config.name);
    let contents = output_dir.join("macos").join(&app_name).join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    for directory in [&contents, &macos, &resources] {
        fs::create_dir_all(directory)
            .with_context(|| format!("failed to create {}", directory.display()))?;
        output.directories.push(directory.clone());
    }
    let plist = contents.join("Info.plist");
    fs::write(&plist, macos_plist(config))?;
    output.files.push(plist);
    copy_assets(config, &resources, output)?;
    Ok(())
}

fn write_windows(
    config: &PackageConfig,
    output_dir: &Path,
    output: &mut PackageOutput,
) -> Result<()> {
    let directory = output_dir.join("windows");
    fs::create_dir_all(&directory)?;
    output.directories.push(directory.clone());
    let manifest = directory.join("installer-manifest.json");
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&serde_json::json!({
            "appId": config.app_id,
            "name": config.name,
            "version": config.version,
            "publisher": config.author,
            "installDirectory": format!("%LOCALAPPDATA%\\{}", config.name),
            "executable": format!("{}.exe", config.name),
            "shortcut": true,
            "signing": "not generated; use signtool or rcodesign in Phase 4"
        }))?,
    )?;
    output.files.push(manifest);
    Ok(())
}

fn write_debian(
    config: &PackageConfig,
    output_dir: &Path,
    output: &mut PackageOutput,
) -> Result<()> {
    let directory = output_dir.join("debian");
    let package_dir = directory.join("DEBIAN");
    fs::create_dir_all(&package_dir)?;
    output.directories.push(directory.clone());
    output.directories.push(package_dir.clone());
    let control = package_dir.join("control");
    let dependencies = BTreeSet::from(["libwebkit2gtk-4.1-0", "libgtk-3-0"]);
    fs::write(
        &control,
        format!(
            "Package: {}\nVersion: {}\nSection: utils\nPriority: optional\nArchitecture: amd64\nMaintainer: {}\nDepends: {}\nDescription: {}\n",
            config.app_id.to_ascii_lowercase().replace('_', "-"),
            config.version,
            config.author,
            dependencies.into_iter().collect::<Vec<_>>().join(", "),
            config.name,
        ),
    )?;
    output.files.push(control);
    Ok(())
}

fn write_arch(config: &PackageConfig, output_dir: &Path, output: &mut PackageOutput) -> Result<()> {
    let directory = output_dir.join("arch");
    fs::create_dir_all(&directory)?;
    output.directories.push(directory.clone());
    let pkgbuild = directory.join("PKGBUILD");
    fs::write(
        &pkgbuild,
        format!(
            "pkgname={}\npkgver={}\npkgrel=1\npkgdesc='{}'\narch=('x86_64')\nurl=''\nlicense=('MIT')\ndepends=('webkit2gtk-4.1' 'gtk3')\ninstall='{}.install'\npackage() {{\n  install -Dm755 \"$srcdir/{}\" \"$pkgdir/usr/bin/{}\"\n}}\n",
            config.app_id.to_ascii_lowercase(),
            config.version,
            config.name.replace('\'', ""),
            config.app_id,
            config.name,
            config.name,
        ),
    )?;
    output.files.push(pkgbuild);
    let install = directory.join(format!("{}.install", config.app_id));
    fs::write(
        &install,
        "post_install() {\n  update-desktop-database -q || true\n}\npost_upgrade() {\n  post_install\n}\n",
    )?;
    output.files.push(install);
    Ok(())
}

fn write_linux_installer(
    config: &PackageConfig,
    output_dir: &Path,
    output: &mut PackageOutput,
) -> Result<()> {
    let directory = output_dir.join("linux");
    fs::create_dir_all(&directory)?;
    output.directories.push(directory.clone());
    let script = directory.join("install.sh");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nset -eu\nAPP_ID='{}'\nAPP_NAME='{}'\nPREFIX=\"${{PREFIX:-$HOME/.local}}\"\ninstall -Dm755 \"$1\" \"$PREFIX/bin/$APP_NAME\"\nprintf '%s\\n' \"[Desktop Entry]\" \"Type=Application\" \"Name=$APP_NAME\" \"Exec=$PREFIX/bin/$APP_NAME\" \"Categories=Utility;\" > \"$PREFIX/share/applications/$APP_ID.desktop\"\n",
            config.app_id,
            config.name,
        ),
    )?;
    output.files.push(script);
    Ok(())
}

fn macos_plist(config: &PackageConfig) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>{}</string>
  <key>CFBundleName</key><string>{}</string>
  <key>CFBundleDisplayName</key><string>{}</string>
  <key>CFBundleShortVersionString</key><string>{}</string>
  <key>CFBundleVersion</key><string>{}</string>
  <key>CFBundleExecutable</key><string>{}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
"#,
        xml_escape(&config.identifier),
        xml_escape(&config.name),
        xml_escape(&config.name),
        xml_escape(&config.version),
        xml_escape(&config.version),
        xml_escape(&config.name),
    )
}

fn copy_assets(
    config: &PackageConfig,
    destination: &Path,
    output: &mut PackageOutput,
) -> Result<()> {
    for source in config.resources.iter().chain(config.icons.iter()) {
        if !source.exists() {
            continue;
        }
        let file_name = source
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("asset has no file name: {}", source.display()))?;
        let target = destination.join(file_name);
        if source.is_dir() {
            copy_dir(source, &target)?;
            output.directories.push(target);
        } else {
            fs::copy(source, &target)?;
            output.files.push(target);
        }
    }
    Ok(())
}

fn copy_dir(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_platform_descriptions() {
        let root = tempfile::tempdir().expect("temp root");
        let config = PackageConfig {
            app_id: "com.example.demo".to_string(),
            name: "Demo".to_string(),
            version: "1.2.3".to_string(),
            author: "Example".to_string(),
            identifier: "com.example.demo".to_string(),
            resources: Vec::new(),
            icons: Vec::new(),
        };
        let output = Packager::generate(&config, root.path()).expect("generate");
        assert!(output.files.iter().any(|path| path.ends_with("Info.plist")));
        assert!(output.files.iter().any(|path| path.ends_with("control")));
        assert!(output.files.iter().any(|path| path.ends_with("PKGBUILD")));
        assert!(output.files.iter().any(|path| path.ends_with("install.sh")));
    }
}
