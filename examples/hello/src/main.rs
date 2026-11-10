//! Hello is the smallest MyRust app: one window and one typed Rust method
//! called from the page.

use anyhow::Result;
use myrust_core::app::{App, WindowConfig};
use myrust_core::ipc::{IpcType, NoRequest};
use myrust_core::webview::{PageSource, WebViewConfig};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct GreetResponse {
    message: String,
}

impl IpcType for GreetResponse {
    const TS_NAME: &'static str = "GreetResponse";
    const TS_DECLARATION: &'static str =
        "export interface GreetResponse { message: string }";
}

const PAGE: &str = r#"<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Hello MyRust</title>
  <style>
    :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
    body { height: 100vh; margin: 0; display: grid; place-content: center; text-align: center; }
    h1 { margin: 0 0 8px; }
    p { margin: 0; color: color-mix(in srgb, currentColor 65%, transparent); }
  </style>
</head>
<body>
  <h1>Hello MyRust</h1>
  <p id="greeting">…</p>
  <script>
    window.__myrust_ipc__.invoke("greet").then(function (response) {
      document.getElementById("greeting").textContent = response.message;
    }).catch(function (error) {
      document.getElementById("greeting").textContent = String(error);
    });
  </script>
</body>
</html>"#;

fn main() -> Result<()> {
    let mut app = App::new("myrust-hello")?;
    app.ipc_router().register::<NoRequest, GreetResponse, _>(
        "greet",
        "Returns a greeting from Rust.",
        |_| {
            Ok(GreetResponse {
                message: format!("Hello from Rust {}.", env!("CARGO_PKG_VERSION")),
            })
        },
    )?;

    app.on_ready(|context| {
        context.create_window(WindowConfig {
            title: "Hello MyRust".to_string(),
            width: 480.0,
            height: 320.0,
            webview: Some(WebViewConfig {
                source: PageSource::Html(PAGE.to_string()),
                devtools: false,
                ..WebViewConfig::default()
            }),
            ..WindowConfig::default()
        })?;
        Ok(())
    });

    app.run()
}
