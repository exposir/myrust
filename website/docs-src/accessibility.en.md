MyRust emits `AccessNode` metadata every frame. It is a flat vector of `role`, `label`, `value`, `disabled`, `focused` and `bounds`; it is not connected to AT-SPI, UIA or NSAccessibility.

```rust
let frame = ui.render(Size::new(640.0, 360.0))?;
for node in &frame.access_nodes {
    println!("{:?} {} {:?}", node.role, node.label, node.value);
}
```

## Names

Standard controls set `label` from the visible text or a fixed English string. `text` uses its content as the label; buttons use their text; `text_field` uses `"text field"`; `checkbox` uses `"checkbox"`; groups use `"column"`, `"row"`, `"container"` or `"grid"`.

There is no public authoring API for an accessible name, label association, description or name-from-author. Custom drawing cannot add or change an `AccessNode` without crate changes.

## Roles

`Role` values currently emitted are `Button`, `CheckBox`, `Slider`, `TextField`, `Group`, `Label`, `ScrollView`, `Switch`, `RadioGroup`, `Radio`, `ComboBox`, `Option`, `TabList`, `Tab`, `ProgressBar`, `Spinner`, `Badge`, `Disclosure` and `Popup`.

There are no `List`, `Table`, `Grid`, `Dialog`, `Alert`, `Landmark` or `Link` roles. Lists expose `Option` rows, tabs expose `TabList`/`Tab`, and overlays expose `Popup`; other semantics are approximated by `Group` and `Label`.

## States

State is a string in `value`: checkbox `"checked"`/`"unchecked"`, tab `"selected"`/`"unselected"`, switch `"on"`/`"off"`, option `"selected"`/`"unselected"`, back/forward `"enabled"`/`"disabled"`, and disclosure open/closed where emitted. `focused` is a boolean.

`AccessNode::disabled` exists but the producer currently hardcodes `false`; standard controls have no public disabled API. Expanded/collapsed, busy, invalid, required, readonly and checked tri-state are not represented.

## Suggestions

There is no suggestion list, active descendant, autocomplete or combobox expansion state in `AccessNode`. `ComboBox` and `Option` nodes can be emitted for the popup, but assistive technology cannot see the relationship or the highlighted option.

## Descriptions and errors

There are no description, error, validation or help fields. Form labels/hints are visual only; `form_field` groups and labels do not emit these relationships.

## Orientation

No orientation field is emitted for sliders, scroll views, split views or tab lists. The visible geometry may imply horizontal, but AT receives only bounds.

## Keyboard shortcuts

No shortcut metadata, access key or accelerator is emitted. Buttons and fields react to keys, but the key bindings are not exposed to assistive technology.

## Formats of text

All accessible text is a plain `String`. There is no markup, language tag, pronunciation, selection range or rich-text attribute in `AccessNode`.

## Lists and tables

Lists emit `Role::Option` rows but no list container, row count, selection mode or multiselect state. Tables emit button nodes for sortable headers; headers and cells are not a table/row/column model.

## Place in a set

There is no position-in-set or set-size metadata (`posinset`/`setsize`). A list's `Option` rows and a grid view's `Button` cells carry only their own label/value/bounds.

## Selected, without the look

Selected state is expressed in visual color and, for some controls, a string `value` of `"selected"`/`"unselected"`. It is not a dedicated selected boolean on every node, and custom selected visuals have no semantic counterpart.

## Grids

There is no `Grid` role, row/column index or cell coordinate metadata. `grid` and `grid_view` output groups/buttons with ordinary bounds; spatial position is implicit in geometry only.

## Unavailable, still reachable

Disabling is visual in some controls (muted painting) but `AccessNode::disabled` is always false and there is no disabled input API. A disabled-looking control may still receive pointer input unless the caller skips constructing it.

There is no “disabled but focusable” or “read-only” state distinction.

## Out of the pointer's way

There is no hidden/occluded flag. Bounds may be offscreen, clipped by `scroll_view` or covered by an overlay, but `AccessNode` still includes them; hit-testing uses separate interaction regions and overlay layers.

## Out of assistive technology's way

There is no `aria-hidden`, accessibility-hidden or excluded subtree API. Not constructing a widget removes its node; constructing it always emits metadata.

## Announcements

No live region, alert, status or announcement API exists. Toasts and dialogs have visual roles but no announcement event.

## Landmarks, live regions and rules

No landmark roles, live-region flags, atomic/relevant properties, error announcements or accessibility rules are implemented. `AccessNode` is a per-frame flat list, not a semantic tree with relationships.

A platform AT bridge would need role/action mapping, focus/selection events, window handles and tree diffing; see the `Inspector` for debugging snapshots, not for AT.
