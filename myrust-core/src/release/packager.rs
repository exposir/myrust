//! Platform package generation and native packaging-tool orchestration.
//!
//! [`Packager::generate`] keeps the original reviewable skeleton behavior.
//! [`Packager::package`] and [`Packager::package_for`] stage an executable,
//! invoke the relevant host tool, and return the generated artifact paths.
//! Missing tools and unsupported hosts are errors, never silent success. The
//! skeleton and any artifacts produced before the failure remain on disk.

use std::{
    collections::BTreeSet,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context, Result};
use flate2::{write::GzEncoder, Compression};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    /// Icon files to copy.
    pub icons: Vec<PathBuf>,
}

/// Paths produced by a packaging run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageOutput {
    /// Generated files and artifacts.
    pub files: Vec<PathBuf>,
    /// Generated directories.
    pub directories: Vec<PathBuf>,
}

/// Host package format to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageTarget {
    /// Linux tarball, install script, Debian package, and Arch PKGBUILD.
    Linux,
    /// macOS `.app` bundle and DMG.
    MacOs,
    /// Windows NSIS installer.
    Windows,
}

/// Optional packaging-tool behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PackageOptions {
    /// Run `makepkg` after generating the Arch source tarball and PKGBUILD.
    ///
    /// This is off by default because `makepkg` is optional and often absent.
    /// When enabled, a missing `makepkg` is an explicit error.
    pub run_makepkg: bool,
}

/// Platform package generator.
pub struct Packager;

impl Packager {
    /// Generates reviewable descriptions for all platforms under `output_dir`.
    ///
    /// This method deliberately does not invoke platform tools. Use
    /// [`Packager::package`] for the host package or
    /// [`Packager::package_for`] for a specific target.
    pub fn generate(config: &PackageConfig, output_dir: impl AsRef<Path>) -> Result<PackageOutput> {
        validate_config(config)?;
        let output_dir = output_dir.as_ref();
        fs::create_dir_all(output_dir)
            .with_context(|| format!("failed to create {}", output_dir.display()))?;

        let mut output = PackageOutput {
            files: Vec::new(),
            directories: vec![output_dir.to_path_buf()],
        };

        write_macos(config, output_dir, &mut output)?;
        write_windows(config, output_dir, &mut output)?;
        write_debian(config, output_dir, &mut output)?;
        write_arch(config, output_dir, &mut output)?;
        write_linux_installer(config, output_dir, &mut output)?;

        Ok(output)
    }

    /// Builds a package for the current host.
    pub fn package(
        config: &PackageConfig,
        executable: impl AsRef<Path>,
        output_dir: impl AsRef<Path>,
    ) -> Result<PackageOutput> {
        let target = if cfg!(target_os = "macos") {
            PackageTarget::MacOs
        } else if cfg!(target_os = "windows") {
            PackageTarget::Windows
        } else {
            PackageTarget::Linux
        };
        Self::package_for(
            config,
            executable,
            output_dir,
            target,
            &PackageOptions::default(),
        )
    }

    /// Builds a package for `target`.
    ///
    /// The reviewable skeleton is written first. Linux tarballs, macOS app
    /// bundles, and Windows NSIS staging directories are created before their
    /// external tool is invoked, so a tool failure leaves useful output behind.
    /// The error names both the missing tool and the preserved output directory.
    pub fn package_for(
        config: &PackageConfig,
        executable: impl AsRef<Path>,
        output_dir: impl AsRef<Path>,
        target: PackageTarget,
        options: &PackageOptions,
    ) -> Result<PackageOutput> {
        validate_config(config)?;
        let executable = executable.as_ref();
        if !executable.is_file() {
            bail!(
                "package executable does not exist or is not a file: {}",
                executable.display()
            );
        }
        let output_dir = output_dir.as_ref();
        let mut output = Self::generate(config, output_dir)?;

        match target {
            PackageTarget::Linux => {
                build_linux(config, executable, output_dir, options, &mut output)?;
            }
            PackageTarget::MacOs => {
                build_macos(config, executable, output_dir, &mut output)?;
            }
            PackageTarget::Windows => {
                build_windows(config, executable, output_dir, &mut output)?;
            }
        }
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
    for (field, value) in [
        ("app_id", &config.app_id),
        ("identifier", &config.identifier),
    ] {
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        {
            bail!(
                "PackageConfig::{field} may only contain ASCII letters, digits, '.', '-', and '_'"
            );
        }
        if value == "." || value == ".." || value.starts_with('.') || value.ends_with('.') {
            bail!("PackageConfig::{field} must be a safe file-name component");
        }
    }
    for (field, value) in [("name", &config.name), ("author", &config.author)] {
        if value.contains('\0')
            || value.contains('/')
            || value.contains('\\')
            || value.contains('\n')
            || value.contains('\r')
        {
            bail!("PackageConfig::{field} contains characters unsafe for package metadata");
        }
    }
    if config.name == "." || config.name == ".." || config.name.starts_with('.') {
        bail!("PackageConfig::name must be a safe file-name component");
    }
    if config.name.starts_with(char::is_whitespace)
        || config.name.ends_with(char::is_whitespace)
        || config.author.starts_with(char::is_whitespace)
        || config.author.ends_with(char::is_whitespace)
    {
        bail!("PackageConfig::name and PackageConfig::author must not have surrounding whitespace");
    }
    Version::parse(&config.version).context("PackageConfig::version must be valid SemVer")?;
    Ok(())
}

fn write_macos(
    config: &PackageConfig,
    output_dir: &Path,
    output: &mut PackageOutput,
) -> Result<()> {
    let app_name = format!("{}.app", config.name);
    let app = output_dir.join("macos").join(&app_name);
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");
    for directory in [&app, &contents, &macos, &resources] {
        fs::create_dir_all(directory)
            .with_context(|| format!("failed to create {}", directory.display()))?;
        set_directory_mode(directory)?;
        output.directories.push(directory.clone());
    }
    let plist = contents.join("Info.plist");
    fs::write(&plist, macos_plist(config))?;
    set_file_mode(&plist, 0o644)?;
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
    set_directory_mode(&directory)?;
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
            "executable": windows_executable_name(config),
            "shortcut": true,
            "signing": "run SigningConfig after makensis, using signtool or rcodesign"
        }))?,
    )?;
    set_file_mode(&manifest, 0o644)?;
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
    set_directory_mode(&directory)?;
    set_directory_mode(&package_dir)?;
    output.directories.push(directory.clone());
    output.directories.push(package_dir.clone());
    let control = package_dir.join("control");
    let dependencies = BTreeSet::from(["libwebkit2gtk-4.1-0", "libgtk-3-0"]);
    fs::write(
        &control,
        format!(
            "Package: {}\nVersion: {}\nSection: utils\nPriority: optional\nArchitecture: {}\nMaintainer: {}\nDepends: {}\nDescription: {}\n",
            debian_package_name(config),
            config.version,
            debian_architecture(),
            config.author,
            dependencies.into_iter().collect::<Vec<_>>().join(", "),
            config.name,
        ),
    )?;
    set_file_mode(&control, 0o644)?;
    output.files.push(control);
    Ok(())
}

fn write_arch(config: &PackageConfig, output_dir: &Path, output: &mut PackageOutput) -> Result<()> {
    let directory = output_dir.join("arch");
    fs::create_dir_all(&directory)?;
    set_directory_mode(&directory)?;
    output.directories.push(directory.clone());
    let pkgbuild = directory.join("PKGBUILD");
    fs::write(&pkgbuild, arch_pkgbuild(config, None))?;
    set_file_mode(&pkgbuild, 0o644)?;
    output.files.push(pkgbuild);
    let install = directory.join(format!("{}.install", debian_package_name(config)));
    fs::write(
        &install,
        "post_install() {\n  update-desktop-database -q || true\n}\npost_upgrade() {\n  post_install\n}\n",
    )?;
    set_file_mode(&install, 0o644)?;
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
    set_directory_mode(&directory)?;
    output.directories.push(directory.clone());

    let desktop = directory.join(format!("{}.desktop", config.app_id));
    fs::write(&desktop, desktop_entry(config))?;
    set_file_mode(&desktop, 0o644)?;
    output.files.push(desktop);

    let script = directory.join("install.sh");
    fs::write(&script, linux_install_script(config))?;
    set_executable(&script)?;
    output.files.push(script);
    Ok(())
}

fn build_linux(
    config: &PackageConfig,
    executable: &Path,
    output_dir: &Path,
    options: &PackageOptions,
    output: &mut PackageOutput,
) -> Result<()> {
    let stage = output_dir.join("linux").join("root");
    let stage_bin = stage.join("usr/bin");
    let stage_share = stage.join("usr/share").join(&config.app_id);
    let applications = stage.join("usr/share/applications");
    for directory in [&stage, &stage_bin, &stage_share, &applications] {
        fs::create_dir_all(directory)?;
        set_directory_mode(directory)?;
        push_unique_directory(output, directory.clone());
    }

    let installed_binary = stage_bin.join(&config.name);
    copy_executable(executable, &installed_binary)?;
    output.files.push(installed_binary);
    copy_payload(config, &stage_share, output)?;

    let desktop = applications.join(format!("{}.desktop", config.app_id));
    fs::write(&desktop, desktop_entry(config))?;
    set_file_mode(&desktop, 0o644)?;
    output.files.push(desktop);

    let install_script = stage.join("install.sh");
    fs::write(&install_script, linux_install_script(config))?;
    set_executable(&install_script)?;
    output.files.push(install_script);

    let tarball = output_dir
        .join("linux")
        .join(linux_artifact_name(config, "tar.gz"));
    create_tar_gz(&stage, &tarball, None)
        .with_context(|| format!("failed to create {}", tarball.display()))?;
    output.files.push(tarball);

    let deb_root = output_dir.join("debian").join("root");
    let deb_control = deb_root.join("DEBIAN");
    let deb_bin = deb_root.join("usr/bin");
    let deb_share = deb_root.join("usr/share").join(&config.app_id);
    let deb_applications = deb_root.join("usr/share/applications");
    for directory in [
        &deb_root,
        &deb_control,
        &deb_bin,
        &deb_share,
        &deb_applications,
    ] {
        fs::create_dir_all(directory)?;
        set_directory_mode(directory)?;
        push_unique_directory(output, directory.clone());
    }
    set_directory_mode(&deb_root)?;
    set_directory_mode(&deb_control)?;
    fs::copy(
        output_dir.join("debian/DEBIAN/control"),
        deb_control.join("control"),
    )?;
    let deb_binary = deb_bin.join(&config.name);
    copy_executable(executable, &deb_binary)?;
    output.files.push(deb_binary);
    copy_payload(config, &deb_share, output)?;
    let deb_desktop = deb_applications.join(format!("{}.desktop", config.app_id));
    fs::write(&deb_desktop, desktop_entry(config))?;
    set_file_mode(&deb_desktop, 0o644)?;
    output.files.push(deb_desktop);

    let deb_path = output_dir.join("debian").join(format!(
        "{}_{}_{}.deb",
        debian_package_name(config),
        config.version,
        debian_architecture()
    ));
    run_tool(
        "dpkg-deb",
        &dpkg_args(&deb_root, &deb_path),
        None,
        "Debian package generation",
        output_dir,
    )?;
    output.files.push(deb_path);

    let arch_dir = output_dir.join("arch");
    let arch_source_name = format!("{}-{}.tar.gz", debian_package_name(config), config.version);
    let prefix = PathBuf::from(format!(
        "{}-{}",
        debian_package_name(config),
        config.version
    ));
    let arch_source = arch_dir.join(&arch_source_name);
    create_tar_gz(&stage, &arch_source, Some(&prefix))?;
    output.files.push(arch_source.clone());
    let sha256 = sha256_file(&arch_source)?;
    let pkgbuild = arch_dir.join("PKGBUILD");
    fs::write(&pkgbuild, arch_pkgbuild(config, Some(&sha256)))?;
    set_file_mode(&pkgbuild, 0o644)?;

    if options.run_makepkg {
        run_tool(
            "makepkg",
            &["--noconfirm".to_string()],
            Some(&arch_dir),
            "Arch package generation",
            output_dir,
        )?;
        for entry in fs::read_dir(&arch_dir)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().contains(".pkg.tar.") {
                output.files.push(entry.path());
            }
        }
    }
    Ok(())
}

fn build_macos(
    config: &PackageConfig,
    executable: &Path,
    output_dir: &Path,
    output: &mut PackageOutput,
) -> Result<()> {
    if !cfg!(target_os = "macos") {
        bail!(
            "macOS DMG generation needs a macOS host with hdiutil; generated metadata remains in {}",
            output_dir.join("macos").display()
        );
    }

    let app = output_dir
        .join("macos")
        .join(format!("{}.app", config.name));
    let executable_target = app.join("Contents/MacOS").join(&config.name);
    copy_executable(executable, &executable_target)?;
    output.files.push(executable_target);

    let dmg = output_dir
        .join("macos")
        .join(format!("{}-{}.dmg", config.name, config.version));
    run_tool(
        "hdiutil",
        &hdiutil_args(config, &app, &dmg),
        None,
        "macOS DMG generation",
        output_dir,
    )?;
    output.files.push(dmg);
    Ok(())
}

fn build_windows(
    config: &PackageConfig,
    executable: &Path,
    output_dir: &Path,
    output: &mut PackageOutput,
) -> Result<()> {
    let windows_dir = output_dir.join("windows");
    let payload = windows_dir.join("app");
    fs::create_dir_all(&payload)?;
    set_directory_mode(&payload)?;
    push_unique_directory(output, payload.clone());

    let installed_binary = payload.join(windows_executable_name(config));
    copy_executable(executable, &installed_binary)?;
    output.files.push(installed_binary);
    copy_payload(config, &payload, output)?;

    let installer_name = format!("{}-{}-setup.exe", config.name, config.version);
    let script = windows_dir.join("installer.nsi");
    fs::write(&script, windows_nsis(config, &installer_name))?;
    set_file_mode(&script, 0o644)?;
    output.files.push(script.clone());

    run_tool(
        "makensis",
        &makensis_args(&script),
        Some(&windows_dir),
        "Windows NSIS installer generation",
        output_dir,
    )?;
    let installer = windows_dir.join(installer_name);
    if !installer.is_file() {
        bail!(
            "makensis reported success but did not create {}; staged files remain in {}",
            installer.display(),
            windows_dir.display()
        );
    }
    output.files.push(installer);
    Ok(())
}

fn arch_pkgbuild(config: &PackageConfig, source_sha256: Option<&str>) -> String {
    let package_name = debian_package_name(config);
    let source_name = format!("{package_name}-{}.tar.gz", config.version);
    let checksum = source_sha256.unwrap_or("SKIP");
    let quoted_name = shell_quote(&config.name);
    format!(
        "pkgname={package_name}\n\
         pkgver={}\n\
         pkgrel=1\n\
         pkgdesc={}\n\
         arch=('{}')\n\
         url=''\n\
         license=('MIT')\n\
         depends=('webkit2gtk-4.1' 'gtk3')\n\
         install={package_name}.install\n\
         source=('{source_name}')\n\
         sha256sums=('{checksum}')\n\
         package() {{\n\
           install -Dm755 \"$srcdir/{package_name}-$pkgver/usr/bin/\"{} \"$pkgdir/usr/bin/\"{}\n\
           install -Dm644 \"$srcdir/{package_name}-$pkgver/usr/share/applications/{}.desktop\" \"$pkgdir/usr/share/applications/{}.desktop\"\n\
           if [ -d \"$srcdir/{package_name}-$pkgver/usr/share/{}\" ]; then\n\
             mkdir -p \"$pkgdir/usr/share/{}\"\n\
             cp -a \"$srcdir/{package_name}-$pkgver/usr/share/{}/.\" \"$pkgdir/usr/share/{}/\"\n\
           fi\n\
         }}\n",
        config.version,
        quoted_name.as_str(),
        arch_architecture(),
        quoted_name.as_str(),
        quoted_name.as_str(),
        config.app_id,
        config.app_id,
        config.app_id,
        config.app_id,
        config.app_id,
        config.app_id,
    )
}

fn dpkg_args(root: &Path, output: &Path) -> Vec<String> {
    vec![
        "--build".to_string(),
        "--root-owner-group".to_string(),
        root.display().to_string(),
        output.display().to_string(),
    ]
}

fn hdiutil_args(config: &PackageConfig, app: &Path, output: &Path) -> Vec<String> {
    vec![
        "create".to_string(),
        "-volname".to_string(),
        config.name.clone(),
        "-srcfolder".to_string(),
        app.display().to_string(),
        "-ov".to_string(),
        "-format".to_string(),
        "UDZO".to_string(),
        output.display().to_string(),
    ]
}

fn makensis_args(script: &Path) -> Vec<String> {
    vec!["/V2".to_string(), script.display().to_string()]
}

fn linux_install_script(config: &PackageConfig) -> String {
    format!(
        "#!/bin/sh\n\
         set -eu\n\
         APP_ID={}\n\
         APP_NAME={}\n\
         PREFIX=\"${{PREFIX:-$HOME/.local}}\"\n\
         SCRIPT_DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\n\
         ROOT=''\n\
         if [ -x \"$SCRIPT_DIR/usr/bin/$APP_NAME\" ]; then\n\
           ROOT=$SCRIPT_DIR\n\
         elif [ -x \"$SCRIPT_DIR/root/usr/bin/$APP_NAME\" ]; then\n\
           ROOT=$SCRIPT_DIR/root\n\
         fi\n\
         if [ -z \"$ROOT\" ]; then\n\
           if [ \"$#\" -ne 1 ]; then\n\
             printf '%s\\n' 'usage: install.sh <binary>  (or run the packaged self-contained installer)' >&2\n\
             exit 2\n\
           fi\n\
           install -Dm755 \"$1\" \"$PREFIX/bin/$APP_NAME\"\n\
           exit 0\n\
         fi\n\
         install -Dm755 \"$ROOT/usr/bin/$APP_NAME\" \"$PREFIX/bin/$APP_NAME\"\n\
         install -Dm644 \"$ROOT/usr/share/applications/$APP_ID.desktop\" \"$PREFIX/share/applications/$APP_ID.desktop\"\n\
         if [ -d \"$ROOT/usr/share/$APP_ID\" ]; then\n\
           mkdir -p \"$PREFIX/share/$APP_ID\"\n\
           cp -R \"$ROOT/usr/share/$APP_ID/.\" \"$PREFIX/share/$APP_ID/\"\n\
         fi\n",
        shell_quote(&config.app_id),
        shell_quote(&config.name),
    )
}

fn desktop_entry(config: &PackageConfig) -> String {
    let name = desktop_escape(&config.name);
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec=\"{}\"\nCategories=Utility;\n",
        name, name
    )
}

fn desktop_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\t', "\\t")
}

fn windows_nsis(config: &PackageConfig, installer_name: &str) -> String {
    let name = nsis_quote(&config.name);
    let exe = nsis_quote(&windows_executable_name(config));
    format!(
        "Unicode true\n\
         !include \"MUI2.nsh\"\n\
         Name \"{name}\"\n\
         OutFile \"{}\"\n\
         InstallDir \"$LOCALAPPDATA\\{}\"\n\
         RequestExecutionLevel user\n\
         !insertmacro MUI_PAGE_DIRECTORY\n\
         !insertmacro MUI_PAGE_INSTFILES\n\
         !insertmacro MUI_LANGUAGE \"English\"\n\
         !insertmacro MUI_UNPAGE_CONFIRM\n\
         !insertmacro MUI_UNPAGE_INSTFILES\n\
         Section \"Install\"\n\
           SetOutPath \"$INSTDIR\"\n\
           File /r \"app\\*\"\n\
           WriteUninstaller \"$INSTDIR\\uninstall.exe\"\n\
           CreateShortcut \"$DESKTOP\\{name}.lnk\" \"$INSTDIR\\{exe}\"\n\
           CreateShortcut \"$INSTDIR\\{name}.lnk\" \"$INSTDIR\\{exe}\"\n\
         SectionEnd\n\
         Section \"Uninstall\"\n\
           Delete \"$DESKTOP\\{name}.lnk\"\n\
           Delete \"$INSTDIR\\{name}.lnk\"\n\
           RMDir /r \"$INSTDIR\"\n\
         SectionEnd\n",
        nsis_quote(installer_name),
        nsis_quote(&config.name),
    )
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

fn copy_payload(
    config: &PackageConfig,
    destination: &Path,
    output: &mut PackageOutput,
) -> Result<()> {
    copy_assets(config, destination, output)
}

fn copy_assets(
    config: &PackageConfig,
    destination: &Path,
    output: &mut PackageOutput,
) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    set_directory_mode(destination)?;
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
            push_unique_directory(output, target);
        } else {
            fs::copy(source, &target).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    source.display(),
                    target.display()
                )
            })?;
            output.files.push(target);
        }
    }
    Ok(())
}

fn copy_dir(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    set_directory_mode(destination)?;
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

fn copy_executable(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy executable {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    set_executable(destination)?;
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_directory_mode(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_directory_mode(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_file_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

fn create_tar_gz(source: &Path, destination: &Path, prefix: Option<&Path>) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(destination)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut archive = tar::Builder::new(encoder);
    if let Some(prefix) = prefix {
        archive.append_dir_all(prefix, source)?;
    } else {
        archive.append_dir_all(".", source)?;
    }
    let encoder = archive.into_inner()?;
    encoder.finish()?.flush()?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn run_tool(
    tool: &str,
    args: &[String],
    current_dir: Option<&Path>,
    operation: &str,
    output_dir: &Path,
) -> Result<()> {
    let program = find_tool(tool).ok_or_else(|| {
        anyhow::anyhow!(
            "{operation} needs `{tool}`, but it was not found on PATH; generated skeleton and staged files remain in {}",
            output_dir.display()
        )
    })?;
    let mut command = Command::new(&program);
    command.args(args);
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }
    let command_output = command.output().with_context(|| {
        format!(
            "failed to execute {} for {operation}; staged files remain in {}",
            program.display(),
            output_dir.display()
        )
    })?;
    if !command_output.status.success() {
        let stderr = String::from_utf8_lossy(&command_output.stderr);
        let stdout = String::from_utf8_lossy(&command_output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        bail!(
            "{operation} failed with {}: {}; staged files remain in {}",
            command_output.status,
            detail,
            output_dir.display()
        );
    }
    Ok(())
}

fn find_tool(tool: &str) -> Option<PathBuf> {
    if tool.contains('/') || tool.contains('\\') {
        let path = PathBuf::from(tool);
        return is_executable_file(&path).then_some(path);
    }
    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        for name in tool_names(tool) {
            let candidate = directory.join(name);
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn tool_names(tool: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        if Path::new(tool).extension().is_some() {
            return vec![tool.to_string()];
        }
        let extensions = env::var_os("PATHEXT")
            .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into())
            .to_string_lossy()
            .to_string();
        let mut names = vec![tool.to_string()];
        names.extend(
            extensions
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(|extension| format!("{tool}{}", extension.to_ascii_lowercase())),
        );
        names
    }
    #[cfg(not(windows))]
    {
        vec![tool.to_string()]
    }
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn linux_artifact_name(config: &PackageConfig, extension: &str) -> String {
    format!(
        "{}-{}-{}.{}",
        debian_package_name(config),
        config.version,
        arch_architecture(),
        extension
    )
}

fn debian_package_name(config: &PackageConfig) -> String {
    let source = if config.app_id.is_empty() {
        &config.identifier
    } else {
        &config.app_id
    };
    source.to_ascii_lowercase().replace('_', "-")
}

fn windows_executable_name(config: &PackageConfig) -> String {
    format!("{}.exe", config.name)
}

fn debian_architecture() -> &'static str {
    match env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "i386",
        "arm" => "armhf",
        other => other,
    }
}

fn arch_architecture() -> &'static str {
    match env::consts::ARCH {
        "x86" => "i686",
        other => other,
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn nsis_quote(value: &str) -> String {
    value.replace('$', "$$").replace('"', "$\\\"")
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn push_unique_directory(output: &mut PackageOutput, directory: PathBuf) {
    if !output.directories.contains(&directory) {
        output.directories.push(directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> PackageConfig {
        PackageConfig {
            app_id: "com.example.demo".to_string(),
            name: "Demo".to_string(),
            version: "1.2.3".to_string(),
            author: "Example".to_string(),
            identifier: "com.example.demo".to_string(),
            resources: Vec::new(),
            icons: Vec::new(),
        }
    }

    fn executable(root: &Path) -> PathBuf {
        let path = root.join("demo");
        fs::write(&path, b"test executable").expect("write executable");
        set_executable(&path).expect("chmod executable");
        path
    }

    #[test]
    fn generates_platform_descriptions() {
        let root = tempfile::tempdir().expect("temp root");
        let output = Packager::generate(&config(), root.path()).expect("generate");
        assert!(output.files.iter().any(|path| path.ends_with("Info.plist")));
        assert!(output.files.iter().any(|path| path.ends_with("control")));
        assert!(output.files.iter().any(|path| path.ends_with("PKGBUILD")));
        assert!(output.files.iter().any(|path| path.ends_with("install.sh")));
        assert!(output
            .files
            .iter()
            .any(|path| path.ends_with("installer-manifest.json")));
    }

    #[test]
    fn rejects_unsafe_metadata() {
        let mut config = config();
        config.name = "../Demo".to_string();
        let error = Packager::generate(&config, tempfile::tempdir().unwrap().path())
            .expect_err("unsafe name");
        assert!(error.to_string().contains("unsafe"));
    }

    #[test]
    fn linux_tarball_is_written_before_tool_failure_and_deb_is_built_when_available() {
        let root = tempfile::tempdir().expect("temp root");
        let exe = executable(root.path());
        let output_dir = root.path().join("packages");
        let result = Packager::package_for(
            &config(),
            &exe,
            &output_dir,
            PackageTarget::Linux,
            &PackageOptions::default(),
        );

        let tar = output_dir
            .join("linux")
            .join(linux_artifact_name(&config(), "tar.gz"));
        assert!(tar.is_file(), "tarball must survive a later tool failure");

        let unpacked = root.path().join("unpacked");
        fs::create_dir_all(&unpacked).unwrap();
        let file = fs::File::open(&tar).unwrap();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
        archive.unpack(&unpacked).expect("unpack tarball");
        assert!(unpacked.join("usr/bin/Demo").is_file());
        assert!(unpacked.join("install.sh").is_file());

        match result {
            Ok(output) => {
                let deb = output
                    .files
                    .iter()
                    .find(|path| path.extension().is_some_and(|ext| ext == "deb"))
                    .expect("Debian package path");
                let listing = Command::new("dpkg-deb")
                    .arg("-c")
                    .arg(deb)
                    .output()
                    .expect("inspect Debian package");
                assert!(listing.status.success());
                let stdout = String::from_utf8_lossy(&listing.stdout);
                assert!(stdout.contains("./usr/bin/Demo"));
                assert!(stdout.contains("./usr/share/applications/com.example.demo.desktop"));
                let arch_source = output_dir
                    .join("arch")
                    .join("com.example.demo-1.2.3.tar.gz");
                assert!(arch_source.is_file());
            }
            Err(error) => {
                let message = error.to_string();
                assert!(
                    message.contains("needs `dpkg-deb`") || message.contains("needs `makepkg`"),
                    "missing tool must be explicit: {message}"
                );
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn macos_dmg_needs_a_macos_host_and_preserves_skeleton() {
        let root = tempfile::tempdir().expect("temp root");
        let exe = executable(root.path());
        let output_dir = root.path().join("packages");
        let error = Packager::package_for(
            &config(),
            &exe,
            &output_dir,
            PackageTarget::MacOs,
            &PackageOptions::default(),
        )
        .expect_err("DMG must fail off macOS");
        assert!(error.to_string().contains("macOS host"));
        assert!(output_dir
            .join("macos/Demo.app/Contents/Info.plist")
            .is_file());
    }

    #[test]
    fn missing_tools_are_explicit() {
        let root = tempfile::tempdir().unwrap();
        let error = run_tool(
            "myrust-tool-that-cannot-exist",
            &[],
            None,
            "test operation",
            root.path(),
        )
        .expect_err("missing tool");
        assert!(error.to_string().contains("not found on PATH"));
    }

    #[test]
    fn windows_package_reports_missing_makensis_and_keeps_staging() {
        if find_tool("makensis").is_some() {
            eprintln!("skipping missing-makensis test: makensis is installed");
            return;
        }
        let root = tempfile::tempdir().expect("temp root");
        let exe = executable(root.path());
        let output_dir = root.path().join("packages");
        let error = Packager::package_for(
            &config(),
            &exe,
            &output_dir,
            PackageTarget::Windows,
            &PackageOptions::default(),
        )
        .expect_err("missing makensis");
        assert!(error.to_string().contains("needs `makensis`"));
        assert!(output_dir.join("windows/installer.nsi").is_file());
        assert!(output_dir.join("windows/app/Demo.exe").is_file());
    }

    #[test]
    fn nsis_installer_script_names_payload_and_output() {
        let text = windows_nsis(&config(), "Demo-1.2.3-setup.exe");
        assert!(text.contains("OutFile \"Demo-1.2.3-setup.exe\""));
        assert!(text.contains("File /r \"app\\*\""));
        assert!(text.contains("Demo.exe"));
        assert!(text.contains("MUI_UNPAGE_CONFIRM"));
        assert!(text.contains("MUI_UNPAGE_INSTFILES"));
    }

    #[test]
    fn packaging_tool_arguments_are_stable() {
        assert_eq!(
            dpkg_args(Path::new("debian/root"), Path::new("debian/demo.deb")),
            vec![
                "--build",
                "--root-owner-group",
                "debian/root",
                "debian/demo.deb"
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
        );
        assert_eq!(
            hdiutil_args(
                &config(),
                Path::new("Demo.app"),
                Path::new("Demo-1.2.3.dmg")
            ),
            vec![
                "create",
                "-volname",
                "Demo",
                "-srcfolder",
                "Demo.app",
                "-ov",
                "-format",
                "UDZO",
                "Demo-1.2.3.dmg"
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
        );
        assert_eq!(
            makensis_args(Path::new("installer.nsi")),
            vec!["/V2", "installer.nsi"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
    }

    #[cfg(unix)]
    #[test]
    fn generated_shell_files_parse() {
        let root = tempfile::tempdir().unwrap();
        let config = config();
        let install = root.path().join("install.sh");
        fs::write(&install, linux_install_script(&config)).unwrap();
        let pkgbuild = root.path().join("PKGBUILD");
        fs::write(&pkgbuild, arch_pkgbuild(&config, Some("deadbeef"))).unwrap();
        let Some(bash) = find_tool("bash") else {
            eprintln!("skipping shell syntax test: bash is not installed");
            return;
        };
        for script in [&install, &pkgbuild] {
            let status = Command::new(&bash)
                .arg("-n")
                .arg(script)
                .status()
                .expect("parse generated shell");
            assert!(status.success(), "{} did not parse", script.display());
        }
    }
}
