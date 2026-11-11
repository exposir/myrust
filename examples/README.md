# Examples

Small runnable apps, one idea each. They follow the
[mygo examples](https://github.com/egoist/mygo/tree/main/examples) where the
current MyRust API can express them.

Run any of them from the workspace root:

```bash
cargo run -p example-hello
```

## WebView windows

| Example | Crate | Shows |
|---|---|---|
| Hello | `example-hello` | A page calling one typed Rust method |
| Todo | `example-todo` | Typed IPC in both directions, JSON persistence under the data directory |
| Drag and drop | `example-drag-drop` | Native file-drop events forwarded into page JavaScript |

## Native UI windows

MyRust draws these itself with the CPU presenter; there is no web engine in the
process.

| Example | Crate | Shows |
|---|---|---|
| Counter | `example-counter-native` | A view rebuilt every frame from Rust state, with buttons |
| Clipboard | `example-clipboard` | Text fields plus system clipboard read/write |
| Text input | `example-text-input` | Text field, checkbox, slider, and button state |
| Loading | `example-loading` | A frame-driven animation without a web page |
| Streaming chart | `example-chart-streaming` | `DisplayList` lines redrawn from animated state |
| Gallery | `example-gallery` | Built-in widgets and raw drawing commands: button, checkbox, switch, radio group, select, tabs, accordion, list, table, tree, outline, grid, grid view, split view, sidebar, breadcrumbs, stepper, rating, range slider, number input, decimal input, search field, combobox, autocomplete, token field, editable text, pages/router, history buttons, find bar, markdown, rich text, meter, avatar, bar/line charts, image, icon, transition, inspector, frame stats, transformed hit, RTL row, locale formatting, control-id migration, link, toggle, segmented control, toolbar, calendar, date input, time input, color picker, code editor, diff viewer, drag/drop, reorder list, text-input primitives, slider, progress, spinner, badge, collapsible, menu button, context menu, tooltip, popover, toast, dialog, alert dialog, form field, text field |

## Coverage against mygo

Nine of mygo's 38 examples have a direct equivalent. The rest are not stubs:
they need capabilities the current API does not provide.

| Not ported | Reason |
|---|---|
| `blackhole`, `wind`, `chat-glow`, `liquid-glass`, `effort-slider` | Shader and effect rendering; the native presenter draws geometry and text only. |
| `farm`, `mayoi` | Game loops, sprites, and collision input beyond the current widget set. |
| `piano`, `video`, `terminal` | No audio, video, or PTY backends. |
| `notes`, `documents`, `code-editor`, `text-buffer`, `markdown-chat` | No rich-text document model; the native text field is single-line. |
| `launcher`, `frameless`, `flyout`, `menubar-flyout`, `menubar-native`, `permissionflow` | No frameless drag regions, separate popover/panel windows, menu-bar-only mode, or permission APIs. In-window menus and popovers exist; tray icons and global shortcuts exist, but the window behaviors around them do not. |
| `embed-webview` | A window hosts either a WebView or a native UI, not both. |
| `native-module` | No dynamic native-module loading. |
| `rust-native` | The host language is already Rust; every example calls into the core crate directly. |
| `vibrancy`, `sf-symbols` | macOS-only appearance APIs that are not exposed. |
| `confetti`, `timeline` | Animation is immediate-mode and frame-driven; there are no pose or timeline composition primitives. |
| `native` | Menus, tray, dialogs, notifications, and shortcuts exist individually, but there is no combined desktop-integration tour yet. |

## In-window overlays

The Gallery covers the Batch 2a surface controls. `MenuButton` opens a menu
anchored to its trigger; `ContextMenu` opens at a secondary-button press inside
a caller-provided area; `Tooltip` appears after a hover or focus delay; and
`Popover` is the overlay primitive used for focus-trapped anchored panels.
Escape and a primary-button release outside the previous surface bounds dismiss
menus and popovers.

Right-click depends on the window adapter forwarding
`UiEvent::PointerPressed { button: PointerButton::Right, .. }` (or another
non-primary button). The tao path already maps `MouseButton::Right`, but
Ctrl+click and trackpad context-menu gestures are not synthesized yet. Tooltips
are painted on their own layer and never capture pointer input or focus.

## Feedback, dialogs, and forms

Batch 2b covers the remaining in-window surfaces:

- `Toast` queues stack from the top-right, expire on the frame clock, and can
  carry one action button. Toasts are non-modal: they neither steal focus nor
  block the controls underneath, so a click can reach both layers.
- `Dialog` centers a caller-built panel and reuses the overlay focus trap,
  lower-layer pointer blocking, and focus restoration. Escape closes it;
  outside clicks do not.
- `AlertDialog` is the title/body/confirm/cancel layout on top of `Dialog`.
- `FormField` groups a label, description, input, and error message. The group
  carries the visible label as its accessibility label, and error text uses
  `Theme::warn()`.

## Layout and navigation

Batch 3a exposes the grid solver and adds three navigation/layout controls:

- `Grid` is a `grid(c)` builder with `.columns(n)` and the same `.gap`,
  `.padding`, `.size`, and `.children` methods as `column`/`row`. Columns are
  equal width and rows use one uniform height (the tallest cell in the grid).
- `SplitView` holds a caller-owned `SplitViewState { ratio }`, clamps the ratio
  to 15%..85%, and supports pointer drag plus ArrowLeft/ArrowRight
  keyboard adjustment on the focused divider.
- `Sidebar` renders titled sections of selectable items, keeps one item
  selected, supports Arrow/Home/End navigation while focused, and returns the
  selected id.
- `Breadcrumbs` returns the id of a clicked non-final path item. The final
  item is rendered as an inert label for the current page.

## Accordion, list, table, and tree

Batch 3b adds data-shaped controls:

- `Accordion` owns a list of open section ids. `AccordionMode::Single` closes
  other sections when one opens; `Multiple` allows several. Headers toggle by
  click or focused Enter/Space.
- `List` virtualizes rows: it constructs only rows intersecting the viewport,
  uses the same scroll-view state under the list id, and preserves total height
  with spacer nodes. Row heights may be explicit per row or use a default.
- `Table` exposes fixed-width columns, sort requests from header clicks, and
  row selection. The caller sorts its own row data from the returned
  `TableSort` before the next frame.
- `Tree` renders flat nodes with `depth` indentation and a caller-owned
  `expanded` id list. The disclosure arrow toggles expansion; clicking the row
  selects it.
- `Outline` is the panel-style presentation over the same tree interaction,
  state, and response semantics.

## Numeric inputs

Batch 4a adds five controls built on the existing text-field and slider
infrastructure:

- `Stepper` steps an integer with minus/plus buttons or ArrowUp/ArrowDown,
  clamped to its min/max.
- `Rating` sets a 0..max value by clicking a star, previews on hover, and
  adjusts with Arrow keys while focused.
- `RangeSlider` holds low/high bounds, picks the nearest knob on press, clamps
  low <= high, and adjusts the active knob with Arrow keys.
- `NumberInput` is a filtered text field that drops non-numeric characters,
  limits fraction digits, and steps with ArrowUp/ArrowDown.
- `DecimalInput` keeps an exact scaled integer for money-style values,
  truncating extra fraction digits without rounding and rejecting illegal
  characters. `format_decimal_units` renders the canonical fixed-decimal text.

## Search and inline editing

Batch 4b adds five text-centric controls:

- `SearchField` wraps the single-line editor with a clear button. Escape
  clears the text and Enter reports `submitted`.
- `Combobox` filters options as the user types, moves the highlight with
  ArrowUp/ArrowDown, selects with Enter, and closes with Escape. Its overlay
  list keeps focus in the text input.
- `Autocomplete` shows caller-provided prefix matches below the input. Enter or
  clicking a suggestion commits it. Dismissing the overlay hides it until the
  query changes.
- `TokenField` renders tokens as chips. Enter and comma add tokens, Backspace
  on an empty buffer removes the last token, and clicking a chip removes it.
- `EditableText` switches a label into an in-place text field on click.
  Enter commits, Escape cancels, and losing focus commits.

## Routing and formatted text

Batch 5 adds the UI-level routing and text presentation layer:

- `RouterState` keeps a page history with push/pop/back/forward. `router`
  renders the current page closure; unknown page ids render a fallback label.
- `BackForwardButtons` enables back/forward from history and mutates the
  router when clicked.
- `FindBar` combines a text field, caller-supplied match count, and wrapped
  previous/next navigation. `count_matches` is available for simple
  case-insensitive counting.
- `Markdown` supports headings 1..=6, paragraphs, unordered/ordered lists, and
  inline bold/italic/code/link spans. Rendering is built from ordinary text and
  layout nodes, not a web view.
- `RichText` renders a read-only formatted paragraph from the same inline
  parser. It does not edit text.

## Grids, meters, avatars, charts, and images

Batch 6a adds data-display controls:

- `GridView` is a scrollable virtualized grid of fixed-size items. Only rows
  intersecting the viewport are constructed; the caller gets an item render
  closure and a selected index.
- `Meter` is a separate quantity bar with low/medium/high bands from two
  thresholds, using Theme muted/accent/warn colors.
- `Avatar` draws initials or decoded RGBA pixels in a circle or rounded square,
  with a caller-supplied accessible label.
- `BarChart` / `LineChart` map finite values to a local plot rectangle, auto or
  caller-provided min/max, and emit ordinary rect/line/circle commands.
- `Image` accepts decoded RGBA8 pixels and scales them with nearest-neighbor
  sampling into ordinary filled rects. It does not decode PNG/JPEG and does
  not add a dependency; decoding belongs to the caller.

## Icons and layout transitions

Batch 6b extends the rendering path and adds a layout interpolation helper:

- `parse_svg_path` handles M/m, L/l, H/h, V/v, Q/q, C/c, A/a, and Z/z,
  flattening curves and elliptical arcs into contours.
- `CpuRasterizer` now fills SVG contours with even-odd scan conversion and
  strokes them with scaled line segments. Built-in `Icon` widgets are visible
  through the CPU presenter; images remain the only unsupported display
  command.
- `icon(c, id, name, size, color)` places a built-in icon as a fixed-size
  immediate-mode widget.
- `TransitionState` interpolates previous/current `Rect` bounds using the
  existing `Animation` duration and easing. A changed target retargets from the
  current interpolated bounds and requests animation frames until done.

## Transforms, inspector, and frame stats

Batch 7a adds debug and hit-testing infrastructure:

- `transform(c, transform, content)` applies a translation/scale/rotation scope
  to leaf controls and records the affine transform on their interaction
  regions. Pointer hit-testing inverse-transforms the pointer back into local
  bounds before testing, so transformed buttons and sliders remain clickable.
- `inspect_frame` builds a containment tree from a `ViewFrame` with
  id/role/bounds and per-element paint summaries. `inspector` renders the tree
  as an indented text list; `inspect_display` summarizes a bare display list.
- `FrameStats` records the previous frame's layout, paint, and total
  milliseconds. `Context::frame_stats()` returns the last sample,
  `accumulate` sums samples, and `frame_stats_panel` renders the optional
  compact display.

## Direction, formatting, and migration

Batch 7b closes the UI layer with three small helpers:

- `Direction` (`Ltr`/`Rtl`) is set on `Context`. RTL reverses relative row
  children, flips default text alignment to end, and mirrors icons. The default
  stays LTR.
- `format_number` and `format_date` cover an explicit en-US/zh-CN subset:
  comma-grouped numbers with fixed decimals, ISO dates, en-US short dates, and
  zh-CN short dates. They return `FormatError` for invalid input.
- `migrate_value` moves a map entry from an old id to a free new id.
  `Context::migrate_state` does the same for text-field/select/scroll/slider
  state, id-prefixed animations, overlay runtimes, and focus.

## Links, toggles, segments, and toolbars

Batch 8a adds four navigation/action controls:

- `Link` is an accent-colored text button with hover/focus underline. Click or
  Enter returns `clicked`; URL opening is caller-owned.
- `Toggle` is a pressed-state button. Without a group it owns its `bool`; with
  a shared `ToggleGroup` the group holds one active id and activating one toggle
  clears the others.
- `Segmented` is an equal-width selector. Click or Arrow/Home/End changes the
  caller-owned selected index.
- `Toolbar` lays out leading items by caller-declared widths and moves the tail
  into a MenuButton overflow menu. `overflow_split` exposes the same fit
  decision.

## Calendar, date, time, and color

Batch 8b adds the date/time/color input family:

- `Calendar` renders a 6x7 month grid with caller-owned year/month/selected
  day. Click and arrow keys select, Home/End jump within a month, and
  PageUp/PageDown change month.
- `DateInput` combines an ISO text field with a calendar overlay. Valid text
  syncs the calendar; a calendar day updates the text and closes the popup;
  Escape closes through the overlay primitive.
- `TimeInput` is a `HH:MM` text field with four hour/minute step buttons.
  Values are range-checked and clamp at `00:00..=23:59`; Enter normalizes.
- `ColorPicker` is a swatch that opens a preset palette and hex field.
  Preset clicks and valid hex return a `Color`; invalid hex falls back to the
  current color.

## Code, diff, drag, and text primitives

Batch 9 closes the widget surface:

- `CodeEditor` is a read-only line-numbered code view with lightweight Rust
  keyword/string/comment/number spans. Clicking a line selects it.
- `DiffViewer` computes a line LCS diff and renders unified or paired
  side-by-side rows with added/removed backgrounds.
- `DragSource`/`DropTarget` use a caller-owned `DragState` for payload, origin,
  and pointer position; the source paints a ghost and the target reports a
  completed `DragDrop`.
- `ReorderList` reports source/target row indices and `reorder` applies the
  move to caller data.
- `TextInputState` exposes cursor byte offset, normalized selection, and
  field/cursor geometry for custom controls built on the existing text field.
