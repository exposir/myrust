//! Code signing and macOS notarization through platform tools.
//!
//! Signing never falls back to success when a tool or credential is missing.
//! macOS uses `codesign` (or `rcodesign` when signing off a macOS host),
//! Windows uses `signtool` or `rcodesign`, and Linux uses `minisign`.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context, Result};

/// Target platform for signing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigningPlatform {
    /// Apple `codesign`, with `rcodesign` as a cross-host fallback.
    MacOs,
    /// Windows Authenticode through `signtool`, or `rcodesign`.
    Windows,
    /// Linux artifact signing through `minisign`.
    Linux,
}

/// Signing configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningConfig {
    /// Target platform.
    pub platform: SigningPlatform,
    /// Signing identity, certificate path, secret key, or key id.
    pub identity: String,
    /// Artifact to sign.
    pub artifact: PathBuf,
    /// Optional output path. When set, the artifact is copied there first and
    /// the copy is signed in place.
    pub output: Option<PathBuf>,
}

impl SigningConfig {
    /// Signs an artifact using the platform toolchain.
    ///
    /// For macOS, `codesign` is preferred on a macOS host. `rcodesign` is the
    /// explicit cross-host fallback. Windows tries `signtool`, then
    /// `rcodesign`. Linux invokes `minisign`.
    pub fn sign(&self) -> Result<()> {
        if !self.artifact.exists() {
            bail!(
                "signing artifact does not exist: {}",
                self.artifact.display()
            );
        }
        let artifact = prepare_output(&self.artifact, self.output.as_deref())?;

        match self.platform {
            SigningPlatform::MacOs => sign_macos(&artifact, &self.identity),
            SigningPlatform::Windows => sign_windows(&artifact, &self.identity),
            SigningPlatform::Linux => sign_linux(&artifact, &self.identity),
        }
    }

    /// Submits a signed macOS artifact for notarization and staples the ticket.
    ///
    /// This command is macOS-only because it invokes `xcrun notarytool` and
    /// `xcrun stapler`. Credentials come from `NOTARYTOOL_PROFILE`, or from
    /// `APPLE_ID`, `APPLE_TEAM_ID`, and `APPLE_APP_PASSWORD` (or
    /// `APPLE_PASSWORD`). Missing credentials are an explicit error; no
    /// credential is committed to source control.
    pub fn notarize(&self) -> Result<()> {
        if self.platform != SigningPlatform::MacOs {
            bail!("notarization is only applicable to macOS artifacts");
        }
        if !cfg!(target_os = "macos") {
            bail!("notarization needs a macOS host with xcrun notarytool");
        }

        let artifact = self
            .output
            .as_deref()
            .filter(|path| path.exists())
            .unwrap_or(&self.artifact);
        if !artifact.exists() {
            bail!(
                "notarization artifact does not exist: {}",
                artifact.display()
            );
        }
        let credentials = notarization_credentials_from_env()?;
        let notary_args = notarytool_args(artifact, &credentials);
        run_tool("xcrun", &notary_args, "macOS notarization", "notarytool")?;
        run_tool(
            "xcrun",
            &[
                "stapler".to_string(),
                "staple".to_string(),
                artifact.display().to_string(),
            ],
            "macOS notarization ticket stapling",
            "stapler",
        )
    }

    /// Returns the artifact path.
    pub fn artifact(&self) -> &Path {
        &self.artifact
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NotarizationCredentials {
    KeychainProfile(String),
    AppleId {
        apple_id: String,
        team_id: String,
        password: String,
    },
}

fn notarization_credentials_from_env() -> Result<NotarizationCredentials> {
    if let Some(profile) = nonempty_env("NOTARYTOOL_PROFILE") {
        return Ok(NotarizationCredentials::KeychainProfile(profile));
    }

    let apple_id = nonempty_env("APPLE_ID");
    let team_id = nonempty_env("APPLE_TEAM_ID");
    let password = nonempty_env("APPLE_APP_PASSWORD").or_else(|| nonempty_env("APPLE_PASSWORD"));
    match (apple_id, team_id, password) {
        (Some(apple_id), Some(team_id), Some(password)) => {
            Ok(NotarizationCredentials::AppleId {
                apple_id,
                team_id,
                password,
            })
        }
        _ => bail!(
            "notarization credentials are missing: set NOTARYTOOL_PROFILE or APPLE_ID, APPLE_TEAM_ID, and APPLE_APP_PASSWORD"
        ),
    }
}

fn nonempty_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn prepare_output(artifact: &Path, output: Option<&Path>) -> Result<PathBuf> {
    let Some(output) = output else {
        return Ok(artifact.to_path_buf());
    };
    if output == artifact {
        return Ok(output.to_path_buf());
    }
    copy_path(artifact, output).with_context(|| {
        format!(
            "failed to copy signing artifact {} to {}",
            artifact.display(),
            output.display()
        )
    })?;
    Ok(output.to_path_buf())
}

fn copy_path(source: &Path, destination: &Path) -> Result<()> {
    if source.is_dir() {
        if destination.exists() {
            fs::remove_dir_all(destination)?;
        }
        copy_directory(source, destination)
    } else {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
        Ok(())
    }
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if file_type.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(entry.path())?, target)?;
            #[cfg(not(unix))]
            fs::copy(entry.path(), target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn sign_macos(artifact: &Path, identity: &str) -> Result<()> {
    if cfg!(target_os = "macos") && find_tool("codesign").is_some() {
        let args = codesign_args(identity, artifact);
        return run_tool("codesign", &args, "macOS code signing", "codesign");
    }

    let args = rcodesign_args(identity, artifact);
    run_tool(
        "rcodesign",
        &args,
        "macOS code signing off a macOS host",
        "rcodesign",
    )
}

fn sign_windows(artifact: &Path, identity: &str) -> Result<()> {
    let signtool_args = signtool_args(identity, artifact);
    if find_tool("signtool").is_some() {
        return run_tool(
            "signtool",
            &signtool_args,
            "Windows Authenticode signing",
            "signtool",
        );
    }

    let args = rcodesign_args(identity, artifact);
    run_tool(
        "rcodesign",
        &args,
        "Windows Authenticode signing fallback",
        "rcodesign",
    )
}

fn sign_linux(artifact: &Path, identity: &str) -> Result<()> {
    let args = minisign_args(identity, artifact);
    run_tool("minisign", &args, "Linux artifact signing", "minisign")
}

fn codesign_args(identity: &str, artifact: &Path) -> Vec<String> {
    vec![
        "--force".to_string(),
        "--options".to_string(),
        "runtime".to_string(),
        "--timestamp".to_string(),
        "--sign".to_string(),
        identity.to_string(),
        artifact.display().to_string(),
    ]
}

fn signtool_args(identity: &str, artifact: &Path) -> Vec<String> {
    let identity_flag = if Path::new(identity).is_file() {
        "/f"
    } else {
        "/n"
    };
    vec![
        "sign".to_string(),
        "/fd".to_string(),
        "SHA256".to_string(),
        "/td".to_string(),
        "SHA256".to_string(),
        identity_flag.to_string(),
        identity.to_string(),
        artifact.display().to_string(),
    ]
}

fn rcodesign_args(identity: &str, artifact: &Path) -> Vec<String> {
    vec![
        "sign".to_string(),
        "--pkey".to_string(),
        identity.to_string(),
        artifact.display().to_string(),
    ]
}

fn minisign_args(identity: &str, artifact: &Path) -> Vec<String> {
    vec![
        "-S".to_string(),
        "-m".to_string(),
        artifact.display().to_string(),
        "-s".to_string(),
        identity.to_string(),
    ]
}

fn notarytool_args(artifact: &Path, credentials: &NotarizationCredentials) -> Vec<String> {
    let mut args = vec![
        "notarytool".to_string(),
        "submit".to_string(),
        artifact.display().to_string(),
        "--wait".to_string(),
    ];
    match credentials {
        NotarizationCredentials::KeychainProfile(profile) => {
            args.push("--keychain-profile".to_string());
            args.push(profile.clone());
        }
        NotarizationCredentials::AppleId {
            apple_id,
            team_id,
            password,
        } => {
            args.push("--apple-id".to_string());
            args.push(apple_id.clone());
            args.push("--team-id".to_string());
            args.push(team_id.clone());
            args.push("--password".to_string());
            args.push(password.clone());
        }
    }
    args
}

fn run_tool(tool: &str, args: &[String], operation: &str, tool_label: &str) -> Result<()> {
    let program = find_tool(tool).ok_or_else(|| {
        anyhow::anyhow!(
            "{operation} needs `{tool}`, but it was not found on PATH; install {tool_label} or configure an explicit tool path"
        )
    })?;
    let output = Command::new(&program)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute {} for {operation}", program.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        bail!("{operation} failed with {}: {detail}", output.status);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config(platform: SigningPlatform) -> SigningConfig {
        SigningConfig {
            platform,
            identity: "release-key".to_string(),
            artifact: PathBuf::from("artifact"),
            output: None,
        }
    }

    #[test]
    fn missing_artifacts_are_explicit() {
        let error = config(SigningPlatform::Linux)
            .sign()
            .expect_err("missing artifact");
        assert!(error.to_string().contains("does not exist"));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn macos_signing_reports_missing_cross_host_tool() {
        if find_tool("rcodesign").is_some() {
            eprintln!("skipping missing-rcodesign test: rcodesign is installed");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let artifact = root.path().join("Demo.app");
        fs::write(&artifact, b"bundle").unwrap();
        let error = SigningConfig {
            platform: SigningPlatform::MacOs,
            identity: "release.p12".to_string(),
            artifact,
            output: None,
        }
        .sign()
        .expect_err("missing rcodesign");
        assert!(error.to_string().contains("needs `rcodesign`"));
    }

    #[test]
    fn notarization_is_macos_only() {
        let error = config(SigningPlatform::Windows)
            .notarize()
            .expect_err("platform");
        assert!(error.to_string().contains("only applicable"));
    }

    #[test]
    fn constructs_platform_tool_arguments() {
        let artifact = Path::new("Demo.app");
        assert_eq!(
            codesign_args("Developer ID", artifact),
            vec![
                "--force",
                "--options",
                "runtime",
                "--timestamp",
                "--sign",
                "Developer ID",
                "Demo.app"
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
        );
        assert_eq!(
            rcodesign_args("release.p12", artifact),
            vec!["sign", "--pkey", "release.p12", "Demo.app"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            signtool_args("Example Code Signing", Path::new("Demo.exe")),
            vec![
                "sign",
                "/fd",
                "SHA256",
                "/td",
                "SHA256",
                "/n",
                "Example Code Signing",
                "Demo.exe"
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
        );
        assert_eq!(
            minisign_args("release.key", Path::new("Demo.tar.gz")),
            vec!["-S", "-m", "Demo.tar.gz", "-s", "release.key"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn notarytool_accepts_profile_or_apple_id_credentials() {
        let artifact = Path::new("Demo.dmg");
        let profile = notarytool_args(
            artifact,
            &NotarizationCredentials::KeychainProfile("release".to_string()),
        );
        assert!(profile.iter().any(|arg| arg == "--keychain-profile"));
        assert!(profile.iter().any(|arg| arg == "release"));

        let apple = notarytool_args(
            artifact,
            &NotarizationCredentials::AppleId {
                apple_id: "dev@example.com".to_string(),
                team_id: "TEAM".to_string(),
                password: "test-app-password".to_string(),
            },
        );
        assert!(apple.iter().any(|arg| arg == "--apple-id"));
        assert!(apple.iter().any(|arg| arg == "--team-id"));
        assert!(apple.iter().any(|arg| arg == "--password"));
    }

    #[test]
    fn missing_tool_is_explicit() {
        let error = run_tool(
            "myrust-signing-tool-that-cannot-exist",
            &[],
            "test signing",
            "the test tool",
        )
        .expect_err("missing tool");
        assert!(error.to_string().contains("not found on PATH"));
    }
}
