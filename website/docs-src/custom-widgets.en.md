Custom widgets are composed from the public builders: containers, rows/columns, `.draw(...)` commands, standard controls and focusable regions. There is no `Widget` trait, base class, lifecycle hook or retained component protocol.

```rust
use myrust_core::ui::{container, DrawCommand, Radius, Color};

container(c)
    .size(120.0, 44.0)
    .focusable()
    .draw([
        DrawCommand::RoundedRect {
            x: 0.0, y: 0.0, width: 120.0, height: 44.0,
            radius: Radius::all(8.0), color: Color::rgb(17, 24, 39),
        },
        DrawCommand::Text {
            x: 12.0, y: 12.0, text: "Custom".to_string(),
            font_size: 14.0, weight: 500,
            color: Color::rgb(226, 232, 240),
            max_width: Some(96.0), alignment: myrust_core::ui::TextAlignment::Start,
        },
    ]);
```

## Widgets of your own

Builders create nodes; `.draw` attaches raw commands to a container, `.focusable()` registers the container as one focus stop with a default id. `Context::hover(id)` can query pointer hover, but there is no public “was this custom node clicked” query.

For click/keyboard behavior, compose a standard `button`/`checkbox`/control inside the custom drawing or handle raw `UiEvent`s outside. There is no disabled flag, builder-state struct, measurement protocol or custom layout engine.

### Stopping what a widget started

There is no cancellation handle or teardown hook. A custom widget stops by not being constructed on the next frame; its interaction region disappears after the previous frame. Any animation/state it owns must be dropped or ignored by the caller.

A standard control's transient state is keyed by string id in `Context`; `migrate_state` can move it, but there is no public API to cancel a started drag/gesture except releasing the pointer.

Custom containers emit `Role::Group` with label `"container"`; raw draw commands emit no AccessNode. A custom accessible role/name is not supported; place a standard control or text node inside if semantics are needed.
