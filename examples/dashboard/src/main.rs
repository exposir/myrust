//! Dashboard example for myrust.
//!
//! The default mode opens a native WebView window and serves stats through
//! typed IPC. `--web` instead serves the same Dashboard over HTTP on port 8090.

use std::{
    cell::RefCell,
    env,
    net::SocketAddr,
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
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
    app::{
        file_watcher::{FileWatcher, WatchOptions},
        recent_documents::RecentDocuments,
        window_state::JsonFileStore,
        App, WindowConfig, WindowContent,
    },
    desktop::menu::{Menu, MenuBuilder},
    ipc::{IpcError, IpcRouter, IpcType, NoRequest},
    release::packager::{PackageConfig, Packager},
    ui::{
        button, checkbox, column, row, scroll_view, slider, spacer, text, text_field, Color,
        DrawCommand, NativeUi, Radius, RendererKind, TextAlignment, Theme,
    },
    webview::{FileDropKind, PageOptions, PageSource, Session, SessionKind, WebViewConfig},
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

    if wants_native_ui(&args) {
        if wants_web_mode(&args) {
            anyhow::bail!("--native-ui cannot be combined with --web or --dev");
        }
        run_native_ui_demo()
    } else if wants_web_mode(&args) {
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

    if args.iter().any(|argument| argument == "--package-describe") {
        let output = Packager::generate(
            &PackageConfig {
                app_id: "myrust-dashboard".to_string(),
                name: "myrust Dashboard".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                author: "myrust".to_string(),
                identifier: "dev.myrust.dashboard".to_string(),
                resources: Vec::new(),
                icons: Vec::new(),
            },
            frontend_dir().join("../target/package"),
        )?;
        println!("package description files: {}", output.files.len());
        for file in output.files {
            println!("{}", file.display());
        }
        return Ok(());
    }

    if env::var_os("MYRUST_DEV").is_some() {
        app.watch_path(frontend_dir());
    }

    let state_store = Arc::new(JsonFileStore::new("myrust-dashboard")?);
    app.restore_window_state(state_store);
    let recent_documents = Arc::new(RecentDocuments::new("myrust-dashboard")?);
    let watcher = FileWatcher::watch_with(
        frontend_dir(),
        WatchOptions {
            recursive: true,
            hints_only: true,
        },
    )?;
    std::thread::spawn(move || {
        while let Ok(Some(event)) = watcher.recv() {
            tracing::debug!(?event, "dashboard asset changed");
        }
    });
    app.register_deep_link("myrust-dashboard")?;
    app.on_open_url(move |context, url| {
        tracing::info!(%url, "deep link opened");
        if let Some(window) = context.windows().first() {
            context.set_window_title(window.id, format!("myrust Dashboard - {url}"))?;
        }
        Ok(())
    });

    let menu = build_app_menu()?;
    app.set_app_menu(menu);
    app.on_menu_event(|context, event| {
        match event.id.as_str() {
            "refresh" => {
                for window in context.windows() {
                    context.page(window.id)?.reload()?;
                }
            }
            "about" => {
                tracing::info!("myrust Dashboard Phase 2");
            }
            "fit" => {
                if let Some(window) = context.windows().first() {
                    context.fit_content(window.id)?;
                }
            }
            "quit" => context.quit(),
            _ => {}
        }
        Ok(())
    });
    let recent_documents_for_drop = recent_documents.clone();
    app.on_file_drop(move |context, window_id, event| {
        if event.kind == FileDropKind::Dropped {
            if let Some(path) = event.paths.first() {
                let _ = recent_documents_for_drop.add(path);
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("dropped file");
                context.set_window_title(window_id, format!("myrust Dashboard - {name}"))?;
            }
        }
        Ok(())
    });

    let session = Session::new(
        "myrust-dashboard",
        SessionKind::Persistent("dashboard-main".to_string()),
    )?;
    app.on_ready(move |context| {
        let html = embedded_index()?;
        let config = WindowConfig {
            title: "myrust Dashboard".to_string(),
            width: 1200.0,
            height: 860.0,
            min_size: Some((760.0, 600.0)),
            window_key: Some("dashboard-main".to_string()),
            webview: Some(WebViewConfig {
                source: PageSource::Html(html),
                devtools: cfg!(debug_assertions),
                on_file_drop: Some(Arc::new(|event| {
                    if event.kind == FileDropKind::Dropped {
                        tracing::info!(
                            paths = ?event.paths,
                            position = ?event.position,
                            "WebView file drop"
                        );
                    }
                })),
                page_options: PageOptions {
                    devtools: cfg!(debug_assertions),
                    ..PageOptions::default()
                },
                session: Some(session.clone()),
                ..WebViewConfig::default()
            }),
            ..WindowConfig::default()
        };
        context.create_window(config)?;
        Ok(())
    });

    app.run()
}

struct NativeDashboardState {
    stats_state: StatsState,
    snapshot: Stats,
    clicks: u32,
    typed: String,
    scroll: f32,
    checked: bool,
    slider_value: f32,
}

impl NativeDashboardState {
    fn new() -> Self {
        let stats_state = StatsState::new();
        let snapshot = build_stats(&stats_state);
        Self {
            stats_state,
            snapshot,
            clicks: 0,
            typed: String::new(),
            scroll: 0.0,
            checked: true,
            slider_value: 42.0,
        }
    }

    fn refresh(&mut self) {
        self.snapshot = build_stats(&self.stats_state);
    }
}

fn ring_metric(
    c: &mut myrust_core::ui::Context,
    label: &str,
    percent: f32,
    accent: Color,
    theme: &Theme,
) {
    let percent = percent.clamp(0.0, 100.0);
    let mut commands = vec![
        DrawCommand::Circle {
            x: 66.0,
            y: 66.0,
            radius: 51.0,
            color: theme.border,
        },
        DrawCommand::Circle {
            x: 66.0,
            y: 66.0,
            radius: 42.0,
            color: theme.surface,
        },
    ];
    let sweep = percent / 100.0 * std::f32::consts::TAU;
    for segment in 0..48 {
        let start = -std::f32::consts::FRAC_PI_2 + sweep * segment as f32 / 48.0;
        let end = -std::f32::consts::FRAC_PI_2 + sweep * (segment + 1) as f32 / 48.0;
        commands.push(DrawCommand::Line {
            x1: 66.0 + 46.0 * start.cos(),
            y1: 66.0 + 46.0 * start.sin(),
            x2: 66.0 + 46.0 * end.cos(),
            y2: 66.0 + 46.0 * end.sin(),
            width: 7.0,
            color: accent,
        });
    }
    commands.push(DrawCommand::Text {
        x: 42.0,
        y: 51.0,
        text: format!("{percent:.0}%"),
        font_size: 22.0,
        weight: 600,
        color: theme.foreground,
        max_width: None,
        alignment: TextAlignment::Start,
    });
    commands.push(DrawCommand::Text {
        x: 29.0,
        y: 88.0,
        text: label.to_string(),
        font_size: 12.0,
        weight: 500,
        color: theme.muted,
        max_width: None,
        alignment: TextAlignment::Start,
    });
    column(c).size(132.0, 132.0).draw(commands);
}

fn bar_metric(
    c: &mut myrust_core::ui::Context,
    label: &str,
    percent: f32,
    accent: Color,
    theme: &Theme,
) {
    let percent = percent.clamp(0.0, 100.0);
    let commands = vec![
        DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width: 620.0,
            height: 46.0,
            radius: Radius::all(9.0),
            color: theme.surface,
        },
        DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width: (620.0 * percent / 100.0).max(8.0),
            height: 46.0,
            radius: Radius::all(9.0),
            color: accent,
        },
        DrawCommand::Text {
            x: 14.0,
            y: 13.0,
            text: label.to_string(),
            font_size: 14.0,
            weight: 600,
            color: theme.foreground,
            max_width: None,
            alignment: TextAlignment::Start,
        },
        DrawCommand::Text {
            x: 550.0,
            y: 13.0,
            text: format!("{percent:.0}%"),
            font_size: 14.0,
            weight: 500,
            color: theme.foreground,
            max_width: None,
            alignment: TextAlignment::Start,
        },
    ];
    column(c).size(620.0, 46.0).draw(commands);
}

fn run_native_ui_demo() -> Result<()> {
    let mut app = App::new("myrust-dashboard-native")?;
    let state = Rc::new(RefCell::new(NativeDashboardState::new()));
    let render_state = state.clone();
    let system_theme = Theme::system();
    let callback_theme = system_theme.clone();
    let native = NativeUi::new(move |c: &mut myrust_core::ui::Context| {
        let theme = callback_theme.clone();
        let spacing = theme.spacing;
        let mut state = render_state.borrow_mut();
        let cpu = state.snapshot.cpu.percent as f32;
        let memory = state.snapshot.memory.percent as f32;
        let disk_one = state
            .snapshot
            .disks
            .first()
            .map(|disk| f32::from(disk.percent))
            .unwrap_or(0.0);
        let disk_two = state
            .snapshot
            .disks
            .get(1)
            .map(|disk| f32::from(disk.percent))
            .unwrap_or(0.0);
        let size = c.available_size();
        c.container()
            .fill()
            .draw([DrawCommand::Rect {
                x: 0.0,
                y: 0.0,
                width: size.width,
                height: size.height,
                color: theme.background,
            }])
            .children(|c| {
                column(c).padding(spacing.lg).gap(spacing.md).children(|c| {
                    text(c, "MYRUST NATIVE DASHBOARD")
                        .font_size(theme.font_sizes.title)
                        .bold()
                        .color(theme.foreground);
                    text(
                        c,
                        &format!(
                            "{} / {} / refreshed {} time(s)",
                            state.snapshot.hostname, state.snapshot.os, state.clicks
                        ),
                    )
                    .font_size(theme.font_sizes.small)
                    .color(theme.muted);
                    row(c).gap(spacing.lg).children(|c| {
                        ring_metric(c, "CPU", cpu, theme.accent, &theme);
                        ring_metric(c, "MEMORY", memory, Color::rgb(251, 191, 36), &theme);
                        spacer(c);
                    });
                    bar_metric(c, "Macintosh HD", disk_one, theme.accent, &theme);
                    bar_metric(c, "Data", disk_two, Color::rgb(244, 114, 182), &theme);
                    text(c, "TEXT FIELD")
                        .font_size(theme.font_sizes.small)
                        .color(theme.muted);
                    text_field(c, "dashboard-text", &mut state.typed);
                    checkbox(c, "dashboard-checkbox", &mut state.checked);
                    text(c, &format!("SLIDER {:.0}", state.slider_value))
                        .font_size(theme.font_sizes.small)
                        .color(theme.muted);
                    slider(c, "dashboard-slider", &mut state.slider_value, 0.0, 100.0);
                    text(c, "SCROLL VIEW")
                        .font_size(theme.font_sizes.small)
                        .color(theme.muted);
                    let scroll = scroll_view(c, "dashboard-scroll", 150.0, |c| {
                        column(c).height(320.0).gap(spacing.sm).children(|c| {
                            for index in 0..12 {
                                text(
                                    c,
                                    &format!("Scroll row {index:02} - wheel and drag the bar"),
                                )
                                .font_size(theme.font_sizes.body)
                                .color(theme.foreground);
                            }
                        });
                    });
                    state.scroll = scroll.scroll;
                    c.id("refresh-stats");
                    let refresh = button(
                        c,
                        &format!(
                            "Refresh snapshot{}",
                            if state.clicks == 0 {
                                String::new()
                            } else {
                                format!(" ({})", state.clicks)
                            }
                        ),
                    );
                    if refresh.clicked {
                        state.clicks += 1;
                        state.refresh();
                        tracing::info!(clicks = state.clicks, "native dashboard refreshed");
                    }
                    text(
                        c,
                        &format!(
                            "text: {}    scroll: {:.0}    slider: {:.0}",
                            if state.typed.is_empty() {
                                "<type on the keyboard>"
                            } else {
                                state.typed.as_str()
                            },
                            state.scroll,
                            state.slider_value
                        ),
                    )
                    .font_size(theme.font_sizes.small)
                    .color(theme.muted);
                });
            });
    })
    .with_renderer_kind(RendererKind::Software)
    .with_theme(system_theme);

    app.on_ready(move |context| {
        let id = context.create_window(WindowConfig {
            title: "myrust Native Dashboard".to_string(),
            width: 1024.0,
            height: 820.0,
            min_size: Some((760.0, 620.0)),
            window_key: Some("native-dashboard".to_string()),
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        tracing::info!(
            backend = ?context.native_ui_backend(id)?,
            stats = ?context.native_ui_stats(id)?,
            "native dashboard window created"
        );
        Ok(())
    });
    app.run()
}

fn build_app_menu() -> Result<Menu> {
    MenuBuilder::new()
        .add_submenu("Dashboard", |menu| {
            menu.add_item("refresh", "Refresh Stats")
                .add_separator()
                .add_item("fit", "Fit Content")
                .add_item("about", "About")
                .add_item("quit", "Quit")
        })
        .build()
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

fn wants_native_ui(args: &[String]) -> bool {
    args.iter().any(|argument| argument == "--native-ui")
        || env::var("MYRUST_MODE")
            .map(|mode| mode == "native-ui")
            .unwrap_or(false)
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("dashboard=info,myrust_core=info,tower_http=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
