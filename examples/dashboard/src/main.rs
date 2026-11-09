//! Dashboard example for myrust.
//!
//! The default mode opens a native WebView window and serves stats through
//! typed IPC. `--web` instead serves the same Dashboard over HTTP on port 8090.

use std::{
    env,
    net::SocketAddr,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use myrust_core::{
    app::{App, WindowConfig},
    ipc::{IpcError, IpcRouter, IpcType, NoRequest},
    webview::{PageSource, WebViewConfig},
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::EnvFilter;

const CPU_CORES: usize = 8;
const MEMORY_TOTAL_GB: f64 = 16.0;
const WEB_ADDR: &str = "0.0.0.0:8090";

#[derive(RustEmbed)]
#[folder = "../../frontend"]
struct FrontendAssets;

/// Runtime state shared by HTTP and IPC handlers.
#[derive(Clone)]
struct StatsState {
    started_at: Instant,
}

impl StatsState {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

/// Dashboard system statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stats {
    cpu: CpuStats,
    memory: MemoryStats,
    disks: Vec<DiskStats>,
    hostname: String,
    os: String,
    uptime: String,
    time: String,
}

/// CPU load across eight mock cores.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CpuStats {
    percent: f64,
    core_pcts: Vec<u8>,
}

/// Mock memory usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoryStats {
    percent: f64,
    used_gb: String,
    total_gb: String,
}

/// Mock disk usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiskStats {
    name: String,
    used_gb: String,
    total_gb: String,
    percent: u8,
}

impl IpcType for Stats {
    const TS_NAME: &'static str = "Stats";
    const TS_DECLARATION: &'static str = r#"
export interface CpuStats {
  percent: number;
  corePcts: number[];
}

export interface MemoryStats {
  percent: number;
  usedGb: string;
  totalGb: string;
}

export interface DiskStats {
  name: string;
  usedGb: string;
  totalGb: string;
  percent: number;
}

export interface Stats {
  cpu: CpuStats;
  memory: MemoryStats;
  disks: DiskStats[];
  hostname: string;
  os: string;
  uptime: string;
  time: string;
}
"#;
}

fn main() -> Result<()> {
    init_tracing();
    let args = env::args().collect::<Vec<_>>();

    if wants_web_mode(&args) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("failed to create Tokio runtime")?;
        runtime.block_on(run_web(args))
    } else {
        run_desktop(&args)
    }
}

fn run_desktop(args: &[String]) -> Result<()> {
    let stats_state = StatsState::new();
    let mut app = App::new("myrust-dashboard")?;
    register_ipc(app.ipc_router(), stats_state)?;

    if args.iter().any(|argument| argument == "--generate-ts") {
        let output = frontend_dir().join("myrust.d.ts");
        app.generate_typescript(&output)?;
        println!("type declarations: {}", output.display());
        return Ok(());
    }

    if env::var_os("MYRUST_DEV").is_some() {
        app.watch_path(frontend_dir());
    }

    app.on_ready(|context| {
        let html = embedded_index()?;
        let config = WindowConfig {
            title: "myrust Dashboard".to_string(),
            width: 1200.0,
            height: 860.0,
            min_size: Some((760.0, 600.0)),
            webview: Some(WebViewConfig {
                source: PageSource::Html(html),
                devtools: cfg!(debug_assertions),
            }),
            ..WindowConfig::default()
        };
        context.create_window(config)?;
        Ok(())
    });

    app.run()
}

fn register_ipc(router: &IpcRouter, state: StatsState) -> Result<(), IpcError> {
    router.register::<NoRequest, Stats, _>(
        "get_stats",
        "Returns the latest mock system statistics.",
        move |_| Ok(build_stats(&state)),
    )
}

async fn run_web(args: Vec<String>) -> Result<()> {
    let dev =
        env::var_os("MYRUST_DEV").is_some() || args.iter().any(|argument| argument == "--dev");
    let state = StatsState::new();

    let base = Router::new().route("/api/stats", get(web_stats));
    let app = if dev {
        base.fallback_service(ServeDir::new(frontend_dir()))
    } else {
        base.fallback(serve_embedded)
    }
    .layer(TraceLayer::new_for_http())
    .with_state(state);

    let address: SocketAddr = WEB_ADDR.parse().expect("valid dashboard address");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .context("failed to bind dashboard web server")?;
    tracing::info!("Dashboard web mode listening on http://{address}");

    axum::serve(listener, app)
        .await
        .context("dashboard web server failed")
}

async fn web_stats(State(state): State<StatsState>) -> Json<Stats> {
    Json(build_stats(&state))
}

async fn serve_embedded(uri: Uri) -> Response {
    let requested = uri.path().trim_start_matches('/');
    let path = if requested.is_empty() {
        "index.html"
    } else {
        requested
    };

    let Some(file) = FrontendAssets::get(path) else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };

    (
        [(header::CONTENT_TYPE, content_type(path))],
        file.data.into_owned(),
    )
        .into_response()
}

fn build_stats(state: &StatsState) -> Stats {
    let core_pcts = (0..CPU_CORES)
        .map(|_| (5 + random_u32() % 51) as u8)
        .collect::<Vec<_>>();
    let cpu_percent = ((core_pcts.iter().map(|value| u32::from(*value)).sum::<u32>() as f64
        / CPU_CORES as f64)
        * 10.0)
        .round()
        / 10.0;

    let raw_memory_used = 3.2 + (random_u32() % 4001) as f64 / 1000.0;
    let memory_used = (raw_memory_used * 10.0).round() / 10.0;
    let memory_percent = (memory_used / MEMORY_TOTAL_GB * 1000.0).round() / 10.0;

    Stats {
        cpu: CpuStats {
            percent: cpu_percent,
            core_pcts,
        },
        memory: MemoryStats {
            percent: memory_percent,
            used_gb: format!("{memory_used:.1}"),
            total_gb: format!("{:.0}", MEMORY_TOTAL_GB),
        },
        disks: vec![
            DiskStats {
                name: "Macintosh HD".to_string(),
                used_gb: "182".to_string(),
                total_gb: "256".to_string(),
                percent: 71,
            },
            DiskStats {
                name: "Data".to_string(),
                used_gb: "206".to_string(),
                total_gb: "512".to_string(),
                percent: 40,
            },
        ],
        hostname: hostname(),
        os: format!("{} {}", env::consts::OS, env::consts::ARCH),
        uptime: format_uptime(state.started_at.elapsed()),
        time: current_time(),
    }
}

fn random_u32() -> u32 {
    static RNG_STATE: AtomicU64 = AtomicU64::new(0);

    let mut state = RNG_STATE.load(Ordering::Relaxed);
    if state == 0 {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15)
            ^ 0x9E37_79B9_7F4A_7C15;

        match RNG_STATE.compare_exchange(0, seed, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => state = seed,
            Err(actual) => state = actual,
        }
    }

    loop {
        let next = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);

        match RNG_STATE.compare_exchange_weak(state, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return (next >> 32) as u32,
            Err(actual) => state = actual,
        }
    }
}

fn hostname() -> String {
    env::var("HOSTNAME")
        .or_else(|_| env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "myrust-demo".to_string())
}

fn format_uptime(elapsed: std::time::Duration) -> String {
    let total_seconds = elapsed.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds / 60) % 60;
    let seconds = total_seconds % 60;
    format!("{hours}h {minutes}m {seconds}s")
}

fn current_time() -> String {
    let total_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hours = (total_seconds / 3600) % 24;
    let minutes = (total_seconds / 60) % 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn frontend_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../frontend")
}

fn embedded_index() -> Result<String> {
    let file = FrontendAssets::get("index.html").context("frontend index.html is missing")?;
    String::from_utf8(file.data.into_owned()).context("frontend index.html is not UTF-8")
}

fn content_type(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".js") || path.ends_with(".ts") {
        "text/javascript; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

fn wants_web_mode(args: &[String]) -> bool {
    args.iter()
        .any(|argument| argument == "--web" || argument == "--dev")
        || env::var("MYRUST_MODE")
            .map(|mode| mode == "web")
            .unwrap_or(false)
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("dashboard=info,myrust_core=info,tower_http=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
