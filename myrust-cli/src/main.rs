//! `myrust` command-line interface.
//!
//! Phase 1 provides `myrust dev` and `myrust build` for the dashboard example.

use std::{
    env,
    io::ErrorKind,
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};

const DASHBOARD_URL: &str = "http://localhost:8090";
const DASHBOARD_HOST: &str = "127.0.0.1:8090";

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("dev") => run_dev(),
        Some("build") => run_build(),
        Some("-h" | "--help") => {
            print_help();
            Ok(())
        }
        Some(command) => bail!("unknown command `{command}`\n\n{}", help_text()),
        None => {
            print_help();
            Ok(())
        }
    }
}

/// Builds the dashboard release binary and refreshes its TypeScript client.
fn run_build() -> Result<()> {
    let root = workspace_root()?;
    let status = cargo(&root)
        .args(["build", "--release", "-p", "dashboard"])
        .status()
        .context("failed to execute cargo build")?;
    if !status.success() {
        bail!("cargo build failed with status {status}");
    }

    let dashboard = root
        .join("target")
        .join("release")
        .join(binary_name("dashboard"));
    generate_types(&dashboard, &root)?;

    println!("release binary: {}", dashboard.display());
    println!(
        "type declarations: {}",
        root.join("frontend/myrust.d.ts").display()
    );
    Ok(())
}

/// Starts the dashboard web dev server and opens the default browser.
fn run_dev() -> Result<()> {
    let root = workspace_root()?;
    refresh_types_with_cargo(&root)?;

    let mut child = cargo(&root)
        .args(["run", "-p", "dashboard", "--", "--web", "--dev"])
        .env("MYRUST_DEV", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to start dashboard dev server")?;

    wait_for_server(&mut child)?;
    if let Err(error) = open_browser(DASHBOARD_URL) {
        eprintln!("warning: could not open browser automatically: {error}");
    }
    println!("Dashboard dev server: {DASHBOARD_URL}");

    let status = child.wait().context("failed to wait for dev server")?;
    if !status.success() {
        bail!("dashboard dev server exited with status {status}");
    }
    Ok(())
}

/// Generates dashboards types by running the example's generation entry point.
fn refresh_types_with_cargo(root: &Path) -> Result<()> {
    let status = cargo(root)
        .args(["run", "--quiet", "-p", "dashboard", "--", "--generate-ts"])
        .status()
        .context("failed to generate TypeScript IPC declarations")?;
    if !status.success() {
        bail!("dashboard type generation failed with status {status}");
    }
    Ok(())
}

/// Runs the already-built dashboard binary to generate declarations.
fn generate_types(binary: &Path, root: &Path) -> Result<()> {
    if !binary.exists() {
        bail!("dashboard binary does not exist: {}", binary.display());
    }

    let status = Command::new(binary)
        .arg("--generate-ts")
        .current_dir(root)
        .status()
        .with_context(|| format!("failed to run {}", binary.display()))?;
    if !status.success() {
        bail!("type generation failed with status {status}");
    }
    Ok(())
}

/// Waits until the dashboard HTTP server accepts connections.
fn wait_for_server(child: &mut Child) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(90);
    let address: SocketAddr = DASHBOARD_HOST
        .parse()
        .expect("dashboard address is a valid socket address");

    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().context("failed to query dev server")? {
            bail!("dev server exited before becoming ready: {status}");
        }

        match TcpStream::connect_timeout(&address, Duration::from_millis(300)) {
            Ok(_) => return Ok(()),
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::ConnectionRefused | ErrorKind::TimedOut
                ) =>
            {
                thread::sleep(Duration::from_millis(300));
            }
            Err(error) => {
                return Err(error).context("failed while waiting for dev server");
            }
        }
    }

    bail!("timed out waiting for dashboard dev server")
}

/// Opens the dashboard URL with the platform default browser.
fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to launch browser for {url}"))?;
    Ok(())
}

/// Returns the Cargo executable for this environment.
fn cargo(root: &Path) -> Command {
    let mut command = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()));
    command.current_dir(root);
    command
}

/// Resolves the workspace root from the CLI crate location.
fn workspace_root() -> Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .context("CLI manifest has no workspace parent")
}

/// Returns the platform executable filename for a workspace binary.
fn binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Prints CLI usage.
fn print_help() {
    println!("{}", help_text());
}

/// Returns CLI usage text.
fn help_text() -> &'static str {
    "\
myrust Phase 1

Usage:
  myrust dev      Start the dashboard web dev server and open a browser
  myrust build    Build the dashboard release binary and refresh myrust.d.ts
  myrust --help   Show this help
"
}
