A view is a Rust closure from caller-owned state to widgets. `NativeUi::new` stores that closure and runs it for every frame; `Context` is recreated per frame, while state lives outside the closure.

```rust
use myrust_core::ui::{button, column, text, NativeUi};

let ui = NativeUi::new(|c| {
    let mut count = 0;
    column(c).padding(16.0).gap(8.0).children(|c| {
        if button(c, "Increment").clicked {
            count += 1;
        }
        text(c, &count.to_string());
    });
});
```

The example above is intentionally wrong as app state: `count` must live in the caller, commonly in `Rc<RefCell<_>>`, because the closure runs again for every frame. `button` reports the click after the pointer is released over the previous frame's bounds.

## Events and actions

Controls return input as part of constructing them: `ButtonState::clicked`, `TextFieldState::changed`, `CheckboxState::changed`. Actions should be taken inline, after the control, so they see the latest edits from controls built earlier.

```rust
if button(c, "Save").clicked {
    save(&app);
}
```

There is no deferred `OnClick` queue, no disabled-state shortcut and no action object. If an action must run later, the caller owns that scheduling outside the view.

## Keys

Widgets consume `Key::Tab`, `ShiftTab`, `Enter`, `Space`, `Backspace`, `Delete`, arrows, Home and End where relevant. `NativeUi::input(UiEvent::Key { key })` is the way a host delivers them.

`Context` does not expose a public “was this key pressed?” query for arbitrary custom drawing. Custom behavior should be built from existing focusable controls or handled by the application before input reaches `NativeUi`; modifier state, key repeat and key-up events are not modeled.

## Change the state from other goroutines

The view runs on the thread that calls `NativeUi::render` (or the window loop). `Context`, `NativeUi` and the captured state are `Rc`/`RefCell` based and are not `Send`; a view cannot be mutated from another thread.

There is no channel-draining or thread-safe state API. Cross-thread work must finish outside the UI, then be applied on the UI thread before the next frame—typically through the application's own event loop. Sharing a `Mutex` into a view is not supported by the current closure types.

## State of an element

Persistent state is caller-owned: `TabsState`, `TextFieldState`, `ListState`, `CalendarState` and similar values are returned each frame, and stable string ids key context-owned transient state. There is no `Element` handle, owner slot or generation check.

Ids must be unique and must not move between unrelated controls. `Context::focus(id)` and `Context::request_focus(id)` query and change focus by id; `Context::migrate_state(old, new)` moves transient state when a control is renamed.

## Check build lifetimes

Rust's borrow checker checks access during construction; there is no runtime element-lifetime diagnostic. A widget builder borrows `Context` mutably for the builder expression and its children closure, so holding a builder across another `Context` call does not compile.

The context does not retain a builder after the frame. Stale interaction is avoided with stable ids and context-owned previous-frame regions, not with generation counters. Use `NativeUi::render(size)` in tests to run a closure without a window and inspect the resulting `ViewFrame`.
