An overlay is a popup anchored to a previous-frame rectangle. It paints after normal roots, owns the top layer, and blocks pointer input to layers below while open.

```rust
use myrust_core::ui::{overlay, text, OverlayState};

let anchor = myrust_core::ui::Rect::new(16.0, 16.0, 160.0, 28.0);
overlay(c, "popover", &mut app.popover, anchor, |c| {
    text(c, "Anchored content");
});
```

`OverlayState::open` is caller-owned. The overlay flips above the anchor when it would overflow the window and sizes itself to the anchor width (with a minimum).

## The keyboard

Opening an overlay assigns it a focused layer; Tab and Shift+Tab stay inside that layer. Escape closes it, and a pointer release outside the previous overlay bounds closes it. `OverlayResponse` reports `closed`, `escape_pressed`, `outside_clicked` and previous-frame `bounds`.

Focus is restored to the control that was focused before opening. While open, lower layers do not receive pointer input. There is no modal scrim inside the primitive itself; dialogs draw their own dimming layer.

## Your own overlays

`context.overlay(id, &mut state, anchor, content)` is the public primitive; `popup` is an alias. Dialogs are built on the same layer and add a centered panel, a scrim and a `DialogAction` result.

Custom overlay content is ordinary widgets. There is no arbitrary window stack, z-index, nested overlay depth API or non-anchored floating panel. An overlay is registered as `Role::Popup` with its id as the label; no live-region or alert role is emitted.
