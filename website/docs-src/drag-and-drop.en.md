Drag and drop is an in-window interaction, not an operating-system session. The caller owns a `DragState` with `payload`, `origin` and `current`; a source paints a ghost, and a target reports the completed drop.

```rust
use myrust_core::ui::{drag_source, drop_target, DragState};

drag_source(c, "card", "Drag me", "card-42", &mut app.drag);
if let Some(drop) = drop_target(c, "bin", "Drop here", &mut app.drag).dropped {
    app.items.retain(|item| item != &drop.payload);
}
```

The payload is a `String`. There is no typed payload, drag handle, hover-delay or automatic scroll while dragging.

## Reordering

`reorder_list` renders rows whose drag reports source/target indices; `reorder` applies the move to caller data. `ListReorderState` owns the `DragState` plus `from`/`to`.

```rust
let result = reorder_list(c, "rows", &mut app.reorder, &rows, 26.0);
if result.dropped {
    if let (Some(from), Some(to)) = (result.from, result.to) {
        reorder(&mut app.rows, from, to);
    }
}
```

The list is not virtualized in this helper, does not paint a row ghost, and only reports indices; the caller remains responsible for data order.

## Between windows and applications

Native UI does not start or receive OS drag sessions. A `DragState` never leaves the process; there is no native pasteboard, drop target registration or cross-window drag.

WebView windows can receive file drops (`FileDropEvent`/`FileDropHandler` in `myrust-core`), but that path is separate from native UI drag and drop.

### Representations and lazy providers

There is no representation negotiation or lazy item provider. A drag carries one `String` payload that the caller constructs up front.

For richer values, put an id in the payload and look up the object in application state. There is no MIME type, file promise, image thumbnail or remote fetch during drag.

### Copy, move, completion and cancellation

The target only reports `DragDrop { target, origin, payload }` on release. Modifier-driven copy/move, drop effects, completion handshakes and cancellation are not modeled.

A release outside a target leaves the state to be cleared on the next idle frame; there is no “drag cancelled” callback. The caller can compare origin/target and decide whether to copy or move.

### Native format mapping

No native drag formats are mapped. Native UI cannot export an OS file, URI, image or text selection; only the `String` payload exists inside the window.

For OS interoperability use a WebView surface, the clipboard APIs of the host platform, or an application-specific bridge outside `myrust-ui`.
