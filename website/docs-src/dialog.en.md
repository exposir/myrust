`dialog(c, "id", &mut open, width, content)` draws a centered modal panel over a scrim. The content closure returns `DialogAction::{None,Confirm,Cancel}`.

```rust
use myrust_core::ui::{dialog, DialogAction};

let response = dialog(c, "rename", &mut app.open, 320.0, |c| {
    if button(c, "Save").clicked { DialogAction::Confirm } else { DialogAction::None }
});
if response.confirmed {
    app.commit();
}
```

## The keyboard

Opening focuses the first focusable control in the panel; Tab/Shift+Tab are trapped there, Escape closes the dialog, and focus is restored to the opener. Outside pointer input is blocked by the scrim.

## Keeping it open

`open` is caller-owned. Returning `None` keeps it open; `Confirm`/`Cancel` close and report `confirmed`/`cancelled`. There is no async validation hook or “prevent close” callback.

## Beyond the window

The dialog is an in-window overlay, not an OS modal/sheet. It cannot extend beyond the window, create a separate native alert panel or block other applications.

## Without a look

There is no low-level modal builder, custom width/animation or separate title bar. `dialog` owns the scrim/panel; custom chrome needs an `overlay` with caller-drawn content.

## Accessibility

The panel emits `Role::Popup` with the dialog id; its title/body are `Role::Label`s, buttons are `Role::Button`. There is no Dialog/AlertDialog role, modal flag, focus containment metadata or AT bridge.
