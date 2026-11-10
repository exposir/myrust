//! Native drag and drop: files from the desktop land on the WebView, and
//! Rust pushes their paths into the page.

use anyhow::Result;
use myrust_core::app::{App, WindowConfig};
use myrust_core::webview::{FileDropKind, PageSource, WebViewConfig};

const PAGE: &str = r##"<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Drag and drop</title>
  <style>
    :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
    body { margin: 0; min-height: 100vh; display: grid; place-content: center; gap: 18px; padding: 28px; box-sizing: border-box; }
    #zone { width: 420px; min-height: 220px; display: grid; place-content: center; text-align: center; border: 2px dashed #8886; border-radius: 14px; transition: border-color .15s, background .15s; }
    #zone.over { border-color: #2563eb; background: #2563eb18; }
    h1 { margin: 0 0 6px; font-size: 22px; }
    p { margin: 0; opacity: .65; }
    ul { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 6px; }
    li { padding: 8px 10px; border: 1px solid #8884; border-radius: 8px; font: 13px ui-monospace, monospace; word-break: break-all; }
  </style>
</head>
<body>
  <div id="zone">
    <h1>Drop files here</h1>
    <p>Rust receives the native drop event and updates this page.</p>
  </div>
  <ul id="files"></ul>
  <script>
    const zone = document.getElementById("zone");
    const files = document.getElementById("files");

    window.setDragging = function (on) {
      zone.classList.toggle("over", Boolean(on));
    };

    window.showDropped = function (paths) {
      zone.classList.remove("over");
      files.replaceChildren();
      for (const path of paths) {
        const item = document.createElement("li");
        item.textContent = path;
        files.append(item);
      }
    };
  </script>
</body>
</html>"##;

fn main() -> Result<()> {
    let mut app = App::new("myrust-drag-drop")?;

    app.on_file_drop(|context, window_id, event| {
        match event.kind {
            FileDropKind::Entered | FileDropKind::Over => {
                context.evaluate_script(window_id, "window.setDragging(true)")?;
            }
            FileDropKind::Cancelled => {
                context.evaluate_script(window_id, "window.setDragging(false)")?;
            }
            FileDropKind::Dropped => {
                let paths: Vec<String> = event
                    .paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect();
                let payload = serde_json::to_string(&paths)?;
                context.evaluate_script(
                    window_id,
                    &format!("window.showDropped({payload})"),
                )?;
            }
        }
        Ok(())
    });

    app.on_ready(|context| {
        context.create_window(WindowConfig {
            title: "Drag and drop".to_string(),
            width: 620.0,
            height: 480.0,
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
