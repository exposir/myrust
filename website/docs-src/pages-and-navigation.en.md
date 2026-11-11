A router is a caller-owned history of page ids plus a set of page closures. It is not a URL router and it does not create windows; it chooses one closure to build inside the current view.

```rust
use myrust_core::ui::{back_forward_buttons, router, text, Context, Page, RouterState};

fn home_page(c: &mut Context) { text(c, "Home"); }
fn about_page(c: &mut Context) { text(c, "About"); }

let pages = [
    Page::new("home", &home_page),
    Page::new("about", &about_page),
];
let buttons = back_forward_buttons(c, "nav-buttons", &mut app.router);
router(c, "pages", &mut app.router, &pages);
```

## Going to pages

`RouterState::push(page)` truncates forward history and appends the page; `RouterState::current()` returns the active id. `router` finds a matching `Page` and calls its `render` closure with the same `Context`, or draws “Page not found”.

There is no URL parsing, route parameters, query strings or external navigation. Push/pop is entirely application-driven; a page can push another page from its own controls.

## Back and forward

`RouterState::back()` and `forward()` move the index when possible; `can_back` / `can_forward` report availability. `back_forward_buttons` renders two Button nodes labelled `back` and `forward`, with value `enabled` or `disabled`.

There is no browser-style history API, no page-id stack exposed for editing besides `history()`, and no browser back gesture. A disabled history button is painted muted but still emits an AccessNode with `disabled` value; it is not keyboard-interactive.

## Layouts

The router has no layout objects or page transition layout. Each page closure builds whatever containers it needs; the router root is a `Group` with the current page id as its label.

Switching pages may completely change the layout on the next frame. There is no retained page tree, no scroll restoration and no route-level animation curve. Use `transition` if a caller wants explicit rect animation between two states.

## Pages keep their state

`RouterState` stores ids, not page state. State shared by pages lives in the caller and survives navigation; per-page state must be keyed by page id in application state. Nothing is automatically saved when leaving a page.

There is no page cache, lazy page construction aside from the selected closure, or per-route focus restoration. `migrate_state` can move transient widget state by id, but page data migration is the caller's job.

## The focus and screen readers

The router emits one `Role::Group` node labelled with the current page id; page contents emit their own nodes. Back/forward are `Role::Button` with labels `back`/`forward` and values `enabled`/`disabled`.

The router does not move focus on navigation, announce the new page, or expose a landmark/live region. A platform screen reader is not connected; these are metadata nodes only.

## Transitions

`RouterState` itself has no transition state. `transition(c, &mut state, target)` exposes `TransitionFrame { rect, progress }` for a caller to animate a layout rectangle; it uses the frame clock and honors reduce-motion.

Page cross-fades, enter/exit animations, shared-element transitions and history-driven transition directions are not implemented. The caller must build both states and animate them explicitly.
