//! A todo list whose page calls Rust through typed IPC. Rust owns the data
//! and persists it to disk; the page only renders and sends actions.

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use myrust_core::app::{App, WindowConfig};
use myrust_core::ipc::{IpcError, IpcType, NoRequest};
use myrust_core::webview::{PageSource, WebViewConfig};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
struct Todo {
    id: u64,
    title: String,
    done: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    next_id: u64,
    items: Vec<Todo>,
}

#[derive(Serialize, Deserialize)]
struct TodosResponse {
    items: Vec<Todo>,
}

impl IpcType for TodosResponse {
    const TS_NAME: &'static str = "TodosResponse";
    const TS_DECLARATION: &'static str =
        "export interface TodosResponse { items: Todo[] }";
}

#[derive(Serialize, Deserialize)]
struct AddRequest {
    title: String,
}

impl IpcType for AddRequest {
    const TS_NAME: &'static str = "AddRequest";
    const TS_DECLARATION: &'static str =
        "export interface AddRequest { title: string }";
}

#[derive(Serialize, Deserialize)]
struct IdRequest {
    id: u64,
}

impl IpcType for IdRequest {
    const TS_NAME: &'static str = "IdRequest";
    const TS_DECLARATION: &'static str = "export interface IdRequest { id: number }";
}

fn store_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("myrust-todo")
        .join("todos.json")
}

fn load() -> Store {
    fs::read_to_string(store_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save(store: &Store) {
    let path = store_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(store) {
        let _ = fs::write(path, text);
    }
}

fn response(store: &Store) -> TodosResponse {
    TodosResponse {
        items: store.items.clone(),
    }
}

const PAGE: &str = r##"<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Todos</title>
  <style>
    :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
    body { max-width: 560px; margin: 0 auto; padding: 32px 20px; }
    h1 { margin: 0 0 4px; }
    p.lede { margin: 0 0 24px; opacity: .65; }
    form { display: flex; gap: 8px; margin-bottom: 20px; }
    input[type=text] { flex: 1; padding: 9px 11px; border-radius: 8px; border: 1px solid #8884; font: inherit; }
    button { padding: 9px 14px; border-radius: 8px; border: 0; background: #2563eb; color: white; font: inherit; }
    ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
    li { display: flex; align-items: center; gap: 10px; padding: 10px 12px; border: 1px solid #8884; border-radius: 8px; }
    li span { flex: 1; }
    li.done span { text-decoration: line-through; opacity: .5; }
    li button { background: transparent; color: #ef4444; padding: 4px 8px; }
  </style>
</head>
<body>
  <h1>Todos</h1>
  <p class="lede">The page calls Rust. Rust owns the list and writes it to disk.</p>
  <form id="form">
    <input id="title" type="text" placeholder="What needs doing?" autocomplete="off">
    <button type="submit">Add</button>
  </form>
  <ul id="list"></ul>
  <script>
    const bridge = window.__myrust_ipc__;
    const list = document.getElementById("list");
    const title = document.getElementById("title");

    function render(items) {
      list.replaceChildren();
      for (const item of items) {
        const li = document.createElement("li");
        li.className = item.done ? "done" : "";
        const box = document.createElement("input");
        box.type = "checkbox";
        box.checked = item.done;
        box.addEventListener("change", async () => {
          const result = await bridge.invoke("todos.toggle", { id: item.id });
          render(result.items);
        });
        const span = document.createElement("span");
        span.textContent = item.title;
        const remove = document.createElement("button");
        remove.type = "button";
        remove.textContent = "✕";
        remove.addEventListener("click", async () => {
          const result = await bridge.invoke("todos.remove", { id: item.id });
          render(result.items);
        });
        li.append(box, span, remove);
        list.append(li);
      }
      if (items.length === 0) {
        const empty = document.createElement("li");
        empty.textContent = "Nothing yet.";
        empty.style.borderStyle = "dashed";
        list.append(empty);
      }
    }

    async function refresh() {
      const result = await bridge.invoke("todos.list");
      render(result.items);
    }

    document.getElementById("form").addEventListener("submit", async (event) => {
      event.preventDefault();
      const value = title.value.trim();
      if (!value) return;
      const result = await bridge.invoke("todos.add", { title: value });
      title.value = "";
      render(result.items);
    });

    refresh();
  </script>
</body>
</html>"##;

fn main() -> Result<()> {
    let mut app = App::new("myrust-todo")?;
    let store = Arc::new(Mutex::new(load()));

    let list_store = Arc::clone(&store);
    app.ipc_router().register::<NoRequest, TodosResponse, _>(
        "todos.list",
        "Returns every todo.",
        move |_| {
            let store = list_store
                .lock()
                .map_err(|_| IpcError::handler("todo store is poisoned"))?;
            Ok(response(&store))
        },
    )?;

    let add_store = Arc::clone(&store);
    app.ipc_router().register::<AddRequest, TodosResponse, _>(
        "todos.add",
        "Adds a todo and returns the new list.",
        move |request| {
            let mut store = add_store
                .lock()
                .map_err(|_| IpcError::handler("todo store is poisoned"))?;
            let title = request.title.trim().to_string();
            if title.is_empty() {
                return Err(IpcError::handler("title is empty"));
            }
            let id = store.next_id.max(1);
            store.next_id = id + 1;
            store.items.push(Todo {
                id,
                title,
                done: false,
            });
            save(&store);
            Ok(response(&store))
        },
    )?;

    let toggle_store = Arc::clone(&store);
    app.ipc_router().register::<IdRequest, TodosResponse, _>(
        "todos.toggle",
        "Toggles one todo and returns the list.",
        move |request| {
            let mut store = toggle_store
                .lock()
                .map_err(|_| IpcError::handler("todo store is poisoned"))?;
            if let Some(item) = store.items.iter_mut().find(|item| item.id == request.id) {
                item.done = !item.done;
                save(&store);
            }
            Ok(response(&store))
        },
    )?;

    let remove_store = Arc::clone(&store);
    app.ipc_router().register::<IdRequest, TodosResponse, _>(
        "todos.remove",
        "Removes one todo and returns the list.",
        move |request| {
            let mut store = remove_store
                .lock()
                .map_err(|_| IpcError::handler("todo store is poisoned"))?;
            store.items.retain(|item| item.id != request.id);
            save(&store);
            Ok(response(&store))
        },
    )?;

    app.on_ready(|context| {
        context.create_window(WindowConfig {
            title: "Todos".to_string(),
            width: 720.0,
            height: 640.0,
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
