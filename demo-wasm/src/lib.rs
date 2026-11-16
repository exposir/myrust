//! Raw C-ABI wasm entry points for `/docs-demo`.
//!
//! The JavaScript host supplies `env.myrust_now_ms` and reads the RGBA buffer
//! through `demo_command_ptr`/`demo_command_len` and `demo_render`. No wasm-bindgen or JS glue is used.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, OnceLock, atomic::{AtomicBool, Ordering}},
};

use myrust_ui::{
    accordion, alert_dialog, autocomplete, back_forward_buttons, breadcrumbs, button, icon_button, button_primary_with_icon, button_with_icon, calendar,
    checkbox, checkbox_labeled, checkbox_indeterminate, code_editor, collapsible, collapsible_custom, color_panel, color_picker, combobox, column, container, date_input, decimal_input, dialog, diff_viewer, editable_text,
    find_bar, form_field, form_group, form_row, format_date, format_number, frame_stats_panel, grid, grid_view, inspector,
    button_disabled, button_padded, button_padded_disabled, button_primary, link, menu_button, menu_icon_button, number_input, outline, overlay,
    popover, radio, range_slider,
    rating, rich_text_spans, row, router, scroll_view, scroll_view_styled, ScrollStyle, search_field, segmented, select,
    select_sized, sidebar, slider, spacer,
    slider_stepped,
    spinner, split_sized, split_view, stepper, switch, tabs, table, text, text_area_mono, text_area_placeholder, text_area_sized,
    area_path, extent, filled_path, grid_line, line_path, path_mark, pie, plot, plot_sized, point,
    rule_x, measured_text_height,
    sector_path, stack as chart_stack, stroked_path, AxisStyle, Band, Curve, Linear, Tick, TimeScale,
    code_editor_with_options, text_area_sized_placeholder, text_field, text_field_invalid, text_field_password_placeholder, text_field_placeholder, time_input, toasts,
    token_field, segment, tooltip_for, tree, avatar, badge, bar_chart, icon,
    image, image_styled, ImageFit, ImageStyle, line_chart, meter, meter_levels, progress, progress_indeterminate, AccordionMode, AccordionSection,
    AccordionState, AutocompleteState, AvatarShape, BreadcrumbItem, CalendarState,
    CodeDiagnostic, CodeEditorOptions, CodeEditorState, CodeSeverity, ColorPanelState, ColorPickerState, ComboboxState, ContextMenuState, DateInputState, DateStyle,
    DecimalInputOptions, DialogAction, DiffViewerState, Direction, Color, Context,
    DrawCommand, EditableTextState, FontFace,
    GridViewState, ImageData, InspectorItem, InspectorTree, ListState, Locale, MenuState,
    NativeUi,
    NumberInputOptions, OverlayState, Page, PaintSummary, RangeSliderState, Role,
    RouterState, SidebarItem, SidebarSection, Size, SplitOrientation, SplitViewState, TableColumn,
    TableRow, TableState, Theme, ToastQueue,
    Transform, TransitionState, TreeState, TreeNode, AlignItems, Edges, JustifyContent,
    RichTextSpan,
    GradientStop, Point, SegmentFace, SortDirection,
};

thread_local! {
    static APP: RefCell<Option<DemoApp>> = const { RefCell::new(None) };
    static COMMAND_BUFFER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// Bytes the page writes before calling an export that takes a string
    /// (a dropped file's name, size and first line).
    static HOST_BUFFER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Capacity of the buffer `demo_host_buffer` hands to the page.
const HOST_BUFFER_BYTES: usize = 1 << 16;

/// Returns a writable buffer the page fills before `demo_drop_files`.
#[no_mangle]
pub extern "C" fn demo_host_buffer() -> *mut u8 {
    HOST_BUFFER.with(|buffer| {
        let mut bytes = buffer.borrow_mut();
        if bytes.capacity() < HOST_BUFFER_BYTES {
            let missing = HOST_BUFFER_BYTES.saturating_sub(bytes.len());
            bytes.reserve(missing);
        }
        bytes.as_mut_ptr()
    })
}

/// Records the files of a drop. Each line is `name<TAB>size<TAB>first line`
/// — the path a desktop build reads is not exposed to a page, so the demo
/// shows the same facts a browser can give.
#[no_mangle]
pub extern "C" fn demo_drop_files(len: u32) {
    let len = (len as usize).min(HOST_BUFFER_BYTES);
    let mut payload = String::new();
    HOST_BUFFER.with(|buffer| {
        let bytes = buffer.borrow();
        if len <= bytes.capacity() {
            // SAFETY: the page wrote `len` bytes at this buffer's pointer
            // through `demo_host_buffer`, the only writer of `HOST_BUFFER`.
            let slice = unsafe { std::slice::from_raw_parts(bytes.as_ptr(), len) };
            if let Ok(text) = std::str::from_utf8(slice) {
                payload.push_str(text);
            }
        }
    });
    if payload.is_empty() {
        return;
    }
    APP.with(|cell| {
        let mut app = cell.borrow_mut();
        let Some(app) = app.as_mut() else { return };
        let Some(dropped) = app.dropped.as_ref() else {
            return;
        };
        let mut files = dropped.borrow_mut();
        files.clear();
        for line in payload.lines() {
            let mut parts = line.split('\t');
            let name = parts.next().unwrap_or("").to_string();
            let size = parts
                .next()
                .and_then(|size| size.parse::<usize>().ok())
                .unwrap_or(0);
            let first = parts.next().unwrap_or("").to_string();
            if !name.is_empty() {
                files.push(DroppedFile {
                    name,
                    size,
                    line: first,
                });
            }
        }
        app.probe.set(files.len() as i32);
    });
}

/// Reports whether a drag hovers the drop zone.
#[no_mangle]
pub extern "C" fn demo_drag_hover(hovering: u32) {
    APP.with(|cell| {
        let app = cell.borrow();
        if let Some(hover) = app.as_ref().and_then(|app| app.drag_hover.as_ref()) {
            hover.set(hovering != 0);
        }
    });
}

static FONT: OnceLock<Option<Arc<FontFace>>> = OnceLock::new();

/// The docs preview: MyGo wraps every demo in
/// `Column(c).Fill().Center().Gap(12).Padding(20)` (`docs-demo/main.go`
/// preview), so the same frame is used here. `width` limits the column of
/// the demo's own items where a demo sets its controls' widths.
fn stack<F: FnOnce(&mut Context)>(context: &mut Context, width: f32, _gap: f32, children: F) {
    // MyGo's preview — `Column(c).Fill().Center().Gap(12).Padding(20)` — is
    // drawn by `NativeUi::with_preview`; a demo's items go straight into
    // it, a column as wide as the demo's own controls. Its children center
    // across, as they do in MyGo's preview column, which leaves a narrower
    // status line centered under the controls rather than at their edge.
    column(context)
        .width(width)
        .gap(12.0)
        .align(AlignItems::Center)
        .children(children);
}

struct DemoApp {
    slug: u32,
    ui: NativeUi,
    frame: Rc<Cell<u64>>,
    selected: Option<Rc<RefCell<usize>>>,
    probe: Rc<Cell<i32>>,
    /// The files a browser drop delivered to the drag-and-drop demo, and
    /// whether a drag currently hovers its zone.
    dropped: Option<Rc<RefCell<Vec<DroppedFile>>>>,
    drag_hover: Option<Rc<Cell<bool>>>,
    /// Toasts the showcase landing page raises from its call-to-action
    /// buttons.
    toasts: Option<Rc<RefCell<ToastQueue>>>,
}

/// A file dropped on the drag-and-drop demo. A browser exposes its name,
/// size and bytes, not the path a desktop build reads with `os.ReadFile`.
struct DroppedFile {
    name: String,
    size: usize,
    line: String,
}

impl DemoApp {
    fn new(slug: u32) -> Self {
        let frame = Rc::new(Cell::new(0_u64));
        let selected = (slug == 0).then(|| Rc::new(RefCell::new(0_usize)));
        let probe = Rc::new(Cell::new(0_i32));
        let dropped = (slug == 7).then(|| Rc::new(RefCell::new(Vec::new())));
        let drag_hover = (slug == 7).then(|| Rc::new(Cell::new(false)));
        let toasts = (slug == 83).then(|| Rc::new(RefCell::new(ToastQueue::new())));
        let ui = match slug {
            0 => {
                let view_selected = Rc::clone(selected.as_ref().expect("tabs selection"));
                let view_frame = Rc::clone(&frame);
                let view_probe = Rc::clone(&probe);
                let notify = Rc::new(Cell::new(true));
                let sounds = Rc::new(Cell::new(false));
                let dark = Rc::new(Cell::new(false));
                let compact = Rc::new(Cell::new(true));
                let logging = Rc::new(Cell::new(false));
                let view_notify = Rc::clone(&notify);
                let view_sounds = Rc::clone(&sounds);
                let view_dark = Rc::clone(&dark);
                let view_compact = Rc::clone(&compact);
                let view_logging = Rc::clone(&logging);
                NativeUi::new(move |context| {
                    tabs_demo(
                        context,
                        &view_selected,
                        &view_notify,
                        &view_sounds,
                        &view_dark,
                        &view_compact,
                        &view_logging,
                        &view_frame,
                        &view_probe,
                    );
                })
            }
            1 => {
                let copied = Rc::new(Cell::new(false));
                let mode = Rc::new(Cell::new(0_usize));
                let view_frame = Rc::clone(&frame);
                let view_copied = Rc::clone(&copied);
                let view_mode = Rc::clone(&mode);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    markdown_demo(context, &view_frame, &view_copied, &view_mode, &view_probe);
                })
            }
            2 => {
                let counter = Rc::new(Cell::new(0_i32));
                let view_counter = Rc::clone(&counter);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    views_demo(context, &view_counter, &view_probe);
                })
            }
            3 => {
                let wide = Rc::new(Cell::new(false));
                let view_wide = Rc::clone(&wide);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    layout_demo(context, &view_wide, &view_probe);
                })
            }
            4 => {
                let large = Rc::new(Cell::new(false));
                let view_large = Rc::clone(&large);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    text_demo(context, &view_large, &view_probe);
                })
            }
            5 => {
                let light = Rc::new(Cell::new(false));
                let view_light = Rc::clone(&light);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    styling_demo(context, &view_light, &view_probe);
                })
            }
            6 => {
                let value = Rc::new(RefCell::new(String::new()));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    input_demo(context, &view_value, &view_probe);
                })
            }
            7 => {
                let view_dropped = Rc::clone(dropped.as_ref().expect("drag files"));
                let view_hover = Rc::clone(drag_hover.as_ref().expect("drag hover"));
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    drag_demo(context, &view_dropped, &view_hover, &view_probe);
                })
            }
            8 => {
                let route = Rc::new(RefCell::new(RouterState::new("home")));
                let view_route = Rc::clone(&route);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    navigation_demo(context, &view_route, &view_probe);
                })
            }
            9 => {
                let open = Rc::new(RefCell::new(OverlayState::new(false)));
                let view_open = Rc::clone(&open);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    overlays_demo(context, &view_open, &view_probe);
                })
            }
            10 => {
                let checked = Rc::new(Cell::new(false));
                let view_checked = Rc::clone(&checked);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    accessibility_demo(context, &view_checked, &view_probe);
                })
            }
            11 => {
                let rtl = Rc::new(Cell::new(false));
                let view_rtl = Rc::clone(&rtl);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    rtl_demo(context, &view_rtl, &view_probe);
                })
            }
            12 => {
                let zh = Rc::new(Cell::new(false));
                let view_zh = Rc::clone(&zh);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    languages_demo(context, &view_zh, &view_probe);
                })
            }
            13 => {
                let light = Rc::new(Cell::new(false));
                let view_light = Rc::clone(&light);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    drawing_demo(context, &view_light, &view_probe);
                })
            }
            14 => {
                let moved = Rc::new(Cell::new(false));
                let transition_state = Rc::new(RefCell::new(TransitionState::default()));
                let view_moved = Rc::clone(&moved);
                let view_transition = Rc::clone(&transition_state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    transitions_demo(context, &view_moved, &view_transition, &view_probe);
                })
            }
            15 => {
                let target = Rc::new(Cell::new(0.0_f32));
                // MyGo's demo starts on "Snappy", the middle of its three
                // animation styles (`website/docs-demo/motion.go`: `style := 1`).
                let style = Rc::new(Cell::new(1_usize));
                let view_target = Rc::clone(&target);
                let view_style = Rc::clone(&style);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    motion_demo(context, &view_target, &view_style, &view_probe);
                })
            }
            16 => {
                let rotated = Rc::new(Cell::new(false));
                let view_rotated = Rc::clone(&rotated);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    transforms_demo(context, &view_rotated, &view_probe);
                })
            }
            17 => {
                let accent = Rc::new(Cell::new(false));
                let view_accent = Rc::clone(&accent);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    custom_widget_demo(context, &view_accent, &view_probe);
                })
            }
            18 => {
                let expanded = Rc::new(Cell::new(false));
                let view_expanded = Rc::clone(&expanded);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    windows_demo(context, &view_expanded, &view_probe);
                })
            }
            19 => {
                let passed = Rc::new(Cell::new(false));
                let view_passed = Rc::clone(&passed);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    testing_demo(context, &view_passed, &view_probe);
                })
            }
            20 => {
                let extra = Rc::new(Cell::new(false));
                let view_extra = Rc::clone(&extra);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    inspector_demo(context, &view_extra, &view_probe);
                })
            }
            21 => {
                let filled = Rc::new(Cell::new(false));
                let view_filled = Rc::clone(&filled);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    rendering_demo(context, &view_filled, &view_probe);
                })
            }
            22 => {
                let renamed = Rc::new(Cell::new(false));
                let value = Rc::new(RefCell::new("keep me".to_string()));
                let view_renamed = Rc::clone(&renamed);
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    migration_demo(context, &view_renamed, &view_value, &view_probe);
                })
            }
            23 => {
                let stress = Rc::new(Cell::new(false));
                let view_stress = Rc::clone(&stress);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    performance_demo(context, &view_stress, &view_probe);
                })
            }
            24 => {
                let count = Rc::new(Cell::new(0_i32));
                let editing = Rc::new(Cell::new(true));
                let view_count = Rc::clone(&count);
                let view_editing = Rc::clone(&editing);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    button_demo(context, &view_count, &view_editing, &view_probe);
                })
            }
            25 => {
                let details = Rc::new(Cell::new(false));
                let view_details = Rc::clone(&details);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    link_demo(context, &view_details, &view_probe);
                })
            }
            26 => {
                let state = Rc::new(RefCell::new(MenuState::default()));
                let more = Rc::new(RefCell::new(MenuState::default()));
                let selected = Rc::new(Cell::new(-1_i32));
                let view_state = Rc::clone(&state);
                let view_more = Rc::clone(&more);
                let view_selected = Rc::clone(&selected);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    menu_demo(context, &view_state, &view_more, &view_selected, &view_probe);
                })
            }
            27 => {
                let state = Rc::new(RefCell::new(NotesDemo::new()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    context_demo(context, &view_state, &view_probe);
                })
            }
            28 => {
                let state = Rc::new(RefCell::new([false, true, false, false]));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    toggle_demo(context, &view_state, &view_probe);
                })
            }
            29 => {
                let selected = Rc::new(Cell::new(0_i32));
                let view_selected = Rc::clone(&selected);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    segmented_demo(context, &view_selected, &view_probe);
                })
            }
            30 => {
                let style = Rc::new(RefCell::new([false, false]));
                let shared = Rc::new(Cell::new(0_i32));
                let view_style = Rc::clone(&style);
                let view_shared = Rc::clone(&shared);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    toolbar_demo(context, &view_style, &view_shared, &view_probe);
                })
            }
            31 => {
                let values = Rc::new(RefCell::new([true, false, true, false, true]));
                let reloads = Rc::new(Cell::new(0_i32));
                let view_values = Rc::clone(&values);
                let view_reloads = Rc::clone(&reloads);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    checkbox_demo(context, &view_values, &view_reloads, &view_probe);
                })
            }
            32 => {
                let wifi = Rc::new(Cell::new(true));
                let bluetooth = Rc::new(Cell::new(false));
                let view_wifi = Rc::clone(&wifi);
                let view_bluetooth = Rc::clone(&bluetooth);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    switch_demo(context, &view_wifi, &view_bluetooth, &view_probe);
                })
            }
            33 => {
                let selected = Rc::new(Cell::new(1_i32));
                let align = Rc::new(Cell::new(1_i32));
                let view_selected = Rc::clone(&selected);
                let view_align = Rc::clone(&align);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    radio_demo(context, &view_selected, &view_align, &view_probe);
                })
            }
            34 => {
                let selected = Rc::new(Cell::new(1_i32));
                let view_selected = Rc::clone(&selected);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    select_demo(context, &view_selected, &view_probe);
                })
            }
            35 => {
                let volume = Rc::new(Cell::new(35.0_f32));
                let quality = Rc::new(Cell::new(75.0_f32));
                let view_volume = Rc::clone(&volume);
                let view_quality = Rc::clone(&quality);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    slider_demo(context, &view_volume, &view_quality, &view_probe);
                })
            }
            36 => {
                let state = Rc::new(RefCell::new(RangeSliderState::new(200.0, 800.0)));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    range_demo(context, &view_state, &view_probe);
                })
            }
            37 => {
                let value = Rc::new(Cell::new(2_i64));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    stepper_demo(context, &view_value, &view_probe);
                })
            }
            38 => {
                let value = Rc::new(Cell::new(3_u32));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    rating_demo(context, &view_value, &view_probe);
                })
            }
            39 => {
                let value = Rc::new(RefCell::new(String::new()));
                let password = Rc::new(RefCell::new(String::new()));
                let notes = Rc::new(RefCell::new(String::new()));
                let view_value = Rc::clone(&value);
                let view_password = Rc::clone(&password);
                let view_notes = Rc::clone(&notes);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    text_input_demo(context, &view_value, &view_password, &view_notes, &view_probe)
                })
            }
            40 => {
                let value = Rc::new(RefCell::new(
                    "func main() {\n\tfmt.Println(\"Hello, world\")\n}\n".to_string(),
                ));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| primitives_demo(context, &view_value, &view_probe))
            }
            41 => {
                let copies = Rc::new(RefCell::new("2".to_string()));
                let opacity = Rc::new(RefCell::new("0.50".to_string()));
                let view_copies = Rc::clone(&copies);
                let view_opacity = Rc::clone(&opacity);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    number_demo(context, &view_copies, &view_opacity, &view_probe)
                })
            }
            42 => {
                let price = Rc::new(RefCell::new("1234.50".to_string()));
                let discount = Rc::new(RefCell::new("0.00".to_string()));
                let view_price = Rc::clone(&price);
                let view_discount = Rc::clone(&discount);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    decimal_demo(context, &view_price, &view_discount, &view_probe)
                })
            }
            43 => {
                let value = Rc::new(RefCell::new(String::new()));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| search_demo(context, &view_value, &view_probe))
            }
            44 => {
                let query = Rc::new(RefCell::new("Georgia".to_string()));
                let state = Rc::new(RefCell::new(ComboboxState {
                    open: false,
                    ..ComboboxState::default()
                }));
                let view_query = Rc::clone(&query);
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| combobox_demo(context, &view_query, &view_state, &view_probe))
            }
            45 => {
                let text = Rc::new(RefCell::new(String::new()));
                let state = Rc::new(RefCell::new(AutocompleteState {
                    highlight: 0,
                    placeholder: Some("City".to_string()),
                    ..AutocompleteState::default()
                }));
                let view_text = Rc::clone(&text);
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| autocomplete_demo(context, &view_text, &view_state, &view_probe))
            }
            46 => {
                let tokens = Rc::new(RefCell::new(vec![
                    "go".to_string(),
                    "native".to_string(),
                ]));
                let input = Rc::new(RefCell::new(String::new()));
                let view_tokens = Rc::clone(&tokens);
                let view_input = Rc::clone(&input);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| token_demo(context, &view_tokens, &view_input, &view_probe))
            }
            47 => {
                let text = Rc::new(RefCell::new("Quarterly report.pdf".to_string()));
                let state = Rc::new(RefCell::new(EditableTextState::new()));
                let view_text = Rc::clone(&text);
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| editable_demo(context, &view_text, &view_state, &view_probe))
            }
            48 => {
                let finding = Rc::new(Cell::new(true));
                let query = Rc::new(RefCell::new("fox".to_string()));
                let current = Rc::new(Cell::new(0_usize));
                let view_finding = Rc::clone(&finding);
                let view_query = Rc::clone(&query);
                let view_current = Rc::clone(&current);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    find_demo(context, &view_finding, &view_query, &view_current, &view_probe)
                })
            }
            49 => {
                let state = Rc::new(RefCell::new(RichTextEditorDemoState::default()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    rich_text_editor_demo(context, &view_state, &view_probe)
                })
            }
            50 => {
                let state = Rc::new(RefCell::new(CodeEditorState::default()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| code_demo(context, &view_state, &view_probe))
            }
            51 => {
                let state = Rc::new(RefCell::new(DiffViewerState { layout: myrust_ui::DiffLayout::SideBySide }));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| diff_demo(context, &view_state, &view_probe))
            }
            52 => {
                let state = Rc::new(RefCell::new(CalendarState::new(2026, 10, Some(15))));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| calendar_demo(context, &view_state, &view_probe))
            }
            53 => {
                // MyGo's demo starts from 4/12/1990 and 10/15/2026 at 14:30
                // (`website/docs-demo/dates.go` date-input).
                let birthday = Rc::new(RefCell::new("1990-04-12".to_string()));
                let meeting = Rc::new(RefCell::new("2026-10-15".to_string()));
                let state = Rc::new(RefCell::new(DateInputState::default()));
                let meeting_state = Rc::new(RefCell::new(DateInputState::default()));
                let view_birthday = Rc::clone(&birthday);
                let view_meeting = Rc::clone(&meeting);
                let view_state = Rc::clone(&state);
                let view_meeting_state = Rc::clone(&meeting_state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    date_demo(
                        context,
                        &view_birthday,
                        &view_state,
                        &view_meeting,
                        &view_meeting_state,
                        &view_probe,
                    )
                })
            }
            54 => {
                let text = Rc::new(RefCell::new("07:30".to_string()));
                let view_text = Rc::clone(&text);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| time_demo(context, &view_text, &view_probe))
            }
            55 => {
                // MyGo's color-picker demo starts from #FDF2F8, the panel's
                // own background, and #E11D48 as the tint.
                let background = Color::rgb(0xfd, 0xf2, 0xf8);
                let color = Rc::new(RefCell::new(background));
                let tint = Rc::new(RefCell::new(Color::rgb(0xe1, 0x1d, 0x48)));
                let state = Rc::new(RefCell::new(ColorPanelState::new(background)));
                let view_color = Rc::clone(&color);
                let view_tint = Rc::clone(&tint);
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    color_demo(context, &view_color, &view_tint, &view_state, &view_probe)
                })
            }
            56 => {
                let state = Rc::new(RefCell::new(ListState {
                    selected_id: "Beach.jpg".to_string(),
                }));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| list_demo(context, &view_state, &view_probe))
            }
            57 => {
                let state = Rc::new(RefCell::new(TableState {
                    // MyGo starts with Budget 2026.xlsx chosen, sorted by
                    // name, the demo's `row = 2` and `SortOrder{Column: "Name"}`.
                    selected_id: "Budget 2026.xlsx".to_string(),
                    sort_column: Some(0),
                    sort_direction: SortDirection::Ascending,
                }));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| table_demo(context, &view_state, &view_probe))
            }
            58 => {
                let state = Rc::new(RefCell::new(TreeState {
                    selected_id: "mygo/ui/button.go".to_string(),
                }));
                // MyGo opens `mygo` and `mygo/ui` and chooses button.go.
                let expanded = Rc::new(RefCell::new(vec![
                    "mygo".to_string(),
                    "mygo/ui".to_string(),
                ]));
                let chosen = Rc::new(RefCell::new("mygo/ui/button.go".to_string()));
                let view_state = Rc::clone(&state);
                let view_expanded = Rc::clone(&expanded);
                let view_chosen = Rc::clone(&chosen);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| tree_demo(context, &view_state, &view_expanded, &view_probe, &view_chosen))
            }
            59 => {
                // MyGo's outline demo: a Segmented control switching between
                // a plain outline and one in table columns, both 206 tall in
                // a bordered box (`website/docs-demo/collections.go`).
                let mode = Rc::new(Cell::new(0_usize));
                let expanded = Rc::new(RefCell::new(vec![
                    "mygo".to_string(),
                    "mygo/ui".to_string(),
                ]));
                let chosen = Rc::new(RefCell::new("mygo/ui".to_string()));
                let view_mode = Rc::clone(&mode);
                let view_expanded = Rc::clone(&expanded);
                let view_chosen = Rc::clone(&chosen);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    outline_demo(
                        context,
                        &view_mode,
                        &view_expanded,
                        &view_chosen,
                        &view_probe,
                    )
                })
            }
            60 => {
                // MyGo's grid view starts with the fourth photo chosen.
                let selected = Rc::new(Cell::new(3_usize));
                let view_selected = Rc::clone(&selected);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    grid_view_demo(context, &view_selected, &view_probe)
                })
            }
            61 => {
                let scroll = Rc::new(Cell::new(0.0_f32));
                let view_scroll = Rc::clone(&scroll);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| scroll_demo(context, &view_scroll, &view_probe))
            }
            62 => {
                let state = Rc::new(RefCell::new(GridDemoState {
                    tiles: 6,
                    columns: 3,
                    email: "ada@example.com".to_string(),
                    bio: String::new(),
                }));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| grid_demo(context, &view_state, &view_probe))
            }
            63 => {
                let state = Rc::new(RefCell::new(SplitDemoState::default()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| split_demo(context, &view_state, &view_probe))
            }
            64 => {
                // MyGo opens Advanced, with Verbose logging off and
                // Experimental features on, and closes Details
                // (`website/docs-demo/containers.go`).
                let advanced = Rc::new(Cell::new(true));
                let verbose = Rc::new(Cell::new(false));
                let experimental = Rc::new(Cell::new(true));
                let details = Rc::new(Cell::new(false));
                let view_advanced = Rc::clone(&advanced);
                let view_verbose = Rc::clone(&verbose);
                let view_experimental = Rc::clone(&experimental);
                let view_details = Rc::clone(&details);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    collapsible_demo(
                        context,
                        &view_advanced,
                        &view_verbose,
                        &view_experimental,
                        &view_details,
                        &view_probe,
                    )
                })
            }
            65 => {
                // MyGo starts with General open, a device name and three
                // switches (`website/docs-demo/containers.go`).
                let section = Rc::new(RefCell::new(0usize));
                let name = Rc::new(RefCell::new("Ada's Mac".to_string()));
                let sync = Rc::new(Cell::new(true));
                let analytics = Rc::new(Cell::new(false));
                let crashes = Rc::new(Cell::new(true));
                let beta = Rc::new(Cell::new(false));
                let view_section = Rc::clone(&section);
                let view_name = Rc::clone(&name);
                let view_sync = Rc::clone(&sync);
                let view_analytics = Rc::clone(&analytics);
                let view_crashes = Rc::clone(&crashes);
                let view_beta = Rc::clone(&beta);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    accordion_demo(
                        context,
                        &view_section,
                        &view_name,
                        &view_sync,
                        &view_analytics,
                        &view_crashes,
                        &view_beta,
                        &view_probe,
                    )
                })
            }
            66 => {
                let state = Rc::new(RefCell::new(FormDemoState::default()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| form_demo(context, &view_state, &view_probe))
            }
            67 => {
                let selected = Rc::new(RefCell::new("b".to_string()));
                let view_selected = Rc::clone(&selected);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| sidebar_demo(context, &view_selected, &view_probe))
            }
            68 => {
                let count = Rc::new(Cell::new(0_i32));
                let view_count = Rc::clone(&count);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| breadcrumbs_demo(context, &view_count, &view_probe))
            }
            69 => {
                // MyGo's router starts at "/" (`website/docs-demo/navigation.go`
                // `ui.NewRouter("/")`).
                let router_state = Rc::new(RefCell::new(RouterState::new("/")));
                let view_router = Rc::clone(&router_state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| history_demo(context, &view_router, &view_probe))
            }
            70 => {
                let open = Rc::new(Cell::new(false));
                let result = Rc::new(Cell::new(0_i32));
                let name = Rc::new(RefCell::new("Notes".to_string()));
                let draft = Rc::new(RefCell::new(String::new()));
                let view_open = Rc::clone(&open);
                let view_result = Rc::clone(&result);
                let view_name = Rc::clone(&name);
                let view_draft = Rc::clone(&draft);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    dialog_demo(
                        context,
                        &view_open,
                        &view_result,
                        &view_name,
                        &view_draft,
                        &view_probe,
                    )
                })
            }
            71 => {
                let open = Rc::new(Cell::new(false));
                let result = Rc::new(Cell::new(0_i32));
                let view_open = Rc::clone(&open);
                let view_result = Rc::clone(&result);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| alert_demo(context, &view_open, &view_result, &view_probe))
            }
            72 => {
                let state = Rc::new(RefCell::new(PopoverDemoState::default()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| popover_demo(context, &view_state, &view_probe))
            }
            73 => {
                let seen = Rc::new(Cell::new(false));
                let view_seen = Rc::clone(&seen);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| tooltip_demo(context, &view_seen, &view_probe))
            }
            74 => {
                let queue = Rc::new(RefCell::new(ToastQueue::new()));
                let view_queue = Rc::clone(&queue);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| toast_demo(context, &view_queue, &view_probe))
            }
            75 => {
                let done = Rc::new(Cell::new(2_i32));
                let view_done = Rc::clone(&done);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| progress_demo(context, &view_done, &view_probe))
            }
            76 => {
                let spinning = Rc::new(Cell::new(true));
                let view_spinning = Rc::clone(&spinning);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| spinner_demo(context, &view_spinning, &view_probe))
            }
            77 => {
                let state = Rc::new(RefCell::new(MeterDemoState::default()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| meter_demo(context, &view_state, &view_probe))
            }
            78 => {
                let unread = Rc::new(Cell::new(3_i32));
                let view_unread = Rc::clone(&unread);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| badge_demo(context, &view_unread, &view_probe))
            }
            79 => {
                let state = Rc::new(RefCell::new(ChartDemoState::default()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| chart_demo(context, &view_state, &view_probe))
            }
            80 => {
                let fit = Rc::new(Cell::new(0_usize));
                let gray = Rc::new(Cell::new(false));
                let view_fit = Rc::clone(&fit);
                let view_gray = Rc::clone(&gray);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    image_demo(context, &view_fit, &view_gray, &view_probe)
                })
            }
            81 => {
                let name = Rc::new(Cell::new(false));
                let view_name = Rc::clone(&name);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| icon_demo(context, &view_name, &view_probe))
            }
            82 => {
                let name = Rc::new(RefCell::new("Ada Lovelace".to_string()));
                let view_name = Rc::clone(&name);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| avatar_demo(context, &view_name, &view_probe))
            }
            83 => {
                let view_frame = Rc::clone(&frame);
                let view_probe = Rc::clone(&probe);
                let view_toasts = Rc::clone(toasts.as_ref().expect("showcase toasts"));
                NativeUi::new(move |context| {
                    showcase_demo(context, &view_frame, &view_probe, &view_toasts)
                })
            }
            _ => {
                let done = Rc::new(Cell::new(2_i32));
                let view_done = Rc::clone(&done);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| progress_demo(context, &view_done, &view_probe))
            }
        }
        .with_theme({
            let mut theme = Theme::light();
            theme.background = Color::rgb(255, 255, 255);
            theme.surface = Color::rgb(244, 244, 245);
            theme.foreground = Color::rgb(24, 24, 27);
            theme.muted = Color::rgb(113, 113, 122);
            theme.accent = Color::rgb(37, 99, 235);
            theme.on_accent = Color::rgb(255, 255, 255);
            theme.border = Color::rgb(217, 217, 222);
            theme
        })
        .with_preview(true);
        Self {
            slug,
            ui,
            frame,
            selected,
            probe,
            dropped,
            drag_hover,
            toasts,
        }
    }
}

fn tabs_demo(
    context: &mut Context,
    selected: &Rc<RefCell<usize>>,
    notify: &Rc<Cell<bool>>,
    sounds: &Rc<Cell<bool>>,
    dark: &Rc<Cell<bool>>,
    compact: &Rc<Cell<bool>>,
    logging: &Rc<Cell<bool>>,
    _frame: &Rc<Cell<u64>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    context.set_widgets_disabled(DEMO_DISABLED.load(Ordering::Relaxed));
    column(context).width(712.0).gap(12.0).children(|context| {
        let mut item = selected.borrow_mut();
        let state = tabs(
            context,
            "docs-demo-tabs",
            &mut item,
            &["General", "Appearance", "Advanced"],
        );
        column(context)
            .height(64.0)
            .gap(10.0)
            .padding_x(4.0)
            .children(|context| {
                let mut toggle = |name: &str, label: &str, cell: &Rc<Cell<bool>>| {
                    let mut checked = cell.get();
                    if checkbox_labeled(context, name, label, &mut checked).changed {
                        cell.set(checked);
                    }
                };
                match *item {
                    0 => {
                        toggle("docs-demo-tab-notify", "Show notifications", notify);
                        toggle("docs-demo-tab-sounds", "Play sounds", sounds);
                    }
                    1 => {
                        toggle("docs-demo-tab-dark", "Dark sidebar", dark);
                        toggle("docs-demo-tab-compact", "Compact rows", compact);
                    }
                    _ => {
                        toggle("docs-demo-tab-logging", "Write a debug log", logging);
                        text(context, "Changes apply after a restart")
                            .font_size(12.0)
                            .color(theme.muted);
                    }
                }
            });
        probe.set(state.selected as i32);
    });
}

fn views_demo(context: &mut Context, counter: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    let value = counter.get();
    column(context)
        .fill()
        .padding(18.0)
        .gap(10.0)
        .children(|context| {
            text(context, "Views")
                .font_size(22.0)
                .bold()
                .color(Color::rgb(241, 245, 249));
            text(context, "A view is a function from state to widgets.")
                .font_size(12.0)
                .color(Color::rgb(148, 163, 184));
            text(context, &format!("count = {value}"))
                .font_size(18.0)
                .color(Color::rgb(34, 211, 238));
            if button(context, "Increment").clicked {
                counter.set(value + 1);
                probe.set(value + 1);
            }
            text(context, "Click the button; it is created again each frame.")
                .font_size(12.0)
                .color(Color::rgb(100, 116, 139));
        });
}

fn layout_demo(context: &mut Context, wide: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let is_wide = wide.get();
    column(context)
        .fill()
        .padding(18.0)
        .gap(10.0)
        .children(|context| {
            text(context, "Layout")
                .font_size(22.0)
                .bold()
                .color(Color::rgb(241, 245, 249));
            if button(context, "Toggle layout").clicked {
                wide.set(!is_wide);
                probe.set(if is_wide { 0 } else { 1 });
            }
            if is_wide {
                row(context).gap(8.0).children(|context| boxes(context));
            } else {
                column(context).gap(8.0).children(|context| boxes(context));
            }
            text(context, if is_wide { "row layout" } else { "column layout" })
                .font_size(12.0)
                .color(Color::rgb(148, 163, 184));
        });
}

fn boxes(context: &mut Context) {
    for _ in 0..3 {
        container(context).size(54.0, 32.0).draw([DrawCommand::Rect {
            x: 0.0,
            y: 0.0,
            width: 54.0,
            height: 32.0,
            color: Color::rgb(34, 211, 238),
        }]);
    }
}

fn text_demo(context: &mut Context, large: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let big = large.get();
    column(context)
        .fill()
        .padding(18.0)
        .gap(10.0)
        .children(|context| {
            text(context, "Text")
                .font_size(if big { 30.0 } else { 18.0 })
                .bold()
                .color(Color::rgb(241, 245, 249));
            text(context, "Font size, weight, color and max width are caller options.")
                .font_size(13.0)
                .color(Color::rgb(148, 163, 184));
            if button(context, "Toggle size").clicked {
                large.set(!big);
                probe.set(if big { 0 } else { 1 });
            }
            text(context, if big { "Large text" } else { "Small text" })
                .font_size(if big { 24.0 } else { 14.0 })
                .color(Color::rgb(34, 211, 238));
        });
}

fn styling_demo(context: &mut Context, light: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let is_light = light.get();
    let fg = if is_light {
        Color::rgb(15, 23, 42)
    } else {
        Color::rgb(241, 245, 249)
    };
    let bg = if is_light {
        Color::rgb(248, 250, 252)
    } else {
        Color::rgb(10, 13, 20)
    };
    let size = context.available_size();
    column(context)
        .fill()
        .padding(18.0)
        .gap(10.0)
        .draw([DrawCommand::Rect {
            x: 0.0,
            y: 0.0,
            width: size.width,
            height: size.height,
            color: bg,
        }])
        .children(|context| {
            text(context, "Styling")
                .font_size(22.0)
                .bold()
                .color(fg);
            if button(context, "Toggle theme").clicked {
                light.set(!is_light);
                probe.set(if is_light { 0 } else { 1 });
            }
            text(context, "A Theme is passed to NativeUi; explicit colors handle local looks.")
                .font_size(12.0)
                .color(if is_light {
                    Color::rgb(71, 85, 105)
                } else {
                    Color::rgb(148, 163, 184)
                });
        });
}

fn input_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    column(context)
        .fill()
        .padding(18.0)
        .gap(10.0)
        .children(|context| {
            text(context, "Input")
                .font_size(22.0)
                .bold()
                .color(Color::rgb(241, 245, 249));
            text(context, "Click the field, then type. Tab/arrow keys move focus and caret.")
                .font_size(12.0)
                .color(Color::rgb(148, 163, 184));
            let mut text_value = value.borrow_mut();
            let field = text_field(context, "docs-demo-input", &mut text_value);
            if field.changed {
                probe.set(text_value.len() as i32);
            }
            text(context, &format!("{} characters", text_value.len()))
                .font_size(12.0)
                .color(Color::rgb(34, 211, 238));
        });
}

fn drag_demo(
    context: &mut Context,
    files: &Rc<RefCell<Vec<DroppedFile>>>,
    hovering: &Rc<Cell<bool>>,
    probe: &Rc<Cell<i32>>,
) {
    // MyGo's drop zone: 440 by 150, centered on both axes with an icon and
    // its hint, over the line saying what a drop is read with. A drag over
    // the zone turns its border to the accent, as `FileDragOver` does.
    let theme = context.theme().clone();
    let files = files.borrow();
    probe.set(files.len() as i32);
    let border = if hovering.get() {
        theme.accent
    } else {
        theme.border
    };
    column(context)
        .width(440.0)
        .height(150.0)
        .padding_x(17.0)
        .padding_y(13.0)
        .gap(8.0)
        .justify(JustifyContent::Center)
        .background(theme.surface, 10.0)
        .border(border, 10.0)
        .children(|context| {
            if files.is_empty() {
                column(context)
                    .width(406.0)
                    .gap(6.0)
                    .align(AlignItems::Center)
                    .children(|context| {
                        icon(context, "docs-demo-drop-icon", "file", 14.0, theme.muted);
                        text(context, "Drop files here").color(theme.muted);
                    });
                return;
            }
            for (index, file) in files.iter().enumerate() {
                if index == 3 {
                    text(context, &format!("and {} more", files.len() - index))
                        .font_size(12.0)
                        .color(theme.muted);
                    break;
                }
                row(context)
                    .gap(8.0)
                    .align(AlignItems::Center)
                    .children(|context| {
                        icon(
                            context,
                            &format!("docs-demo-drop-file-{index}"),
                            "file",
                            14.0,
                            theme.foreground,
                        );
                        text(context, &file.name).single_line().max_width(160.0);
                        text(context, &drag_size(file.size))
                            .font_size(12.0)
                            .color(theme.muted)
                            .single_line();
                        if !file.line.is_empty() {
                            text(context, &format!("“{}”", file.line))
                                .font_size(12.0)
                                .color(theme.muted)
                                .single_line()
                                .max_width(180.0);
                        }
                    });
            }
        });
    text(
        context,
        "Read with os.ReadFile from the paths of the drop",
    )
    .font_size(12.0)
    .color(theme.muted);
}

/// Formats a byte count as MyGo's drag demo does (`dragSize`).
fn drag_size(bytes: usize) -> String {
    if bytes < 1000 {
        format!("{bytes} bytes")
    } else if bytes < 1_000_000 {
        format!("{:.1} KB", bytes as f64 / 1000.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    }
}

static DEMO_DISABLED: AtomicBool = AtomicBool::new(false);

#[no_mangle]
pub extern "C" fn demo_set_disabled(disabled: u32) {
    DEMO_DISABLED.store(disabled != 0, Ordering::Relaxed);
}

#[no_mangle]
pub extern "C" fn demo_get_disabled() -> u32 {
    DEMO_DISABLED.load(Ordering::Relaxed) as u32
}

const MARKDOWN: &str = "### A reply, streamed\n\n\
The parser reads **CommonMark** and *GitHub's* extensions as text comes. Only the block being written is parsed again, so `**bol` is already **bold**.\n\n\
- [x] tables, task lists and ~~strikethrough~~\n\
- [ ] [links](https://github.com/egoist/mygo) and footnotes\n\n\
```go\n\
markdown.Stream(c, reply, &app.md)\n\
```\n\n\
| Strategy | Per token |\n\
|---|--:|\n\
| Re-parse all | O(n) |\n\
| **MyGo** | O(open block) |\n\n\
Text comes in at an even pace and fades in. Pick how much shows at a time above, and stream it again.\n";

fn markdown_demo(
    context: &mut Context,
    frame: &Rc<Cell<u64>>,
    _copied: &Rc<Cell<bool>>,
    mode: &Rc<Cell<usize>>,
    probe: &Rc<Cell<i32>>,
) {
    // MyGo's markdown demo starts with the whole reply and reveals it again
    // when "Stream it" is pressed; frame keeps the line count revealed, or 0
    // for all of it.
    let mut selected = mode.get().min(3);
    let lines: Vec<&str> = MARKDOWN.lines().collect();
    let mut reveal = frame.get() as usize;
    let mut body = MARKDOWN.to_string();
    if reveal > 0 {
        reveal = (reveal + [1, 2, 8, 12][selected]).min(lines.len());
        frame.set(reveal as u64);
        if reveal < lines.len() {
            context.request_animation_frame();
        }
        body = lines[..reveal].join("\n");
    }
    column(context).width(712.0).gap(12.0).children(|context| {
        row(context).gap(12.0).align(AlignItems::Center).children(|context| {
            if context.button_primary("Stream it").clicked {
                frame.set(1);
                probe.set(1);
            }
            let response = segmented(
                context,
                "docs-demo-md-mode",
                &mut selected,
                &["Word", "Sentence", "Line", "Paragraph"],
            );
            if response.changed {
                mode.set(selected);
                frame.set(1);
                probe.set(selected as i32 + 2);
            }
        });
        scroll_view(context, "docs-demo-markdown", 357.0, |context| {
            context.markdown("docs-demo-markdown-body", &body);
        });
    });
}

fn font() -> Option<Arc<FontFace>> {
    FONT.get_or_init(|| {
        let bytes = include_bytes!("../assets/geist.ttf").to_vec();
        myrust_ui::FontRegistry::load_bytes(bytes).ok().map(|face| {
            let font = Arc::new(face);
            myrust_ui::install_default_font(Arc::clone(&font));
            font
        })
    })
    .clone()
}

#[derive(Clone, Copy)]
struct CommandMatrix {
    m11: f32,
    m12: f32,
    m21: f32,
    m22: f32,
    tx: f32,
    ty: f32,
}

impl CommandMatrix {
    fn scale(value: f32) -> Self {
        Self { m11: value, m12: 0.0, m21: 0.0, m22: value, tx: 0.0, ty: 0.0 }
    }

    fn from_transform(transform: Transform) -> Self {
        let cos = transform.rotation.cos();
        let sin = transform.rotation.sin();
        Self {
            m11: cos * transform.scale.x,
            m12: -sin * transform.scale.y,
            m21: sin * transform.scale.x,
            m22: cos * transform.scale.y,
            tx: transform.translation.x,
            ty: transform.translation.y,
        }
    }

    fn then(self, child: Self) -> Self {
        Self {
            m11: self.m11 * child.m11 + self.m12 * child.m21,
            m12: self.m11 * child.m12 + self.m12 * child.m22,
            m21: self.m21 * child.m11 + self.m22 * child.m21,
            m22: self.m21 * child.m12 + self.m22 * child.m22,
            tx: self.m11 * child.tx + self.m12 * child.ty + self.tx,
            ty: self.m21 * child.tx + self.m22 * child.ty + self.ty,
        }
    }

    fn point(self, x: f32, y: f32) -> (f32, f32) {
        (self.m11 * x + self.m12 * y + self.tx, self.m21 * x + self.m22 * y + self.ty)
    }

    fn rect_aabb(self, x: f32, y: f32, width: f32, height: f32) -> [f32; 4] {
        let points = [
            self.point(x, y),
            self.point(x + width, y),
            self.point(x + width, y + height),
            self.point(x, y + height),
        ];
        let left = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let top = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let right = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let bottom = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        [left, top, right - left, bottom - top]
    }
}

fn push_color(buffer: &mut Vec<u8>, color: Color) {
    buffer.extend_from_slice(&[color.r, color.g, color.b, color.a]);
}

/// Measures one line as the browser will draw it: at the run's own weight,
/// slant and family, since MyGo styles text with CSS weights and the browser
/// resolves each to its own face.
fn text_width(
    font: Option<&FontFace>,
    text: &str,
    font_size: f32,
    weight: u16,
    italic: bool,
    mono: bool,
    family: Option<&str>,
) -> f32 {
    let font_size = font_size.max(1.0);
    if let Some(family) = family {
        if let Some(width) =
            myrust_ui::host_text_width_family(text, font_size, italic, mono, weight, family)
        {
            return width;
        }
    }
    if let Some(width) = myrust_ui::host_text_width_weighted(text, font_size, italic, mono, weight) {
        return width;
    }
    match font {
        Some(font) => text
            .chars()
            .map(|character| font.glyph_advance(character, font_size))
            .sum(),
        None => text.chars().count() as f32 * font_size * 0.6,
    }
}

/// The distance between the baselines of a run's lines, as MyGo lays them:
/// the browser font's ascent plus descent, which the monospace family makes
/// a pixel shorter than the sans one (`internal/text/layout.go`
/// metricsOf, `internal/text/canvas_js.go` font).
fn text_line_height(font: Option<&FontFace>, font_size: f32, mono: bool) -> f32 {
    let font_size = font_size.max(1.0);
    if let (Some(ascent), Some(descent)) = (
        myrust_ui::host_text_ascent(font_size, false, mono, false),
        myrust_ui::host_text_descent(font_size, false, mono, false),
    ) {
        return ascent + descent;
    }
    match font {
        Some(font) => font.line_height(font_size),
        None => font_size * 1.25,
    }
}

fn serialize_commands(commands: &[DrawCommand], dpr: f32, font: Option<&FontFace>) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut transform = CommandMatrix::scale(dpr);
    let mut transform_stack: Vec<CommandMatrix> = Vec::new();
    for command in commands {
        match command {
            DrawCommand::TransformPush { transform: local } => {
                transform_stack.push(transform);
                let local = CommandMatrix::from_transform(*local);
                transform = transform.then(local);
                buffer.push(9);
                for value in [local.m11, local.m12, local.m21, local.m22, local.tx, local.ty] {
                    buffer.extend_from_slice(&value.to_le_bytes());
                }
            }
            DrawCommand::TransformPop => {
                if transform_stack.pop().is_some() {
                    buffer.push(10);
                }
            }
            DrawCommand::ScissorPush { rect } => {
                let [x, y, width, height] = transform.rect_aabb(rect.x, rect.y, rect.width, rect.height);
                // MyGo's pushClip snaps the clip's box to whole device
                // pixels before the scene clips to it (`ui/paint.go`
                // pushClip: `Rect: p.snap(r)`), so a box on a fraction of a
                // pixel keeps one row more or less of what it holds.
                let x0 = x.round();
                let y0 = y.round();
                let x1 = (x + width).round();
                let y1 = (y + height).round();
                buffer.push(7);
                for value in [x0, y0, x1 - x0, y1 - y0] {
                    buffer.extend_from_slice(&value.to_le_bytes());
                }
            }
            DrawCommand::ScissorPop => buffer.push(8),
            DrawCommand::Rect { x, y, width, height, color } => {
                buffer.push(0);
                for value in [*x, *y, *width, *height] { buffer.extend_from_slice(&value.to_le_bytes()); }
                push_color(&mut buffer, *color);
            }
            DrawCommand::RoundedRect { x, y, width, height, radius, color } => {
                buffer.push(1);
                for value in [*x, *y, *width, *height, radius.top_left, radius.top_right, radius.bottom_right, radius.bottom_left] {
                    buffer.extend_from_slice(&value.to_le_bytes());
                }
                push_color(&mut buffer, *color);
            }
            DrawCommand::Circle { x, y, radius, color } => {
                buffer.push(2);
                for value in [*x, *y, *radius] { buffer.extend_from_slice(&value.to_le_bytes()); }
                push_color(&mut buffer, *color);
            }
            DrawCommand::Shadow {
                x,
                y,
                width,
                height,
                radius,
                offset_x,
                offset_y,
                blur,
                color,
            } => {
                buffer.push(11);
                for value in [*x, *y, *width, *height, *radius, *offset_x, *offset_y, *blur] {
                    buffer.extend_from_slice(&value.to_le_bytes());
                }
                push_color(&mut buffer, *color);
            }
            DrawCommand::Line { x1, y1, x2, y2, width, color } => {
                buffer.push(3);
                for value in [*x1, *y1, *x2, *y2, *width] { buffer.extend_from_slice(&value.to_le_bytes()); }
                push_color(&mut buffer, *color);
            }
            DrawCommand::Text {
                x,
                y,
                text,
                font_size,
                weight,
                italic,
                mono,
                family,
                color,
                max_width,
                alignment,
            } => {
                let mut draw_x = *x;
                if let Some(max_width) = *max_width {
                    if max_width.is_finite() && max_width > 0.0 {
                        let widest = text
                            .split('\n')
                            .map(|line| text_width(font, line, *font_size, *weight, *italic, *mono, family.as_deref()))
                            .fold(0.0_f32, f32::max);
                        if widest <= max_width {
                            draw_x += match alignment {
                                myrust_ui::TextAlignment::Center => (max_width - widest) / 2.0,
                                myrust_ui::TextAlignment::End => max_width - widest,
                                myrust_ui::TextAlignment::Start => 0.0,
                            };
                        }
                    }
                }
                let line_height = text_line_height(font, *font_size, *mono);
                for (index, line) in text.split('\n').enumerate() {
                    buffer.push(4);
                    for value in [draw_x, *y + index as f32 * line_height, *font_size] {
                        buffer.extend_from_slice(&value.to_le_bytes());
                    }
                    buffer.extend_from_slice(&weight.to_le_bytes());
                    buffer.push(u8::from(*italic));
                    buffer.push(u8::from(*mono));
                    push_color(&mut buffer, *color);
                    buffer.extend_from_slice(&max_width.unwrap_or(f32::NAN).to_le_bytes());
                    buffer.push(match alignment {
                        myrust_ui::TextAlignment::Start => 0,
                        myrust_ui::TextAlignment::Center => 1,
                        myrust_ui::TextAlignment::End => 2,
                    });
                    match family {
                        Some(name) => {
                            buffer.push(1);
                            let bytes = name.as_bytes();
                            buffer.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                            buffer.extend_from_slice(bytes);
                        }
                        None => buffer.push(0),
                    }
                    let bytes = line.as_bytes();
                    buffer.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                    buffer.extend_from_slice(bytes);
                }
            }
            DrawCommand::SvgPath {
                x,
                y,
                width,
                height,
                path,
                fill,
                stroke,
                stroke_width,
            } => {
                buffer.push(5);
                for value in [*x, *y, *width, *height] {
                    buffer.extend_from_slice(&value.to_le_bytes());
                }
                match fill {
                    Some(color) => {
                        buffer.push(1);
                        push_color(&mut buffer, *color);
                    }
                    None => buffer.push(0),
                }
                match stroke {
                    Some(color) => {
                        buffer.push(1);
                        push_color(&mut buffer, *color);
                    }
                    None => buffer.push(0),
                }
                buffer.extend_from_slice(&stroke_width.to_le_bytes());
                let bytes = path.as_bytes();
                buffer.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                buffer.extend_from_slice(bytes);
            }
            DrawCommand::LinearGradient { x, y, width, height, radius, start, end, stops } => {
                buffer.push(6);
                for value in [*x, *y, *width, *height, start.x, start.y, end.x, end.y] {
                    buffer.extend_from_slice(&value.to_le_bytes());
                }
                for value in [
                    radius.top_left,
                    radius.top_right,
                    radius.bottom_right,
                    radius.bottom_left,
                ] {
                    buffer.extend_from_slice(&value.to_le_bytes());
                }
                buffer.extend_from_slice(&(stops.len() as u16).to_le_bytes());
                for stop in stops {
                    buffer.extend_from_slice(&stop.offset.to_le_bytes());
                    push_color(&mut buffer, stop.color);
                }
            }
            DrawCommand::Image { .. } => {}
        }
    }
    buffer
}

#[no_mangle]
pub extern "C" fn demo_render(slug: u32, width: f32, height: f32, dpr: f32) -> u32 {
    if !width.is_finite() || !height.is_finite() || width < 1.0 || height < 1.0 {
        return 0;
    }
    let dpr = if dpr.is_finite() { dpr.clamp(1.0, 3.0) } else { 1.0 };
    let ui_font = font();
    let rendered = APP.with(|cell| {
        let mut app = cell.borrow_mut();
        if app.as_ref().map(|app| app.slug) != Some(slug) {
            *app = Some(DemoApp::new(slug));
        }
        let Some(app) = app.as_mut() else { return false; };
        app.ui.set_pixel_scale(dpr);
        let frame = match app.ui.render(Size::new(width, height)) {
            Ok(frame) => frame,
            Err(_) => return false,
        };
        let bytes = serialize_commands(&frame.display.commands, dpr, ui_font.as_deref());
        COMMAND_BUFFER.with(|buffer| *buffer.borrow_mut() = bytes);
        true
    });
    if !rendered {
        COMMAND_BUFFER.with(|buffer| buffer.borrow_mut().clear());
        return 0;
    }
    1
}

/// Returns a pointer to the serialized canvas2D command buffer.
#[no_mangle]
pub extern "C" fn demo_command_ptr() -> *const u8 {
    COMMAND_BUFFER.with(|buffer| buffer.borrow().as_ptr())
}

/// Returns the byte length of the serialized canvas2D command buffer.
#[no_mangle]
pub extern "C" fn demo_command_len() -> u32 {
    COMMAND_BUFFER.with(|buffer| buffer.borrow().len() as u32)
}

fn home_page(context: &mut Context) {
    text(context, "Home page").font_size(16.0).color(Color::rgb(226, 232, 240));
}

fn about_page(context: &mut Context) {
    text(context, "About page").font_size(16.0).color(Color::rgb(226, 232, 240));
}

fn navigation_demo(context: &mut Context, route: &Rc<RefCell<RouterState>>, probe: &Rc<Cell<i32>>) {
    let mut state = route.borrow_mut();
    let pages = [Page::new("home", &home_page), Page::new("about", &about_page)];
    let buttons = back_forward_buttons(context, "docs-demo-nav-buttons", &mut state);
    if button(context, "Push About").clicked {
        state.push("about");
        probe.set(state.history().len() as i32);
    }
    if buttons.back_clicked || buttons.forward_clicked {
        probe.set(state.history().len() as i32 + 10);
    }
    router(context, "docs-demo-router", &mut state, &pages);
    text(context, &format!("page: {}", state.current()))
        .font_size(13.0)
        .color(Color::rgb(34, 211, 238));
}

fn overlays_demo(context: &mut Context, open: &Rc<RefCell<OverlayState>>, probe: &Rc<Cell<i32>>) {
    let mut state = open.borrow_mut();
    let trigger = button(context, "Open popover");
    if trigger.clicked {
        state.open = true;
        probe.set(1);
    }
    if state.open {
        overlay(
            context,
            "docs-demo-overlay",
            &mut state,
            trigger.bounds.unwrap_or_else(|| myrust_ui::Rect::new(18.0, 50.0, 120.0, 30.0)),
            |context| {
                text(context, "Overlay content").font_size(14.0).color(Color::rgb(241, 245, 249));
            },
        );
    }
    text(context, if state.open { "open; Escape closes" } else { "closed" })
        .font_size(12.0)
        .color(Color::rgb(148, 163, 184));
}

fn accessibility_demo(context: &mut Context, checked: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let mut value = checked.get();
    if checkbox(context, "docs-demo-checkbox", &mut value).changed {
        checked.set(value);
        probe.set(if value { 1 } else { 0 });
    }
    text(context, "CheckBox: label=checkbox, value=checked/unchecked")
        .font_size(12.0)
        .color(Color::rgb(148, 163, 184));
    text(context, "A Label node carries this visible string.")
        .font_size(12.0)
        .color(Color::rgb(148, 163, 184));
}

fn rtl_demo(context: &mut Context, rtl: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let is_rtl = rtl.get();
    if button(context, "Toggle direction").clicked {
        rtl.set(!is_rtl);
        probe.set(if is_rtl { 0 } else { 1 });
    }
    let direction = if is_rtl { Direction::Rtl } else { Direction::Ltr };
    context.with_direction(direction, |context| {
        row(context).gap(8.0).children(|context| {
            for label in ["One", "Two", "Three"] {
                text(context, label).font_size(15.0).color(Color::rgb(226, 232, 240));
            }
        });
    });
    text(context, if is_rtl { "RTL row" } else { "LTR row" })
        .font_size(12.0)
        .color(Color::rgb(148, 163, 184));
}

fn languages_demo(context: &mut Context, zh: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let is_zh = zh.get();
    if button(context, "Switch locale").clicked {
        zh.set(!is_zh);
        probe.set(if is_zh { 0 } else { 1 });
    }
    let locale = if is_zh { Locale::ZhCn } else { Locale::EnUs };
    text(context, &format_number(1234567.89, locale, 2).unwrap_or_default())
        .font_size(18.0)
        .color(Color::rgb(226, 232, 240));
    text(context, &format_date("2024-02-29", locale, DateStyle::Short).unwrap_or_default())
        .font_size(13.0)
        .color(Color::rgb(148, 163, 184));
}

fn drawing_demo(context: &mut Context, filled: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let is_filled = filled.get();
    if button(context, "Toggle mark").clicked {
        filled.set(!is_filled);
        probe.set(if is_filled { 0 } else { 1 });
    }
    let radius = if is_filled { 24.0 } else { 10.0 };
    container(context).size(64.0, 64.0).draw([DrawCommand::Circle {
        x: 32.0,
        y: 32.0,
        radius,
        color: Color::rgb(34, 211, 238),
    }]);
    spinner(context, "docs-demo-spinner");
    text(context, "Animation runs from the frame clock")
        .font_size(12.0)
        .color(Color::rgb(148, 163, 184));
}


fn transitions_demo(
    context: &mut Context,
    open: &Rc<Cell<bool>>,
    _state: &Rc<RefCell<TransitionState>>,
    _probe: &Rc<Cell<i32>>,
) {
    // MyGo's stage: 300 by 190, three 64-pixel tiles that open into a
    // detail view. The tiles are the resting state.
    let theme = context.theme().clone();
    let colors = [
        Color::rgb(37, 99, 235),
        Color::rgb(22, 163, 74),
        Color::rgb(217, 119, 6),
    ];
    // MyGo's column shrinks the stage to make room for the status line:
    // 190 less the 12 gap and the 15-pixel line.
    let stage_height = (context.available_size().height - 40.0 - 27.0).max(0.0);
    column(context)
        .size(300.0, stage_height)
        .padding(13.0)
        .border(theme.border, 12.0)
        .children(|context| {
            if open.get() {
                column(context).gap(10.0).children(|context| {
                    container(context)
                        .size(276.0, 116.0)
                        .background(colors[0], 16.0);
                    row(context).gap(10.0).align(AlignItems::Center).children(|context| {
                        text(context, "Tile 1").bold();
                    });
                });
            } else {
                row(context).gap(12.0).children(|context| {
                    for (index, color) in colors.iter().enumerate() {
                        let _ = index;
                        container(context).size(64.0, 64.0).background(*color, 12.0);
                    }
                });
            }
        });
    text(context, "Click a tile").font_size(12.0).color(theme.muted);
}

fn motion_demo(
    context: &mut Context,
    target: &Rc<Cell<f32>>,
    style: &Rc<Cell<usize>>,
    _probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    let mut selected = style.get().min(2);
    let response = segmented(
        context,
        "docs-demo-motion-style",
        &mut selected,
        &["Smooth", "Snappy", "Bouncy"],
    );
    if response.changed {
        style.set(selected);
    }
    let progress = target.get().clamp(0.0, 1.0);
    column(context)
        .size(300.0, 90.0)
        .border(theme.border, 12.0)
        .children(|context| {
            // MyGo's card is Absolute at Left(12) Top(15) inside the track,
            // whose 1px border takes layout space there: its content origin
            // is a pixel in, so the card lands at 13 and 16.
            let card = container(context)
                .size(96.0, 60.0)
                .center()
                .background(theme.accent, 10.0)
                .absolute(13.0, 16.0);
            card.children(|context| {
                let _ = progress;
                text(context, "Throw me")
                    .bold()
                    .font_size(12.0)
                    .color(theme.on_accent);
            });
        });
    text(context, "Drag and let go, or click")
        .font_size(12.0)
        .color(theme.muted);
}

fn transforms_demo(context: &mut Context, rotated: &Rc<Cell<bool>>, _probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let flipped = rotated.get();
    column(context)
        .size(220.0, 130.0)
        .center()
        .gap(4.0)
        .background(
            if flipped { theme.accent } else { theme.surface },
            14.0,
        )
        .border(
            if flipped {
                Color::TRANSPARENT
            } else {
                theme.border
            },
            14.0,
        )
        .children(|context| {
            text(context, if flipped { "Back" } else { "Front" })
                .bold()
                .color(if flipped {
                    theme.on_accent
                } else {
                    theme.foreground
                });
        });
    text(context, "Click the card").font_size(12.0).color(theme.muted);
}

fn custom_widget_demo(context: &mut Context, accent: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let on = accent.get();
    if button(context, "Toggle custom visual").clicked {
        accent.set(!on);
        probe.set(if on { 0 } else { 1 });
    }
    let color = if on {
        Color::rgb(236, 72, 153)
    } else {
        Color::rgb(34, 211, 238)
    };
    container(context).size(220.0, 64.0).draw([
        DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width: 220.0,
            height: 64.0,
            radius: myrust_ui::Radius::all(10.0),
            color: Color::rgb(17, 24, 39),
        },
        DrawCommand::Circle {
            x: 32.0,
            y: 32.0,
            radius: 18.0,
            color,
        },
        DrawCommand::Text {
            x: 62.0,
            y: 20.0,
            text: "container + draw + focusable".to_string(),
            font_size: 12.0,
            weight: 400,
            italic: false,
            mono: false,
            family: None,
            color: Color::rgb(226, 232, 240),
            max_width: Some(150.0),
            alignment: myrust_ui::TextAlignment::Start,
        },
    ]);
}

fn windows_demo(context: &mut Context, expanded: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let is_expanded = expanded.get();
    if button(context, "Zoom window").clicked {
        expanded.set(!is_expanded);
        probe.set(if is_expanded { 0 } else { 1 });
    }
    let width = if is_expanded { 280.0 } else { 180.0 };
    container(context).size(320.0, 160.0).draw([
        DrawCommand::RoundedRect {
            x: 0.0,
            y: 0.0,
            width,
            height: 120.0,
            radius: myrust_ui::Radius::all(10.0),
            color: Color::rgb(17, 24, 39),
        },
        DrawCommand::Rect {
            x: 0.0,
            y: 0.0,
            width,
            height: 28.0,
            color: Color::rgb(30, 41, 59),
        },
        DrawCommand::Circle {
            x: 14.0,
            y: 14.0,
            radius: 4.0,
            color: Color::rgb(255, 95, 87),
        },
        DrawCommand::Text {
            x: 34.0,
            y: 6.0,
            text: "NativeUi window".to_string(),
            font_size: 12.0,
            weight: 500,
            italic: false,
            mono: false,
            family: None,
            color: Color::rgb(226, 232, 240),
            max_width: Some(200.0),
            alignment: myrust_ui::TextAlignment::Start,
        },
        DrawCommand::Text {
            x: 14.0,
            y: 44.0,
            text: if is_expanded { "zoomed content" } else { "window content" }.to_string(),
            font_size: 13.0,
            weight: 400,
            italic: false,
            mono: false,
            family: None,
            color: Color::rgb(148, 163, 184),
            max_width: Some(width - 20.0),
            alignment: myrust_ui::TextAlignment::Start,
        },
    ]);
}


fn testing_demo(context: &mut Context, passed: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context)
        .fill()
        .padding(18.0)
        .gap(10.0)
        .children(|context| {
            text(context, "Testing").font_size(22.0).bold().color(Color::rgb(241, 245, 249));
            if button(context, "Run check").clicked {
                passed.set(!passed.get());
                probe.set(if passed.get() { 1 } else { 0 });
            }
            text(context, if passed.get() { "PASS: click + AccessNode" } else { "not run" })
                .font_size(14.0)
                .color(if passed.get() { Color::rgb(34, 197, 94) } else { Color::rgb(148, 163, 184) });
            text(context, "NativeUi::render returns a ViewFrame for assertions.")
                .font_size(12.0)
                .color(Color::rgb(148, 163, 184));
        });
}

fn inspector_demo(context: &mut Context, extra: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    if button(context, "Toggle tree detail").clicked {
        extra.set(!extra.get());
        probe.set(if extra.get() { 1 } else { 0 });
    }
    let mut children = vec![InspectorItem {
        id: "button-1".to_string(),
        role: Role::Button,
        label: "Save".to_string(),
        bounds: myrust_ui::Rect::new(0.0, 0.0, 80.0, 30.0),
        paint: PaintSummary { rounded_rects: 1, text: 1, ..PaintSummary::default() },
        children: Vec::new(),
    }];
    if extra.get() {
        children.push(InspectorItem {
            id: "label-1".to_string(),
            role: Role::Label,
            label: "Ready".to_string(),
            bounds: myrust_ui::Rect::new(0.0, 40.0, 80.0, 18.0),
            paint: PaintSummary { text: 1, ..PaintSummary::default() },
            children: Vec::new(),
        });
    }
    let tree = InspectorTree {
        roots: vec![InspectorItem {
            id: "window".to_string(),
            role: Role::Group,
            label: "window".to_string(),
            bounds: myrust_ui::Rect::new(0.0, 0.0, 240.0, 120.0),
            paint: PaintSummary { rounded_rects: 2, text: 3, ..PaintSummary::default() },
            children,
        }],
        total: PaintSummary { rects: 1, rounded_rects: 2, text: 3, ..PaintSummary::default() },
    };
    inspector(context, "docs-demo-inspector", &tree);
}

fn rendering_demo(context: &mut Context, filled: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let on = filled.get();
    if button(context, "Toggle fill").clicked {
        filled.set(!on);
        probe.set(if on { 0 } else { 1 });
    }
    let color = if on { Color::rgb(236, 72, 153) } else { Color::rgb(34, 211, 238) };
    container(context).size(240.0, 80.0).draw([
        DrawCommand::RoundedRect {
            x: 0.0, y: 0.0, width: 240.0, height: 80.0,
            radius: myrust_ui::Radius::all(10.0), color: Color::rgb(17, 24, 39),
        },
        DrawCommand::LinearGradient {
            x: 4.0, y: 4.0, width: 232.0, height: 72.0,
            radius: myrust_ui::Radius::all(0.0),
            start: myrust_ui::Point::new(0.0, 0.0), end: myrust_ui::Point::new(1.0, 0.0),
            stops: vec![
                myrust_ui::GradientStop { offset: 0.0, color },
                myrust_ui::GradientStop { offset: 1.0, color: Color::rgb(17, 24, 39) },
            ],
        },
    ]);
    text(context, "CPU display list · gradient + geometry")
        .font_size(12.0)
        .color(Color::rgb(148, 163, 184));
}

fn migration_demo(
    context: &mut Context,
    renamed: &Rc<Cell<bool>>,
    value: &Rc<RefCell<String>>,
    probe: &Rc<Cell<i32>>,
) {
    if button(context, "Rename field id").clicked {
        if renamed.get() {
            context.migrate_state("docs-demo-old-field", "docs-demo-new-field");
        } else {
            context.migrate_state("docs-demo-new-field", "docs-demo-old-field");
        }
        renamed.set(!renamed.get());
        probe.set(if renamed.get() { 1 } else { 0 });
    }
    let id = if renamed.get() { "docs-demo-new-field" } else { "docs-demo-old-field" };
    let mut text_value = value.borrow_mut();
    text_field(context, id, &mut text_value);
    text(context, "Text and cursor survive the id change.")
        .font_size(12.0)
        .color(Color::rgb(148, 163, 184));
}

fn performance_demo(context: &mut Context, stress: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    if button(context, "Toggle stress").clicked {
        stress.set(!stress.get());
        probe.set(if stress.get() { 1 } else { 0 });
    }
    if stress.get() {
        for index in 0..80 {
            container(context).size(4.0, 4.0).draw([DrawCommand::Rect {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 4.0,
                color: Color::rgb(34, 211, 238),
            }]);
            let _ = index;
        }
    }
    let stats = context.frame_stats();
    frame_stats_panel(context, "docs-demo-stats", stats);
    text(context, "FrameStats is the previous frame's layout/paint/total.")
        .font_size(12.0)
        .color(Color::rgb(148, 163, 184));
}


fn button_demo(
    context: &mut Context,
    count: &Rc<Cell<i32>>,
    editing: &Rc<Cell<bool>>,
    probe: &Rc<Cell<i32>>,
) {
    context.set_widgets_disabled(DEMO_DISABLED.load(Ordering::Relaxed));
    row(context).gap(8.0).children(|context| {
            if editing.get() {
                if button(context, "Cancel").clicked {
                    editing.set(false);
                    probe.set(0);
                }
            } else {
                context.set_widgets_disabled(true);
                button(context, "Cancel");
                context.set_widgets_disabled(false);
            }
            if button_primary_with_icon(context, "Save", "save").clicked {
                count.set(count.get() + 1);
                editing.set(true);
                probe.set(count.get());
            }
            if icon_button(context, "trash").clicked {
                count.set(0);
                probe.set(0);
            }
    });
    let status_text = if editing.get() {
            format!("Saved {} times", count.get())
        } else {
            "Canceled: Save again to edit".to_string()
        };
    text(context, &status_text).font_size(12.0).color(Color::rgb(113, 113, 122));
}

fn link_demo(context: &mut Context, details: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    if link(context, "docs-demo-privacy", "Privacy policy").clicked { probe.set(1); }
    row(context).gap(0.0).children(|context| {
        text(context, "Read ");
        if link(context, "docs-demo-guide", "the guide").clicked { probe.set(2); }
        text(context, " to get started.");
    });
    let label = if details.get() { "Hide details" } else { "Show details" };
    if link(context, "docs-demo-details", label).clicked { details.set(!details.get()); probe.set(3); }
    text(context, if details.get() { "A link with no URL is a text that is clicked" } else { "Links with URLs open a tab" })
        .font_size(12.0).color(theme.muted);
}

fn menu_demo(context: &mut Context, menu: &Rc<RefCell<MenuState>>, more: &Rc<RefCell<MenuState>>, selected: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let mut state = menu.borrow_mut();
    let mut more_state = more.borrow_mut();
    let mut chosen = String::new();
    row(context).gap(8.0).children(|context| {
        if let Some(index) = menu_button(context, "docs-demo-sort", "Sort by", &mut state, &["Name", "Date", "Size"]).selected {
            selected.set(index as i32); probe.set(index as i32 + 1);
        }
        if let Some(index) = menu_icon_button(context, "docs-demo-more", "dots", &mut more_state, &["Duplicate", "Delete"]).selected {
            chosen = ["Duplicate", "Delete"][index].to_string(); probe.set(10 + index as i32);
        }
    });
    let names = ["Name", "Date", "Size"];
    let status_text = if chosen.is_empty() {
        format!("Sorted by {}", names[selected.get().clamp(0, 2) as usize])
    } else {
        format!("Chose {chosen}")
    };
    text(context, &status_text)
        .font_size(12.0)
        .color(theme.muted);
}

struct NotesDemo {
    last: String,
    pinned: [bool; 3],
    deleted: [bool; 3],
    menus: Vec<ContextMenuState>,
}

impl NotesDemo {
    fn new() -> Self {
        Self {
            last: "Right-click a note for its menu".to_string(),
            pinned: [true, false, false],
            deleted: [false, false, false],
            menus: (0..3).map(|_| ContextMenuState::default()).collect(),
        }
    }
}

/// The context-menu demo: three notes, each with its own menu.
fn context_demo(context: &mut Context, state: &Rc<RefCell<NotesDemo>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let mut state = state.borrow_mut();
    let notes = [("Groceries", "Personal"), ("Meeting notes", "Work"), ("Passwords", "Personal")];
    let pinned = state.pinned;
    let deleted = state.deleted;
    let mut menus = std::mem::take(&mut state.menus);
    let mut action: Option<(usize, usize)> = None;
    // MyGo's border takes layout room (`ui/layout.go` padX = pad + border),
    // so the 4-pixel padding insets the rows by five.
    column(context).width(320.0).padding(5.0)
        .background(Color::TRANSPARENT, 8.0)
        .border(theme.border, 8.0)
        .children(|context| {
            for (index, (name, folder)) in notes.iter().enumerate() {
                if deleted[index] { continue; }
                let key = format!("docs-demo-note-{index}");
                let hovered = context.hover(&key);
                // MyGo's Row centers its children vertically
                // (`website/docs-demo/actions.go` builds each note as
                // `Row().Height(32).PaddingX(10).Gap(8).Radius(6)`).
                row(context)
                    .height(32.0)
                    .padding_x(10.0)
                    .gap(8.0)
                    .align(AlignItems::Center)
                    .background(if hovered { theme.surface_hover } else { Color::TRANSPARENT }, 6.0)
                    .interactive(&key)
                    .children(|context| {
                        // MyGo's `Icon` takes no size here, so it is one em
                        // of the 14-pixel text around it
                        // (`intrinsicSize` in `ui/widgets.go`).
                        icon(context, &format!("{key}-file"), "file", 14.0, theme.foreground);
                        text(context, name);
                        container(context).flex_grow(1.0);
                        if pinned[index] {
                            icon(context, &format!("{key}-pin"), "pin", 14.0, theme.accent);
                        }
                        if *name == "Passwords" {
                            icon(context, &format!("{key}-lock"), "lock", 14.0, theme.muted);
                        }
                        text(context, folder).font_size(12.0).color(theme.muted);
                    });
                let response = context.node_context_menu(
                    &key,
                    &mut menus[index],
                    &["Rename", "Pinned", "Move to", "Delete"],
                );
                if let Some(item) = response.selected {
                    action = Some((index, item));
                }
            }
        });
    state.menus = menus;
    if let Some((index, item)) = action {
        let name = notes[index].0;
        let folder = notes[index].1;
        probe.set(10 + item as i32);
        match item {
            0 => state.last = format!("Chose Rename for {name}"),
            1 => {
                state.pinned[index] = !state.pinned[index];
                state.last = if state.pinned[index] {
                    format!("Pinned {name}")
                } else {
                    format!("Unpinned {name}")
                };
            }
            2 => state.last = format!("Moved {name} to {folder}"),
            _ => {
                state.deleted[index] = true;
                state.last = format!("Deleted {name}");
            }
        }
    }
    text(context, &state.last).font_size(12.0).color(theme.muted);
}

fn toggle_demo(context: &mut Context, state: &Rc<RefCell<[bool; 4]>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let mut state = state.borrow_mut();
    row(context).gap(12.0).children(|context| {
        let inspector = segment(
            context,
            "docs-demo-toggle-inspector",
            "Inspector",
            None,
            state[0],
            // MyGo's button padding plus its one-pixel border, which takes
            // layout room there and only paints here.
            Edges::symmetric(7.0, 15.0),
            6.0,
            SegmentFace::Plain,
            false,
        );
        if inspector.clicked {
            state[0] = !state[0];
            probe.set(4);
        }
        row(context)
            .padding(3.0)
            .gap(2.0)
            .background(theme.surface, 6.0)
            .border(theme.border, 6.0)
            .children(|context| {
                for (index, icon_name) in ["bold", "italic", "underline"].iter().enumerate() {
                    let toggle = segment(
                        context,
                        &format!("docs-demo-toggle-{icon_name}"),
                        "",
                        Some(icon_name),
                        state[index + 1],
                        Edges::symmetric(4.0, 12.0),
                        4.0,
                        SegmentFace::Segment,
                        false,
                    );
                    if toggle.clicked {
                        state[index + 1] = !state[index + 1];
                        probe.set(index as i32 + 1);
                    }
                }
            });
    });
    let mut fox = text(context, "The quick brown fox").font_size(18.0);
    if state[1] { fox = fox.bold(); }
    if state[3] { fox = fox.underline(); }
    let _ = fox;
    text(context, if state[0] { "Inspector shown" } else { "Inspector hidden" })
        .font_size(12.0)
        .color(theme.muted);
}

fn segmented_demo(context: &mut Context, selected: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let mut view = selected.get().clamp(0, 1) as usize;
    row(context).gap(12.0).children(|context| {
        row(context)
            .padding(3.0)
            .gap(2.0)
            .background(theme.surface, 6.0)
            .border(theme.border, 6.0)
            .children(|context| {
                for (index, label) in ["List", "Grid"].iter().enumerate() {
                    if segment(
                        context,
                        &format!("docs-demo-segmented-{index}"),
                        label,
                        None,
                        view == index,
                        Edges::symmetric(4.0, 12.0),
                        4.0,
                        SegmentFace::Segment,
                        false,
                    )
                    .clicked
                    {
                        view = index;
                    }
                }
            });
        row(context)
            .padding(3.0)
            .background(theme.surface, 8.0)
            .children(|context| {
                for (index, icon_name) in ["list", "grid"].iter().enumerate() {
                    if segment(
                        context,
                        &format!("docs-demo-segmented-icon-{index}"),
                        "",
                        Some(icon_name),
                        view == index,
                        Edges::symmetric(4.0, 10.0),
                        6.0,
                        SegmentFace::Segment,
                        false,
                    )
                    .clicked
                    {
                        view = index;
                    }
                }
            });
    });
    if view != selected.get() as usize {
        selected.set(view as i32);
        probe.set(view as i32 + 1);
    }
    if view == 0 {
        column(context).width(260.0).height(116.0).gap(2.0).children(|context| {
            for name in ["Notes", "Photos", "Music", "Drafts"] {
                row(context)
                    .height(26.0)
                    .padding_x(8.0)
                    .gap(8.0)
                    .background(theme.surface, 6.0)
                    .children(|context| {
                        icon(context, &format!("docs-demo-list-file-{name}"), "folder", 14.0, theme.foreground);
                        text(context, name).line_height(26.0);
                    });
            }
        });
    } else {
        row(context).width(260.0).height(116.0).gap(8.0).align(AlignItems::FlexStart).children(|context| {
            for name in ["Notes", "Photos", "Music", "Drafts"] {
                column(context)
                    .size(59.0, 72.0)
                    .center()
                    .gap(6.0)
                    .background(theme.surface, 8.0)
                    .children(|context| {
                        icon(context, &format!("docs-demo-grid-file-{name}"), "folder", 14.0, theme.foreground);
                        text(context, name).font_size(12.0);
                    });
            }
        });
    }
}

fn toolbar_demo(context: &mut Context, style: &Rc<RefCell<[bool; 2]>>, shared: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let mut style = style.borrow_mut();
    // MyGo's border takes a pixel of the box's layout (`padX` in
    // `ui/layout.go`), so the padding carries it.
    column(context).width(460.0)
        .padding(1.0)
        .background(Color::TRANSPARENT, 8.0)
        .border(theme.border, 8.0)
        .children(|context| {
            row(context).gap(4.0).padding(4.0).children(|context| {
                // The router sits on its first page: both arrows are disabled.
                // MyGo's history buttons pad Space(1.5) = 6 down and Space(2) = 8
                // across in a toolbar (`historyButton` in `ui/router.go`).
                segment(context, "docs-demo-toolbar-back", "", Some("chevron_left"), false, Edges::symmetric(6.0, 8.0), 6.0, SegmentFace::Toolbar, true);
                segment(context, "docs-demo-toolbar-forward", "", Some("chevron_right"), false, Edges::symmetric(6.0, 8.0), 6.0, SegmentFace::Toolbar, true);
                // The track pads Space(0.5) = 2 and borders 1, which takes
                // a pixel of the layout too.
                row(context)
                    .padding(3.0)
                    .gap(2.0)
                    .background(theme.surface, 6.0)
                    .border(theme.border, 6.0)
                    .children(|context| {
                        for (index, label) in ["Bold", "Italic"].iter().enumerate() {
                            // A segment of a ToggleGroup is padded
                            // Space(1) = 4 down and Space(3) = 12 across,
                            // radius Radius - Space(0.5) = 4
                            // (`segmentButtons` in `ui/widgets.go`).
                            let toggle = segment(
                                context,
                                &format!("docs-demo-toolbar-{label}"),
                                label,
                                None,
                                style[index],
                                Edges::symmetric(4.0, 12.0),
                                4.0,
                                SegmentFace::Segment,
                                false,
                            );
                            if toggle.clicked {
                                style[index] = !style[index];
                                probe.set(index as i32 + 1);
                            }
                        }
                    });
                container(context).flex_grow(1.0);
                if segment(context, "docs-demo-toolbar-share", "Share", None, false, Edges::symmetric(6.0, 10.0), 6.0, SegmentFace::Toolbar, false).clicked {
                    shared.set(shared.get() + 1);
                    probe.set(3);
                }
            });
            container(context).height(1.0).background(theme.border, 0.0);
            column(context)
                .height(72.0)
                .padding_x(16.0)
                .padding_y(12.0)
                .gap(8.0)
                .children(|context| {
                    let mut chapter = text(context, "Chapter 1: the fox jumps over the lazy dog.");
                    if style[0] { chapter = chapter.bold(); }
                    let _ = chapter;
                    if link(context, "docs-demo-toolbar-next", "Next chapter").clicked {
                        probe.set(99);
                    }
                });
        });
    if shared.get() > 0 {
        text(context, &format!("Shared {} times", shared.get())).font_size(12.0).color(theme.muted);
    } else {
        text(context, "Next chapter, then Back").font_size(12.0).color(theme.muted);
    }
}


fn checkbox_demo(
    context: &mut Context,
    values: &Rc<RefCell<[bool; 5]>>,
    reloads: &Rc<Cell<i32>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    context.set_widgets_disabled(DEMO_DISABLED.load(Ordering::Relaxed));
    column(context).gap(8.0).children(|context| {
        let mut v = values.borrow_mut();
        for (index, label) in ["Sync across devices", "Show hidden files"].iter().enumerate() {
            let id = format!("docs-demo-checkbox-{index}");
            let mut checked = v[index];
            if checkbox_labeled(context, &id, label, &mut checked).changed {
                v[index] = checked;
                if index == 1 {
                    reloads.set(reloads.get() + 1);
                }
                probe.set(index as i32 + 1);
            }
        }
        // MyGo's CheckboxGroup: a tri-state box over boxes indented below it.
        let all = v[2] && v[3] && v[4];
        let some = v[2] || v[3] || v[4];
        let mut head = all;
        if checkbox_indeterminate(
            context,
            "docs-demo-checkbox-notifications",
            "Notifications",
            &mut head,
            some && !all,
        )
        .changed
        {
            let next = !all;
            v[2] = next;
            v[3] = next;
            v[4] = next;
            probe.set(10);
        }
        column(context).gap(8.0).children(|context| {
            for (index, label) in ["Mail", "Calendar", "Messages"].iter().enumerate() {
                let id = format!("docs-demo-checkbox-group-{index}");
                let mut checked = v[index + 2];
                row(context).gap(0.0).children(|context| {
                    container(context).size(24.0, 1.0);
                    container(context).children(|context| {
                        if checkbox_labeled(context, &id, label, &mut checked).changed {
                            v[index + 2] = checked;
                            probe.set(index as i32 + 11);
                        }
                    });
                });
            }
        });
    });
    text(context, &format!("Reloaded {} times", reloads.get()))
        .font_size(12.0)
        .color(theme.muted);
}

fn switch_demo(context: &mut Context, wifi: &Rc<Cell<bool>>, bluetooth: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    context.set_widgets_disabled(DEMO_DISABLED.load(Ordering::Relaxed));
    column(context).width(240.0).gap(10.0).children(|context| {
        row(context).gap(12.0).children(|context| {
            container(context).flex_grow(1.0).children(|context| { text(context, "Wi-Fi"); });
            let mut on = wifi.get();
            if switch(context, "docs-demo-switch", &mut on).changed { wifi.set(on); probe.set(if on {1} else {0}); }
        });
        row(context).gap(12.0).children(|context| {
            container(context).flex_grow(1.0).children(|context| { text(context, "Bluetooth"); });
            let mut on = bluetooth.get();
            if switch(context, "docs-demo-switch-bluetooth", &mut on).changed { bluetooth.set(on); probe.set(if on {3} else {2}); }
        });
    });
    text(context, &format!("Wi-Fi {}, Bluetooth {}", if wifi.get() {"on"} else {"off"}, if bluetooth.get() {"on"} else {"off"}))
        .font_size(12.0).color(Color::rgb(113, 113, 122));
}

fn radio_demo(
    context: &mut Context,
    selected: &Rc<Cell<i32>>,
    align: &Rc<Cell<i32>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    let mut size = selected.get().clamp(0, 2) as usize;
    let mut alignment = align.get().clamp(0, 2) as usize;
    row(context).gap(48.0).align(AlignItems::FlexStart).children(|context| {
        column(context).gap(8.0).children(|context| {
            text(context, "Size").font_size(12.0).color(theme.muted);
            for (index, label) in ["Small", "Medium", "Large"].iter().enumerate() {
                if radio(context, &format!("docs-demo-radio-size-{index}"), label, &mut size, index).clicked {
                    selected.set(index as i32);
                    probe.set(index as i32 + 1);
                }
            }
        });
        column(context).gap(8.0).children(|context| {
            text(context, "Alignment").font_size(12.0).color(theme.muted);
            row(context).gap(16.0).children(|context| {
                for (index, label) in ["Left", "Center", "Right"].iter().enumerate() {
                    if radio(context, &format!("docs-demo-radio-align-{index}"), label, &mut alignment, index).clicked {
                        align.set(index as i32);
                        probe.set(index as i32 + 11);
                    }
                }
            });
        });
    });
    text(
        context,
        &format!(
            "{}, aligned {}",
            ["Small", "Medium", "Large"][selected.get().clamp(0, 2) as usize],
            ["left", "center", "right"][align.get().clamp(0, 2) as usize]
        ),
    )
    .font_size(12.0)
    .color(theme.muted);
}

fn select_demo(context: &mut Context, selected: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let mut value = selected.get().clamp(0, 2) as usize;
    row(context).gap(10.0).children(|context| {
        text(context, "Plan");
        let response = select(context, "docs-demo-select", &mut value, &["Free", "Pro", "Team"]);
        if response.changed {
            selected.set(value as i32);
            probe.set(value as i32 + 1);
        }
    });
    let names = ["Free", "Pro", "Team"];
    let prices = ["$0", "$12", "$30"];
    let index = selected.get().clamp(0, 2) as usize;
    text(context, &format!("{}: {} a month", names[index], prices[index]))
        .font_size(12.0)
        .color(theme.muted);
}

fn slider_demo(context: &mut Context, volume: &Rc<Cell<f32>>, quality: &Rc<Cell<f32>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    container(context).width(280.0).children(|context| {
        let mut current = volume.get();
        let state = slider(context, "docs-demo-slider", &mut current, 0.0, 100.0);
        if state.changed { volume.set(current); probe.set(current as i32); }
    });
    text(context, &format!("Volume: {:.0}", volume.get())).font_size(12.0).color(theme.muted);
    container(context).width(280.0).children(|context| {
        let mut current = quality.get();
        let state = slider_stepped(context, "docs-demo-slider-quality", &mut current, 0.0, 100.0, 25.0);
        if state.changed { quality.set(current); probe.set(current as i32); }
    });
    text(context, &format!("Quality: {:.0}%", quality.get())).font_size(12.0).color(theme.muted);
}

fn range_demo(context: &mut Context, state: &Rc<RefCell<RangeSliderState>>, probe: &Rc<Cell<i32>>) {
    let mut range = state.borrow_mut();
    container(context).width(280.0).children(|context| {
        let response = range_slider(context, "docs-demo-range", &mut range, 0.0, 1000.0);
        if response.changed {
            probe.set(range.low.round() as i32 + range.high.round() as i32);
        }
    });
    // MyGo's demo draws this line with plain `ui.Textf`: body size, text color.
    text(context, &format!("${:.0} – ${:.0}", range.low, range.high));
}

fn stepper_demo(context: &mut Context, value: &Rc<Cell<i64>>, probe: &Rc<Cell<i32>>) {
    let mut current = value.get();
    row(context).gap(6.0).children(|context| {
        let word = if current == 1 { "copy" } else { "copies" };
        text(context, &format!("{} {}", current, word));
        let response = stepper(context, "docs-demo-stepper", &mut current, 1, 99, 1);
        if response.changed {
            value.set(current);
            probe.set(current as i32);
        }
    });
}

fn rating_demo(context: &mut Context, value: &Rc<Cell<u32>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let mut current = value.get();
    let response = rating(context, "docs-demo-rating", &mut current, 5);
    if response.changed {
        value.set(current);
        probe.set(current as i32);
    }
    if current == 0 {
        text(context, "Not rated: click a star").font_size(12.0).color(theme.muted);
    } else {
        text(context, &format!("Rated {} of 5", current)).font_size(12.0).color(theme.muted);
    }
}


fn text_input_demo(
    context: &mut Context,
    value: &Rc<RefCell<String>>,
    password: &Rc<RefCell<String>>,
    notes: &Rc<RefCell<String>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    stack(context, 320.0, 12.0, |context| {
        let mut name = value.borrow_mut();
        if text_field_placeholder(context, "docs-demo-text-name", &mut name, "Name").changed {
            probe.set(name.len() as i32);
        }
        let mut secret = password.borrow_mut();
        if text_field_password_placeholder(
            context,
            "docs-demo-text-password",
            &mut secret,
            "Password",
        )
        .changed
        {
            probe.set(secret.len() as i32);
        }
        let mut note = notes.borrow_mut();
        if text_area_placeholder(
            context,
            "docs-demo-text-notes",
            &mut note,
            2,
            "Notes, on as many lines as you like",
        )
        .changed
        {
            probe.set(note.lines().count() as i32);
        }
        text(
            context,
            &format!(
                "Enter in the name submits it. Notes: {} characters",
                note.chars().count()
            ),
        )
        .font_size(12.0)
        .color(theme.muted);
    });
}

fn primitives_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let mut document = value.borrow_mut();
    stack(context, 400.0, 12.0, |context| {
        // MyGo's demo (`website/docs-demo/textinput.go` text-input-client):
        // a monospace text area over a document buffer, and a status line
        // reporting its lines, characters and version.
        if text_area_mono(context, "docs-demo-buffer", &mut document, 3, 120.0).changed {
            probe.set(document.lines().count() as i32);
        }
        text(
            context,
            &format!(
                "{} lines, {} characters, version 0",
                document.split('\n').count(),
                document.chars().count()
            ),
        )
        .font_size(12.0)
        .color(theme.muted);
    });
}

/// One row of a form, as MyGo's Field inside a Form (`ui/form.go`
/// coreField): the label right-aligned in a column as wide as the widest
/// label, then a Space(3) gap, then the control, baseline-aligned.
fn field_row<F: FnOnce(&mut Context)>(
    context: &mut Context,
    label: &str,
    label_width: f32,
    content: F,
) {
    row(context).gap(12.0).align(AlignItems::FlexStart).children(|context| {
        column(context)
            .width(label_width)
            .align(AlignItems::FlexEnd)
            .padding_y(6.0)
            .children(|context| {
                text(context, label);
            });
        content(context);
    });
}

fn number_demo(
    context: &mut Context,
    copies: &Rc<RefCell<String>>,
    opacity: &Rc<RefCell<String>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    let mut copies_text = copies.borrow_mut();
    let mut opacity_text = opacity.borrow_mut();
    // MyGo's demo (`website/docs-demo/textinput.go` number-input): a form
    // of two fields whose labels line up in one column.
    let label_width = ["Copies", "Opacity"]
        .iter()
        .map(|label| myrust_ui::host_text_width(label, 14.0).unwrap_or(0.0))
        .fold(0.0_f32, f32::max);
    context.set_form_label_width(label_width);
    // MyGo's NumberInput row is `Width(140)` (`website/docs-demo/textinput.go`),
    // and its input `Shrink(1)` gives up what the two buttons need beyond
    // that. Both buttons are `Padding(Space(1), Space(2))` around "−"/"+".
    let number_button = myrust_ui::host_text_width("−", 14.0).unwrap_or(8.67) + 16.0 + 2.0;
    let number_input_width = (140.0 - 8.0 - number_button * 2.0).max(0.0);
    stack(context, label_width + 12.0 + 140.0, 12.0, |context| {
        form_group(context, "docs-demo-number-form", |context| {
        form_row(context, "docs-demo-number-copies", "Copies", None, None, |context| {
            // MyGo's NumberInput is its input and a − and a + button
            // (`ui/number.go` coreNumberInput).
            row(context).gap(4.0).align(AlignItems::Center).children(|context| {
                container(context).width(number_input_width).children(|context| {
                    let state = number_input(
                        context,
                        "docs-demo-number",
                        &mut copies_text,
                        NumberInputOptions::default().step(1.0),
                    );
                    if state.changed {
                        probe.set(state.value.unwrap_or(0.0).round() as i32);
                    }
                });
                if button_padded(context, "docs-demo-number-minus", "−", 8.0, 4.0, false).clicked {
                    let value = copies_text.trim().parse::<i64>().unwrap_or(1);
                    *copies_text = (value - 1).clamp(1, 99).to_string();
                }
                if button_padded(context, "docs-demo-number-plus", "+", 8.0, 4.0, false).clicked {
                    let value = copies_text.trim().parse::<i64>().unwrap_or(1);
                    *copies_text = (value + 1).clamp(1, 99).to_string();
                }
            });
        });
        form_row(context, "docs-demo-number-opacity-field", "Opacity", None, None, |context| {
            row(context).gap(4.0).align(AlignItems::Center).children(|context| {
                container(context).width(number_input_width).children(|context| {
                    let state = number_input(
                        context,
                        "docs-demo-number-opacity",
                        &mut opacity_text,
                        NumberInputOptions::default().decimals(2).step(0.05),
                    );
                    if state.changed {
                        probe.set((state.value.unwrap_or(0.0) * 100.0).round() as i32);
                    }
                });
                if button_padded(context, "docs-demo-opacity-minus", "−", 8.0, 4.0, false).clicked {
                    let value = opacity_text.trim().parse::<f64>().unwrap_or(0.0);
                    *opacity_text = format!("{:.2}", (value - 0.05).clamp(0.0, 1.0));
                }
                if button_padded(context, "docs-demo-opacity-plus", "+", 8.0, 4.0, false).clicked {
                    let value = opacity_text.trim().parse::<f64>().unwrap_or(0.0);
                    *opacity_text = format!("{:.2}", (value + 0.05).clamp(0.0, 1.0));
                }
            });
        });
        });
        let copies_now = copies_text.trim().parse::<i64>().unwrap_or(0);
        let opacity_now = (opacity_text.trim().parse::<f64>().unwrap_or(0.0) * 100.0).round() as i64;
        text(
            context,
            &format!("{} copies at {}% opacity", copies_now, opacity_now),
        )
        .font_size(12.0)
        .color(theme.muted);
    });
}

fn decimal_demo(
    context: &mut Context,
    price: &Rc<RefCell<String>>,
    discount: &Rc<RefCell<String>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    let mut price_text = price.borrow_mut();
    let mut discount_text = discount.borrow_mut();
    // MyGo's demo (`website/docs-demo/textinput.go` decimal-input): a form
    // of two fields whose labels line up in one column.
    let label_width = ["Price", "Discount"]
        .iter()
        .map(|label| myrust_ui::host_text_width(label, 14.0).unwrap_or(0.0))
        .fold(0.0_f32, f32::max);
    context.set_form_label_width(label_width);
    // MyGo's DecimalInput row is `Width(180)` (`website/docs-demo/textinput.go`),
    // wide enough that its input keeps the `Space(20)` = 80 it asks for.
    let decimal_input_width = 80.0;
    stack(context, label_width + 12.0 + 180.0, 12.0, |context| {
        form_group(context, "docs-demo-decimal-form", |context| {
        form_row(context, "docs-demo-decimal-price", "Price", None, None, |context| {
            // MyGo's DecimalInput is its input and a − and a + button
            // (`ui/decimal.go` coreDecimalInput).
            row(context).gap(4.0).align(AlignItems::Center).children(|context| {
                container(context).width(decimal_input_width).children(|context| {
                    let state = decimal_input(
                        context,
                        "docs-demo-decimal",
                        &mut price_text,
                        DecimalInputOptions {
                            decimals: 2,
                            step_units: 1,
                            min_units: Some(0),
                            ..Default::default()
                        },
                    );
                    if state.changed {
                        probe.set(state.units.unwrap_or(0) as i32);
                    }
                });
                let price = price_text.trim().parse::<f64>().unwrap_or(0.0);
                if button_padded(context, "docs-demo-decimal-minus", "−", 8.0, 4.0, false).clicked {
                    *price_text = format!("{:.2}", (price - 0.01).max(0.0));
                }
                if button_padded(context, "docs-demo-decimal-plus", "+", 8.0, 4.0, false).clicked {
                    *price_text = format!("{:.2}", price + 0.01);
                }
            });
        });
        form_row(context, "docs-demo-decimal-discount-field", "Discount", None, None, |context| {
            row(context).gap(4.0).align(AlignItems::Center).children(|context| {
                container(context).width(decimal_input_width).children(|context| {
                    let state = decimal_input(
                        context,
                        "docs-demo-decimal-discount",
                        &mut discount_text,
                        DecimalInputOptions {
                            decimals: 2,
                            step_units: 25,
                            min_units: Some(0),
                            max_units: Some(10000),
                            ..Default::default()
                        },
                    );
                    if state.changed {
                        probe.set(state.units.unwrap_or(0) as i32);
                    }
                });
                // MyGo disables a button at the end of the range it would
                // step past (`ui/decimal.go` coreDecimalInput): the demo's
                // discount starts at 0.00, its minimum.
                let discount = discount_text.trim().parse::<f64>().unwrap_or(0.0);
                if button_padded_disabled(context, "docs-demo-discount-minus", "−", 8.0, 4.0, false, discount <= 0.0).clicked {
                    *discount_text = format!("{:.2}", (discount - 0.25).clamp(0.0, 100.0));
                }
                if button_padded(context, "docs-demo-discount-plus", "+", 8.0, 4.0, false).clicked {
                    *discount_text = format!("{:.2}", (discount + 0.25).clamp(0.0, 100.0));
                }
            });
        });
        });
        text(
            context,
            &format!("Price {}, {}% off", price_text, discount_text),
        )
        .font_size(12.0)
        .color(theme.muted);
    });
}

fn search_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let fruits = [
        "Apple", "Apricot", "Banana", "Blueberry", "Cherry", "Grape", "Lemon", "Mango", "Orange",
        "Peach", "Pear", "Plum",
    ];
    let mut query = value.borrow_mut();
    stack(context, 320.0, 12.0, |context| {
        let state = search_field(context, "docs-demo-search", &mut query);
        if state.changed {
            probe.set(query.len() as i32);
        }
        let needle = query.to_lowercase();
        let found: Vec<&str> = fruits
            .iter()
            .filter(|fruit| fruit.to_lowercase().contains(&needle))
            .copied()
            .collect();
        if !found.is_empty() {
            row(context)
                .width(320.0)
                .gap(6.0)
                .wrap()
                .justify(JustifyContent::Center)
                .children(|context| {
                    for fruit in &found {
                        badge(context, fruit);
                    }
                });
        }
        text(
            context,
            &format!("{} of {} fruits", found.len(), fruits.len()),
        )
        .font_size(12.0)
        .color(theme.muted);
    });
}

fn combobox_demo(
    context: &mut Context,
    query: &Rc<RefCell<String>>,
    state: &Rc<RefCell<ComboboxState>>,
    probe: &Rc<Cell<i32>>,
) {
    let mut query = query.borrow_mut();
    let mut state = state.borrow_mut();
    container(context).width(240.0).children(|context| {
        let response = combobox(
            context,
            "docs-demo-combo",
            &mut query,
            &mut state,
            &[
                "Arial",
                "Courier New",
                "Georgia",
                "Helvetica",
                "Times New Roman",
                "Trebuchet MS",
                "Verdana",
            ],
        );
        if response.open {
            probe.set(10);
        }
        if let Some(index) = response.selected {
            probe.set(index as i32 + 1);
        }
    });
    text(context, "The quick brown fox")
        .font(query.clone())
        .font_size(20.0);
    spacer(context); // room below for the options
}

fn autocomplete_demo(
    context: &mut Context,
    value: &Rc<RefCell<String>>,
    state: &Rc<RefCell<AutocompleteState>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    let mut value = value.borrow_mut();
    let mut state = state.borrow_mut();
    container(context).width(240.0).children(|context| {
        let response = autocomplete(
            context,
            "docs-demo-auto",
            &mut value,
            &mut state,
            &[
                "Amsterdam", "Berlin", "Lisbon", "London", "Madrid", "Paris", "Prague", "Rome",
                "Vienna",
            ],
        );
        if response.visible {
            probe.set(10);
        }
        if let Some(index) = response.selected {
            probe.set(index as i32 + 1);
        }
    });
    let trimmed = value.trim().to_string();
    let line = if trimmed.is_empty() {
        "Type a city, or any place".to_string()
    } else {
        format!("Shipping to {}", trimmed)
    };
    text(context, &line).font_size(12.0).color(theme.muted);
    spacer(context); // room below for the suggestions
}

fn token_demo(
    context: &mut Context,
    tokens: &Rc<RefCell<Vec<String>>>,
    input: &Rc<RefCell<String>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    let mut tokens = tokens.borrow_mut();
    let mut input = input.borrow_mut();
    container(context).width(360.0).children(|context| {
        let state = token_field(context, "docs-demo-token", &mut tokens, &mut input);
        if state.added || state.removed {
            probe.set(tokens.len() as i32);
        }
    });
    let line = if tokens.is_empty() {
        "No tags: type one and press Enter".to_string()
    } else {
        format!("{} tags: {}", tokens.len(), tokens.join(", "))
    };
    text(context, &line).font_size(12.0).color(theme.muted);
    spacer(context); // room below for the suggestions
}

fn editable_demo(
    context: &mut Context,
    value: &Rc<RefCell<String>>,
    state: &Rc<RefCell<EditableTextState>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    row(context).gap(8.0).align(AlignItems::Center).children(|context| {
        // MyGo's Icon is as high as the font size, 14 DIPs on wasm.
        icon(context, "docs-demo-edit-icon", "file", 14.0, theme.foreground);
        let mut value = value.borrow_mut();
        let mut state = state.borrow_mut();
        let response = editable_text(context, "docs-demo-edit", &mut state, &mut value);
        if response.editing || response.submitted {
            probe.set(1);
        }
    });
    text(context, "Double-click the name to rename it")
        .font_size(12.0)
        .color(theme.muted);
}

/// The text MyGo's find-bar demo finds in (`website/docs-demo/textinput.go`).
const FIND_TEXT: &str = "The quick brown fox jumps over the lazy dog. The dog sleeps in the sun; the fox runs on into the woods.";

fn find_demo(
    context: &mut Context,
    finding: &Rc<Cell<bool>>,
    query: &Rc<RefCell<String>>,
    current: &Rc<Cell<usize>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    let mut query_ref = query.borrow_mut();
    let mut index = current.get();
    // The app finds: the matches of the query, by their byte offsets.
    let lower = FIND_TEXT.to_lowercase();
    let needle = query_ref.to_lowercase();
    let mut matches: Vec<usize> = Vec::new();
    if !needle.is_empty() {
        let mut at = 0;
        while let Some(offset) = lower[at..].find(&needle) {
            matches.push(at + offset);
            at += offset + needle.len();
        }
    }
    let open = finding.get();
    column(context)
        .width(480.0)
        .padding(1.0)
        .background(theme.background, 8.0)
        .border(theme.border, 8.0)
        .children(|context| {
            if open {
                let state = find_bar(
                    context,
                    "docs-demo-find",
                    &mut query_ref,
                    &mut index,
                    matches.len(),
                );
                if state.closed {
                    finding.set(false);
                }
                if state.changed {
                    probe.set(index as i32 + 1);
                }
            }
            column(context).padding(12.0).gap(10.0).children(|context| {
                // The text, its matches highlighted, the current one in the
                // accent color, as MyGo's demo paints them.
                let mut spans: Vec<RichTextSpan> = Vec::new();
                let mut at = 0;
                for (i, start) in matches.iter().enumerate() {
                    if *start > at {
                        spans.push(RichTextSpan::new(&FIND_TEXT[at..*start]));
                    }
                    let end = *start + needle.len();
                    let mut span = RichTextSpan::new(&FIND_TEXT[*start..end]);
                    if open && i == index {
                        span = span.background(theme.accent).color(theme.on_accent);
                    } else if open {
                        span = span.background(Color::rgba(37, 99, 235, 64));
                    }
                    spans.push(span);
                    at = end;
                }
                if at < FIND_TEXT.len() {
                    spans.push(RichTextSpan::new(&FIND_TEXT[at..]));
                }
                rich_text_spans(context, "docs-demo-find-text", &spans);
                if !open && button(context, "Find…").clicked {
                    finding.set(true);
                }
            });
        });
    current.set(index);
}

/// The formatting state of the rich-text editor demo.
#[derive(Debug, Clone)]
struct RichTextEditorDemoState {
    bold: bool,
    italic: bool,
    underline: bool,
    heading: bool,
    bullets: bool,
    tasks: bool,
}

impl Default for RichTextEditorDemoState {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            heading: false,
            bullets: false,
            // MyGo's document starts on its task list, so that button
            // shows selected on the first frame.
            tasks: true,
        }
    }
}

fn rich_text_editor_demo(
    context: &mut Context,
    state: &Rc<RefCell<RichTextEditorDemoState>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    let mut st = state.borrow_mut();
    let mut toggled = false;
    // MyGo's demo puts the toolbar, the editor and the status directly in
    // the preview column, which spaces them 12 apart and centers the
    // narrower status.
    ({
            row(context)
                .width(712.0)
                .gap(4.0)
                .children(|context| {
                    let buttons: [(&str, &str); 6] = [
                        ("docs-demo-rt-bold", "B"),
                        ("docs-demo-rt-italic", "I"),
                        ("docs-demo-rt-underline", "U"),
                        ("docs-demo-rt-heading", "H"),
                        ("docs-demo-rt-bullets", "•"),
                        ("docs-demo-rt-tasks", "☐"),
                    ];
                    for (index, (id, label)) in buttons.iter().enumerate() {
                        let active = match index {
                            0 => st.bold,
                            1 => st.italic,
                            2 => st.underline,
                            3 => st.heading,
                            4 => st.bullets,
                            _ => st.tasks,
                        };
                        if context.button_padded(id, label, 9.0, 3.0, active).clicked {
                            match index {
                                0 => st.bold = !st.bold,
                                1 => st.italic = !st.italic,
                                2 => st.underline = !st.underline,
                                3 => st.heading = !st.heading,
                                4 => {
                                    st.bullets = !st.bullets;
                                    st.tasks = false;
                                }
                                _ => {
                                    st.tasks = !st.tasks;
                                    st.bullets = false;
                                }
                            }
                            toggled = true;
                        }
                    }
                });
            container(context)
                .width(712.0)
                .height(190.0)
                .background(theme.surface, 6.0)
                .border(theme.border, 6.0)
                .padding_x(12.0)
                .padding_y(10.0)
                .children(|context| {
                    column(context).width(686.0).children(|context| {
                        text(context, "Trip to Lisbon")
                            .font_size(20.3)
                            .weight(700)
                            .color(theme.foreground);
                        container(context).width(686.0).height(3.5);
                        let mut spans = vec![RichTextSpan::new("Flights land at ")];
                        spans.push(RichTextSpan::new("10:40").weight(700));
                        spans.push(RichTextSpan::new(". The hotel is in "));
                        spans.push(RichTextSpan::new("Alfama").italic());
                        spans.push(RichTextSpan::new("."));
                        rich_text_spans(context, "docs-demo-rt-paragraph", &spans);
                        // MyGo sets no room between two body paragraphs
                        // (`richtext.Theme` ParagraphSpacing is 0), so the
                        // task rows follow the line box directly.
                        task_row(
                            context,
                            "docs-demo-rt-task-1",
                            true,
                            &[RichTextSpan::new("Passport")
                                .color(theme.muted)
                                .strikethrough()],
                            686.0,
                        );
                        task_row(
                            context,
                            "docs-demo-rt-task-2",
                            false,
                            &[
                                RichTextSpan::new("Adapter for "),
                                RichTextSpan::new("type F")
                                    .mono()
                                    .font_size(12.88)
                                    .background(Color::rgba(
                                        theme.foreground.r,
                                        theme.foreground.g,
                                        theme.foreground.b,
                                        18,
                                    ))
                                    .background_radius(3.0),
                            ],
                            686.0,
                        );
                    });
                });
    });
    text(context, "17 words · 4 paragraphs")
        .font_size(12.0)
        .color(theme.muted);
    if toggled {
        probe.set(1);
    }
}

/// Draws one task-list paragraph: its marker and its text runs.
fn task_row(context: &mut Context, id: &str, checked: bool, spans: &[RichTextSpan], width: f32) {
    let theme = context.theme().clone();
    let box_size = 13.3;
    row(context).width(width).children(|context| {
        let mut commands = Vec::new();
        if checked {
            commands.push(DrawCommand::RoundedRect {
                x: 2.87,
                y: 1.85,
                width: box_size,
                height: box_size,
                radius: myrust_ui::Radius::all(box_size * 0.25),
                color: theme.accent,
            });
            commands.push(DrawCommand::TransformPush {
                transform: Transform::translate(myrust_ui::Point::new(2.87, 1.85)),
            });
            commands.push(DrawCommand::SvgPath {
                x: 0.0,
                y: 0.0,
                width: box_size,
                height: box_size,
                path: "M3.32 6.92 L5.72 9.31 L10.11 4.26".to_string(),
                fill: None,
                stroke: Some(Color::rgb(255, 255, 255)),
                stroke_width: 1.6,
            });
            commands.push(DrawCommand::TransformPop);
        } else {
            // MyGo fills the empty box's outline as an inset border 1.25
            // wide, which the painter rounds to whole device pixels
            // (`ui/richtext/paint.go` paintMarker, `ui/paint.go` borders):
            // at scale 2 that is three, so the ring is two filled rects
            // three device pixels apart rather than a stroked path.
            commands.push(DrawCommand::TransformPush {
                transform: Transform::translate(myrust_ui::Point::new(2.87, 1.85)),
            });
            commands.push(DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: box_size,
                height: box_size,
                radius: myrust_ui::Radius::all(box_size * 0.25),
                color: theme.muted,
            });
            commands.push(DrawCommand::RoundedRect {
                x: 1.5,
                y: 1.5,
                width: box_size - 3.0,
                height: box_size - 3.0,
                radius: myrust_ui::Radius::all(box_size * 0.25 - 1.5),
                color: theme.surface,
            });
            commands.push(DrawCommand::TransformPop);
        }
        container(context)
            .width(23.8)
            .height(17.0)
            .draw(commands);
        container(context).width(width - 23.8).children(|context| {
            rich_text_spans(context, id, spans);
        });
    });
}


/// The greeter source MyGo's code-editor page shows.
const DEMO_GREETER: &str = "package greeter\n\n// Greet greets name, or the world.\nfunc Greet(name string) string {\n\t// TODO: trim spaces.\n\tif name == \"\" {\n\t\tname = \"world\"\n\t}\n\treturn \"Hello, \" + name + \"!\"\n}\n";

/// The edited greeter MyGo's diff-viewer page shows.
const DEMO_GREETER_EDITED: &str = "package greeter\n\nimport \"strings\"\n\n// Greet greets name, or everyone.\nfunc Greet(name string) string {\n\tname = strings.TrimSpace(name)\n\tif name == \"\" {\n\t\tname = \"everyone\"\n\t}\n\treturn \"Hello, \" + name + \"!\"\n}\n";

fn code_demo(context: &mut Context, state: &Rc<RefCell<CodeEditorState>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    // MyGo marks each TODO comment as a warning diagnostic and guides the
    // 80th column (`code.go` sets `WrapGuides: []int{80}` and marks the TODOs
    // through `SetDiagnostics`).
    let diagnostics = greeter_diagnostics();
    // MyGo frames the editor in a bordered, clipped box as tall as the page's
    // preview, then reports the cursor and the line count below it.
    container(context)
        .width(712.0)
        .height(230.0)
        .padding(1.0)
        .border(theme.border, 8.0)
        .children(|context| {
            let mut state = state.borrow_mut();
            let response = code_editor_with_options(
                context,
                "docs-demo-code",
                DEMO_GREETER,
                &mut state,
                &CodeEditorOptions {
                    diagnostics: &diagnostics,
                    wrap_guide: Some(80),
                    tab_size: 4,
                },
            );
            if let Some(line) = response.clicked_line {
                probe.set(line as i32 + 1);
            }
        });
    let line = state.borrow().selected_line.map(|line| line + 1).unwrap_or(1);
    // A buffer's lines are its newlines' plus one: the text ends in a
    // newline, so its last line is empty and the gutter numbers 11
    // (`Buffer.LineCount`).
    let lines = DEMO_GREETER.split('\n').count();
    text(
        context,
        &format!("Ln {line}, Col 1 · {lines} lines · 1 TODO"),
    )
    .font_size(12.0)
    .color(theme.muted);
}

/// The warnings MyGo's code page marks: each TODO comment, from the TODO to
/// the end of its line (`todosOf` in `docs-demo/code.go`).
fn greeter_diagnostics() -> Vec<CodeDiagnostic> {
    let mut diagnostics = Vec::new();
    for (index, line) in DEMO_GREETER.split('\n').enumerate() {
        if let Some(at) = line.find("TODO") {
            let start = line[..at].chars().count();
            diagnostics.push(CodeDiagnostic {
                line: index,
                start,
                end: line.chars().count(),
                severity: CodeSeverity::Warning,
            });
        }
    }
    diagnostics
}

fn diff_demo(context: &mut Context, state: &Rc<RefCell<DiffViewerState>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let stats = Rc::new(Cell::new((0_i32, 0_i32)));
    let counted = stats.clone();
    // MyGo's border takes a pixel of the box's layout, so the content starts
    // one pixel in, as its `contentX` does.
    container(context)
        .width(712.0)
        .height(250.0)
        .padding(1.0)
        .border(theme.border, 8.0)
        .children(|context| {
            let mut state = state.borrow_mut();
            let response = diff_viewer(
                context,
                "docs-demo-diff",
                DEMO_GREETER,
                DEMO_GREETER_EDITED,
                &mut state,
            );
            counted.set((response.added as i32, response.removed as i32));
            probe.set(if response.layout == myrust_ui::DiffLayout::SideBySide {
                1
            } else {
                0
            });
        });
    let (added, removed) = stats.get();
    text(
        context,
        &format!(
            "+{added} −{removed} · edit the right side, or revert and keep its changes"
        ),
    )
    .font_size(12.0)
    .color(theme.muted);
}

fn month_name(month: u32) -> &'static str {
    [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ][(month.saturating_sub(1) as usize).min(11)]
}

fn calendar_demo(context: &mut Context, state: &Rc<RefCell<CalendarState>>, probe: &Rc<Cell<i32>>) {
    // MyGo's calendar demo puts the calendar and a status line in the
    // preview's own 12-gap column (`website/docs-demo/dates.go`).
    column(context).gap(12.0).center().children(|context| {
        let mut state = state.borrow_mut();
        let response = calendar(context, "docs-demo-calendar", &mut state);
        if let Some(day) = response.selected {
            probe.set(day as i32);
        }
        let selected = state.selected.unwrap_or(1);
        let theme = context.theme().clone();
        text(
            context,
            &format!(
                "Due {}, {} {}, {}",
                weekday_full(state.year, state.month, selected),
                month_name(state.month),
                selected,
                state.year
            ),
        )
        .font_size(12.0)
        .color(theme.muted);
    });
}

fn date_demo(
    context: &mut Context,
    birthday: &Rc<RefCell<String>>,
    state: &Rc<RefCell<DateInputState>>,
    meeting: &Rc<RefCell<String>>,
    meeting_state: &Rc<RefCell<DateInputState>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    // MyGo's demo (`website/docs-demo/dates.go` date-input): a row of two
    // fields, Birthday and Meeting, the second holding a date and a time
    // input 8 apart, and the chosen dates below.
    column(context).fill_parent().gap(12.0).children(|context| {
        row(context).gap(24.0).children(|context| {
            column(context).gap(6.0).children(|context| {
                text(context, "Birthday").weight(500).color(theme.foreground);
                let mut value = birthday.borrow_mut();
                let mut state = state.borrow_mut();
                let response = date_input(context, "docs-demo-date", &mut value, &mut state);
                if response.open {
                    probe.set(2);
                } else if response.valid {
                    probe.set(1);
                }
            });
            column(context).gap(6.0).children(|context| {
                text(context, "Meeting").weight(500).color(theme.foreground);
                row(context).gap(8.0).children(|context| {
                    let mut value = meeting.borrow_mut();
                    let mut meeting_state = meeting_state.borrow_mut();
                    date_input(context, "docs-demo-meeting", &mut value, &mut meeting_state);
                    let mut time = "14:30".to_string();
                    time_input(context, "docs-demo-meeting-time", &mut time);
                });
            });
        });
        let born = birthday.borrow().clone();
        let meeting = meeting.borrow().clone();
        text(
            context,
            &format!(
                "Born on {}, meeting on {}",
                long_date(&born),
                short_weekday(&meeting)
            ),
        )
        .font_size(12.0)
        .color(theme.muted);
    });
}

/// Formats an ISO date as MyGo's `January 2, 2006`.
fn long_date(iso: &str) -> String {
    let mut parts = iso.split('-');
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else {
        return iso.to_string();
    };
    let (Ok(_year), Ok(month), Ok(day)) = (
        y.parse::<i32>(),
        m.parse::<usize>(),
        d.parse::<u32>(),
    ) else {
        return iso.to_string();
    };
    const MONTHS: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December",
    ];
    match MONTHS.get(month.wrapping_sub(1)) {
        Some(name) => format!("{name} {day}, {y}"),
        None => iso.to_string(),
    }
}

/// Formats an ISO date as MyGo's `Mon, Jan 2 at 15:04`, the time taken from
/// the meeting's own time field.
fn short_weekday(iso: &str) -> String {
    let mut parts = iso.split('-');
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else {
        return iso.to_string();
    };
    let (Ok(year), Ok(month), Ok(day)) = (
        y.parse::<i32>(),
        m.parse::<u32>(),
        d.parse::<u32>(),
    ) else {
        return iso.to_string();
    };
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let weekday = weekday_index(year, month, day);
    let month_name = MONTHS
        .get(month.wrapping_sub(1) as usize)
        .copied()
        .unwrap_or("");
    format!("{}, {month_name} {day} at 14:30", DAYS[weekday])
}

/// Returns the weekday of a Gregorian date, 0 for Sunday, as Sakamoto's
/// algorithm gives it.
fn weekday_index(year: i32, month: u32, day: u32) -> usize {
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut y = year;
    if month < 3 {
        y -= 1;
    }
    let index = (y + y / 4 - y / 100 + y / 400 + OFFSETS[(month - 1) as usize] + day as i32)
        .rem_euclid(7);
    index as usize
}

fn time_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    // MyGo's demo (`website/docs-demo/dates.go` time-input): the bell, the
    // time field beside it, and the alarm's time below.
    row(context).gap(8.0).align(AlignItems::Center).children(|context| {
        // MyGo's `Icon(c, iconBell)` takes the theme's font size, an em.
        icon(
            context,
            "docs-demo-time-bell",
            "bell",
            theme.font_sizes.body,
            theme.muted,
        );
        let mut value = value.borrow_mut();
        let response = time_input(context, "docs-demo-time", &mut value);
        if response.changed {
            probe.set(response.hour as i32 * 60 + response.minute as i32);
        }
    });
    let shown = value.borrow().clone();
    let (hour, minute) = shown
        .split_once(':')
        .and_then(|(h, m)| Some((h.parse::<u32>().ok()?, m.parse::<u32>().ok()?)))
        .unwrap_or((7, 30));
    let period = if hour >= 12 { "PM" } else { "AM" };
    let hour12 = match hour % 12 {
        0 => 12,
        h => h,
    };
    text(
        context,
        &format!("The alarm rings at {hour12}:{minute:02} {period}"),
    )
    .font_size(12.0)
    .color(theme.muted);
}

fn color_demo(
    context: &mut Context,
    background: &Rc<RefCell<Color>>,
    tint: &Rc<RefCell<Color>>,
    state: &Rc<RefCell<ColorPanelState>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    let tint_color = *tint.borrow();
    // MyGo's demo: Row(c).Grow(1).Gap(24).AlignItems(Start), the picker at
    // its own 240 DIPs and the rest in a column beside it
    // (`website/docs-demo/dates.go` color-picker).
    row(context).fill_parent().gap(24.0).align(AlignItems::FlexStart).children(|context| {
        let mut background = background.borrow_mut();
        let mut state = state.borrow_mut();
        let response = color_panel(context, "docs-demo-color", &mut background, &mut state);
        if response.changed {
            probe.set(1);
        }
        column(context).gap(12.0).align(AlignItems::FlexStart).children(|context| {
            // Field("Tint", ColorWell(&tint)): a label, then the well.
            column(context).gap(6.0).align(AlignItems::FlexStart).children(|context| {
                text(context, "Tint").weight(500).color(theme.foreground);
                color_well(context, "docs-demo-color-well", &mut tint.borrow_mut(), &theme);
            });
            let background = *background;
            column(context)
                .width(180.0)
                // MyGo's border takes layout space, so its 16-pixel padding
                // and 1-pixel border inset the text by 17.
                .padding(17.0)
                .gap(4.0)
                .background(background, 10.0)
                .border(theme.border, 10.0)
                .children(|context| {
                    // MyGo's border takes layout space, so the text wraps
                    // in the 146 pixels the card leaves inside its edges.
                    text(context, "Hello")
                        .font_size(20.0)
                        .bold()
                        .color(tint_color)
                        .max_width(146.0);
                    text(context, "Tint on background")
                        .font_size(12.0)
                        .color(tint_color)
                        .max_width(146.0);
                });
            text(
                context,
                &format!("{} on {}", dates_hex(tint_color), dates_hex(background)),
            )
            .font_size(12.0)
            .color(theme.muted);
        });
    });
}

/// Returns `c` as `#rrggbb`, as MyGo's demo prints colors.
fn dates_hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

/// Draws MyGo's `ColorWell`: a Surface box, padded Space(1), edged in
/// Border, holding a 36x20 checkerboarded swatch (`ui/colorpicker.go`
/// coreColorWell).
fn color_well(context: &mut Context, id: &str, color: &mut Color, theme: &Theme) {
    let space = theme.spacing.base;
    let inner_w = space * 9.0;
    let inner_h = space * 5.0;
    let swatch = *color;
    let mut commands = checkerboard(inner_w, inner_h, space * 1.25);
    // The swatch fills its box squarely: MyGo rounds the well's clip, not
    // the color it paints inside (`ui/colorpicker.go` coreColorWell), and
    // a Clip only cuts what a box holds, not the box's own fill.
    commands.push(DrawCommand::Rect {
        x: 0.0,
        y: 0.0,
        width: inner_w,
        height: inner_h,
        color: swatch,
    });
    // MyGo's ColorWell is a button that shrinks to its content: Space(1) of
    // padding around a 36x20 swatch, on a Surface face edged in Border. Its
    // Border(1) takes layout space on each side, so the box is 46 by 30.
    context
        .clickable(id)
        .size(inner_w + space * 2.0 + 2.0, inner_h + space * 2.0 + 2.0)
        .padding(space + 1.0)
        .background(theme.surface, 6.0)
        .border(theme.border, 6.0)
        .children(|context| {
            context.container().size(inner_w, inner_h).clip().draw(commands);
        });
}

/// Returns the gray squares that show transparency, as MyGo's `checkers`
/// paints them: white behind, and 204 gray on every other square.
fn checkerboard(width: f32, height: f32, size: f32) -> Vec<DrawCommand> {
    let mut commands = vec![DrawCommand::Rect {
        x: 0.0,
        y: 0.0,
        width,
        height,
        color: Color::rgb(255, 255, 255),
    }];
    let columns = (width / size).ceil() as usize;
    let rows = (height / size).ceil() as usize;
    for row in 0..rows {
        for column in 0..columns {
            if (row + column) % 2 == 0 {
                let x = column as f32 * size;
                let y = row as f32 * size;
                commands.push(DrawCommand::Rect {
                    x,
                    y,
                    width: size.min(width - x),
                    height: size.min(height - y),
                    color: Color::rgb(204, 204, 204),
                });
            }
        }
    }
    commands
}

/// The files MyGo's list and table demos show, with the sizes a file
/// browser would print.
/// The demos' files, as MyGo's `collectionsFiles` builds them: a name, the
/// kind its extension names, and a size from the demo's own formula so the
/// numbers stay the same at every run.
const DEMO_FILES: [(&str, &str, &str); 28] = [
    ("Annual Report.pdf", "PDF document", "170 KB"),
    ("Budget 2026.xlsx", "Spreadsheet", "22 KB"),
    ("Beach.jpg", "JPEG image", "1.1 MB"),
    ("Cover Letter.docx", "Word document", "959 KB"),
    ("Design Review.key", "Keynote", "810 KB"),
    ("Expenses.csv", "CSV text", "662 KB"),
    ("Family.jpg", "JPEG image", "514 KB"),
    ("Flight Tickets.pdf", "PDF document", "366 KB"),
    ("Garden Plan.png", "PNG image", "218 KB"),
    ("Groceries.txt", "Plain text", "70 KB"),
    ("Holiday Video.mov", "QuickTime movie", "46.2 MB"),
    ("Invoice 1042.pdf", "PDF document", "1.0 MB"),
    ("Lecture Notes.md", "Markdown", "859 KB"),
    ("main.go", "Go source", "711 KB"),
    ("Meeting Notes.txt", "Plain text", "563 KB"),
    ("Mountains.jpg", "JPEG image", "414 KB"),
    ("Podcast Episode.mp3", "MP3 audio", "10.7 MB"),
    ("Quarterly Plan.pages", "Pages document", "118 KB"),
    ("README.md", "Markdown", "1.2 MB"),
    ("Recipes.pdf", "PDF document", "1.1 MB"),
    ("Resume.pdf", "PDF document", "907 KB"),
    ("Road Trip.png", "PNG image", "759 KB"),
    ("Screenshot.png", "PNG image", "611 KB"),
    ("Slides.key", "Keynote", "463 KB"),
    ("Sunset.jpg", "JPEG image", "315 KB"),
    ("Taxes 2025.pdf", "PDF document", "166 KB"),
    ("Team Photo.jpg", "JPEG image", "18 KB"),
    ("Wedding Speech.docx", "Word document", "1.1 MB"),
];

fn list_demo(context: &mut Context, state: &Rc<RefCell<ListState>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let selected = state.borrow().selected_id.clone();
    // MyGo's List is a Scroll: a 712×196 box with a one-pixel border inside
    // its padding of 4, its rows 29 tall (17 line box plus 6 above and
    // below) and cut at the viewport edge (`ui/list.go`, `ui/scroll.go`).
    // The border takes room in the layout, so the rows start one pixel in.
    column(context).width(712.0).gap(12.0).center().children(|context| {
        container(context)
            .size(712.0, 196.0)
            .padding(5.0)
            .border(theme.border, 8.0)
            .clip()
            .children(|context| {
                column(context).width(702.0).children(|context| {
                    for (index, (name, _kind, size)) in DEMO_FILES.iter().enumerate() {
                        let id = format!("docs-demo-list-{index}");
                        let is_selected = selected == *name;
                        let row = context
                            .clickable(&id)
                            .size(702.0, 29.0)
                            .background(
                                if is_selected {
                                    theme.accent
                                } else {
                                    Color::TRANSPARENT
                                },
                                6.0,
                            )
                            .padding_xy(10.0, 6.0)
                            .gap(8.0)
                            .align(AlignItems::Center)
                            .children(|context| {
                                let icon_name = if name.ends_with(".jpg")
                                    || name.ends_with(".png")
                                {
                                    "image"
                                } else if name.ends_with(".go") {
                                    "code"
                                } else {
                                    "file"
                                };
                                icon(
                                    context,
                                    &format!("{id}-icon"),
                                    icon_name,
                                    16.0,
                                    if is_selected {
                                        theme.on_accent
                                    } else {
                                        theme.foreground
                                    },
                                );
                                container(context).flex_grow(1.0).children(|context| {
                                    // MyGo's Selected(on) sets the row's text to
                                    // AccentText with its background.
                                    text(context, name).font_size(14.0).color(if is_selected {
                                        theme.on_accent
                                    } else {
                                        theme.foreground
                                    });
                                });
                                text(context, *size)
                                    .font_size(12.0)
                                    .color(if is_selected {
                                        Color::rgba(255, 255, 255, 191)
                                    } else {
                                        theme.muted
                                    });
                            });
                        if row.clicked {
                            state.borrow_mut().selected_id = name.to_string();
                            probe.set(index as i32 + 1);
                        }
                    }
                });
            });
        row(context).width(712.0).gap(12.0).align(AlignItems::Center).children(|context| {
            text(
                context,
                "28 files, 1 chosen: Shift-click or Cmd-click to choose several",
            )
            .font_size(12.0)
            .color(theme.muted);
            container(context).flex_grow(1.0);
            // MyGo disables Restore once every file is back: its
            // `Disabled(len(files) == len(collectionsFiles()))`, and this
            // demo starts with all 28 files.
            if button_disabled(context, "Restore", true).clicked {
                probe.set(98);
            }
            if button(context, "Delete 1").clicked {
                probe.set(99);
            }
        });
    });
}

/// The columns of the table demo, as MyGo's `ui.TableColumn` list: Name
/// grows from its least, Kind is 140 wide, Size is 90 and ends its text.
const DEMO_TABLE_COLUMNS: [(&str, Option<f32>); 3] =
    [("Name", None), ("Kind", Some(140.0)), ("Size", Some(90.0))];

fn table_demo(context: &mut Context, state: &Rc<RefCell<TableState>>, probe: &Rc<Cell<i32>>) {
    context.set_widgets_disabled(DEMO_DISABLED.load(Ordering::Relaxed));
    let theme = context.theme().clone();
    // MyGo's Table is a Column of a 32-pixel header row, a one-pixel divider
    // and the rows' List, in a 712×236 box whose border takes a pixel and
    // which clips its rows (`ui/table.go`, `website/docs-demo/collections.go`).
    // The demo starts sorted by name, with Budget 2026.xlsx chosen.
    let mut chosen = state.borrow().selected_id.clone();
    let sort = state.borrow().sort_column;
    let descending = state.borrow().sort_direction == SortDirection::Descending;
    let mut order: Vec<usize> = (0..DEMO_FILES.len()).collect();
    order.sort_by(|left, right| {
        let key = |index: &usize| match sort {
            Some(1) => DEMO_FILES[*index].1.to_lowercase(),
            Some(2) => DEMO_FILES[*index].2.to_lowercase(),
            _ => DEMO_FILES[*index].0.to_lowercase(),
        };
        let result = key(left).cmp(&key(right));
        if descending { result.reverse() } else { result }
    });
    let selected_row = order
        .iter()
        .position(|index| DEMO_FILES[*index].0 == chosen);
    // 710 pixels of content: Name takes what Kind and Size leave.
    let content_width = 710.0_f32;
    let widths = [
        content_width - 140.0 - 90.0,
        140.0,
        90.0,
    ];
    column(context).width(712.0).gap(12.0).center().children(|context| {
        // MyGo's border takes a pixel of the layout (`padX` adds it), so the
        // header and rows start one pixel in from the box's edge.
        container(context)
            .size(712.0, 236.0)
            .padding(1.0)
            .border(theme.border, 8.0)
            .clip()
            .children(|context| {
                column(context).width(content_width).children(|context| {
                    // The header row: titles in the muted color at weight 600,
                    // a sort chevron after the sorted one, and the grip that
                    // resizes a column at its right edge.
                    row(context)
                        .width(content_width)
                        .height(32.0)
                        .children(|context| {
                            for (index, (title, _)) in DEMO_TABLE_COLUMNS.iter().enumerate() {
                                let id = format!("docs-demo-table-head-{index}");
                                let mut head = context
                                    .clickable(&id)
                                    .size(widths[index], 32.0)
                                    .padding_xy(10.0, 6.0)
                                    .align(AlignItems::Center);
                                if index == 2 {
                                    head = head.justify(JustifyContent::FlexEnd);
                                }
                                let cell = head.children(|context| {
                                    text(context, title)
                                        .font_size(14.0)
                                        .weight(600)
                                        .color(theme.muted);
                                    if sort == Some(index) {
                                        // MyGo's chevron: a 10-pixel box
                                        // stroked at 1.5, four pixels after
                                        // the title.
                                        container(context)
                                            .width(4.0)
                                            .height(10.0);
                                        container(context)
                                            .size(10.0, 10.0)
                                            .draw([DrawCommand::SvgPath {
                                                x: 0.0,
                                                y: 0.0,
                                                width: 10.0,
                                                height: 10.0,
                                                path: if descending {
                                                    "M1 3L5 7L9 3".to_string()
                                                } else {
                                                    "M1 7L5 3L9 7".to_string()
                                                },
                                                fill: None,
                                                stroke: Some(theme.muted),
                                                stroke_width: 1.5,
                                            }]);
                                    }
                                });
                                if cell.clicked {
                                    let next = if sort == Some(index) {
                                        match state.borrow().sort_direction {
                                            SortDirection::Ascending => SortDirection::Descending,
                                            SortDirection::Descending => SortDirection::Ascending,
                                        }
                                    } else {
                                        SortDirection::Ascending
                                    };
                                    let mut state = state.borrow_mut();
                                    state.sort_column = Some(index);
                                    state.sort_direction = next;
                                    probe.set(1);
                                }
                                // The grip's rule at the column's right
                                // edge: a pixel wide, half the header tall
                                // and centered on it, in the border color
                                // (`tableHeader`).
                                if index < DEMO_TABLE_COLUMNS.len() - 1 {
                                    container(context).width(0.0).height(32.0).draw([
                                        DrawCommand::Rect {
                                            x: -1.0,
                                            y: 8.0,
                                            width: 1.0,
                                            height: 16.0,
                                            color: theme.border,
                                        },
                                    ]);
                                }
                            }
                        });
                    // MyGo's divider: a one-pixel rule in the border color.
                    container(context).width(content_width).height(1.0).draw([
                        DrawCommand::Rect {
                            x: 0.0,
                            y: 0.0,
                            width: content_width,
                            height: 1.0,
                            color: theme.border,
                        },
                    ]);
                    for (position, index) in order.iter().enumerate() {
                        let (name, kind, size) = DEMO_FILES[*index];
                        let id = format!("docs-demo-table-row-{position}");
                        let is_selected = Some(position) == selected_row;
                        let cells = [name, kind, size];
                        let row = context
                            .clickable(&id)
                            .size(content_width, 32.0)
                            .background(
                                if is_selected {
                                    theme.accent
                                } else {
                                    Color::TRANSPARENT
                                },
                                6.0,
                            )
                            .children(|context| {
                                row(context).width(content_width).height(32.0).children(|context| {
                                    for (column_index, cell) in cells.iter().enumerate() {
                                        let mut cell_row = row(context)
                                            .width(widths[column_index])
                                            .height(32.0)
                                            .padding_xy(10.0, 6.0)
                                            .align(AlignItems::Center);
                                        if column_index == 2 {
                                            cell_row = cell_row.justify(JustifyContent::FlexEnd);
                                        }
                                        cell_row.children(|context| {
                                            let color = if is_selected {
                                                theme.on_accent
                                            } else {
                                                theme.foreground
                                            };
                                            if column_index == 0 {
                                                row(context)
                                                    .gap(8.0)
                                                    .align(AlignItems::Center)
                                                    .children(|context| {
                                                        let icon_name =
                                                            if name.ends_with(".jpg")
                                                                || name.ends_with(".png")
                                                            {
                                                                "image"
                                                            } else if name.ends_with(".go") {
                                                                "code"
                                                            } else {
                                                                "file"
                                                            };
                                                        icon(
                                                            context,
                                                            &format!("{id}-icon"),
                                                            icon_name,
                                                            16.0,
                                                            color,
                                                        );
                                                        text(context, cell).font_size(14.0).color(color);
                                                    });
                                            } else {
                                                text(context, cell).font_size(14.0).color(color);
                                            }
                                        });
                                    }
                                });
                            });
                        if row.clicked {
                            chosen = name.to_string();
                            state.borrow_mut().selected_id = chosen.clone();
                            probe.set(2);
                        }
                    }
                });
            });
    });
    text(
        context,
        "1 chosen. Click a header to sort; double-click a row to open it",
    )
    .font_size(12.0)
    .color(theme.muted);
}

/// One row of the tree demo: a label, how deep it is, and the path it
/// stands for in MyGo's `collections.go` tree.
struct DemoTreeRow {
    path: &'static str,
    label: &'static str,
    depth: usize,
    open: bool,
    branch: bool,
}

/// Builds the rows the tree demo shows for the open folders: MyGo's
/// `item("mygo", "ui", "docs", "app.go", "window.go", "go.mod")`, with `ui`
/// holding three Go files and `docs` two Markdown ones.
fn demo_tree_rows(open_mygo: bool, open_ui: bool, open_docs: bool) -> Vec<DemoTreeRow> {
    let mut rows = vec![DemoTreeRow {
        path: "mygo",
        label: "mygo",
        depth: 0,
        open: open_mygo,
        branch: true,
    }];
    if open_mygo {
        rows.push(DemoTreeRow { path: "mygo/ui", label: "ui", depth: 1, open: open_ui, branch: true });
        if open_ui {
            for (path, label) in [
                ("mygo/ui/button.go", "button.go"),
                ("mygo/ui/list.go", "list.go"),
                ("mygo/ui/table.go", "table.go"),
            ] {
                rows.push(DemoTreeRow { path, label, depth: 2, open: false, branch: false });
            }
        }
        rows.push(DemoTreeRow { path: "mygo/docs", label: "docs", depth: 1, open: open_docs, branch: true });
        if open_docs {
            for (path, label) in [
                ("mygo/docs/README.md", "README.md"),
                ("mygo/docs/architecture.md", "architecture.md"),
            ] {
                rows.push(DemoTreeRow { path, label, depth: 2, open: false, branch: false });
            }
        }
        for (path, label) in [("mygo/app.go", "app.go"), ("mygo/window.go", "window.go"), ("mygo/go.mod", "go.mod")] {
            rows.push(DemoTreeRow { path, label, depth: 1, open: false, branch: false });
        }
    }
    rows
}

fn tree_demo(
    context: &mut Context,
    _state: &Rc<RefCell<TreeState>>,
    expanded: &Rc<RefCell<Vec<String>>>,
    probe: &Rc<Cell<i32>>,
    chosen: &Rc<RefCell<String>>,
) {
    let theme = context.theme().clone();
    let mut open = expanded.borrow_mut();
    let open_mygo = open.iter().any(|id| id == "mygo");
    let open_ui = open.iter().any(|id| id == "mygo/ui");
    let open_docs = open.iter().any(|id| id == "mygo/docs");
    let rows = demo_tree_rows(open_mygo, open_ui, open_docs);
    let selected = chosen.borrow().clone();
    // MyGo wraps the tree in a Scroll of 206 pixels with padding 6, a
    // one-pixel border and radius 8 (`collections.go`), and a row is
    // Space(7) tall with its arrow in a Space(4) box, 2 between parts and
    // 8 at the ends, indented Space(1+4*depth) (`ui/tree.go`). The preview
    // root centers what a demo builds, so the status line sits centered.
    column(context).width(712.0).gap(12.0).center().children(|context| {
        container(context)
            .size(712.0, 206.0)
            .padding(7.0)
            .border(theme.border, 8.0)
            .clip()
            .children(|context| {
                column(context).width(698.0).children(|context| {
                    for row in &rows {
                        let id = format!("docs-demo-tree-{}", row.path);
                        let is_selected = selected == row.path;
                        let click = context
                            .clickable(&id)
                            .size(698.0, 28.0)
                            .background(
                                if is_selected {
                                    theme.accent
                                } else {
                                    Color::TRANSPARENT
                                },
                                6.0,
                            )
                            // MyGo pads a row Space(1 + 4*depth) at the
                            // start and Space(2) at the end, and gaps the
                            // arrow's Space(4) box from the label by
                            // Space(0.5) (`ui/tree.go` coreTreeItem).
                            .padding_each(0.0, 8.0, 0.0, 4.0 + 16.0 * row.depth as f32)
                            .align(AlignItems::Center)
                            .gap(2.0)
                            .children(|context| {
                                // The arrow's box, then the label.
                                if row.branch {
                                    let arrow_id = format!("{id}-arrow");
                                    let arrow = context
                                        .clickable(&arrow_id)
                                        .size(16.0, 16.0)
                                        .draw([DrawCommand::SvgPath {
                                            x: 0.0,
                                            y: 0.0,
                                            width: 16.0,
                                            height: 16.0,
                                            path: if row.open {
                                                "M4 6L8 10L12 6".to_string()
                                            } else {
                                                "M6 4L10 8L6 12".to_string()
                                            },
                                            fill: None,
                                            stroke: Some(if is_selected {
                                                theme.on_accent
                                            } else {
                                                theme.muted
                                            }),
                                            stroke_width: 1.5,
                                        }]);
                                    if arrow.clicked {
                                        let mut open = expanded.borrow_mut();
                                        if let Some(at) = open.iter().position(|entry| entry == row.path) {
                                            open.remove(at);
                                        } else {
                                            open.push(row.path.to_string());
                                        }
                                        probe.set(1);
                                    }
                                } else {
                                    container(context).width(16.0).height(16.0);
                                }
                                text(context, row.label)
                                    .font_size(14.0)
                                    .color(if is_selected {
                                        theme.on_accent
                                    } else {
                                        theme.foreground
                                    });
                            });
                        if click.clicked {
                            *chosen.borrow_mut() = row.path.to_string();
                            probe.set(2);
                        }
                    }
                });
            });
        text(context, &format!("Chose {selected}"))
            .font_size(12.0)
            .color(theme.muted);
    });
}

/// The outline demo's tree: MyGo's `children` map, as rows in the order the
/// outline shows them.
const DEMO_OUTLINE_CHILDREN: [(&str, &[&str]); 6] = [
    ("mygo", &["mygo/ui", "mygo/docs", "mygo/examples", "mygo/app.go", "mygo/window.go", "mygo/go.mod"]),
    ("mygo/ui", &["mygo/ui/button.go", "mygo/ui/list.go", "mygo/ui/outline.go", "mygo/ui/table.go"]),
    ("mygo/docs", &["mygo/docs/README.md", "mygo/docs/architecture.md"]),
    ("mygo/examples", &["mygo/examples/todo", "mygo/examples/gallery"]),
    ("mygo/examples/todo", &["mygo/examples/todo/main.go"]),
    ("mygo/examples/gallery", &["mygo/examples/gallery/main.go"]),
];

fn outline_children(path: &str) -> Option<&'static [&'static str]> {
    DEMO_OUTLINE_CHILDREN
        .iter()
        .find(|(key, _)| *key == path)
        .map(|(_, kids)| *kids)
}

fn outline_base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn outline_kind(path: &str) -> &'static str {
    if outline_children(path).is_some() {
        return "Folder";
    }
    if path.ends_with(".go") {
        return "Go source";
    }
    if path.ends_with(".md") {
        return "Markdown";
    }
    "Document"
}

/// MyGo's `size` for the outline: a number from the path's own runes.
fn outline_size(path: &str) -> String {
    if outline_children(path).is_some() {
        return "--".to_string();
    }
    let mut value: i64 = 0;
    for character in path.chars() {
        value = value.wrapping_mul(31).wrapping_add(character as i64);
    }
    let bytes = (value.rem_euclid(60_000) + 800) as i64;
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1e6)
    } else if bytes >= 1000 {
        format!("{} KB", bytes / 1000)
    } else {
        format!("{bytes} bytes")
    }
}

/// One visible row of the outline, with its depth.
struct DemoOutlineRow {
    path: &'static str,
    depth: usize,
    open: bool,
    branch: bool,
}

fn demo_outline_rows(expanded: &[String]) -> Vec<DemoOutlineRow> {
    fn walk(
        path: &'static str,
        depth: usize,
        expanded: &[String],
        rows: &mut Vec<DemoOutlineRow>,
    ) {
        let kids = outline_children(path);
        let open = kids.is_some() && expanded.iter().any(|entry| entry == path);
        rows.push(DemoOutlineRow {
            path,
            depth,
            open,
            branch: kids.is_some(),
        });
        if open {
            for kid in kids.expect("open branch has children") {
                // The table lists the children as static strings; the map
                // holds them that way too.
                walk(kid, depth + 1, expanded, rows);
            }
        }
    }
    let mut rows = Vec::new();
    walk("mygo", 0, expanded, &mut rows);
    rows
}

fn outline_demo(
    context: &mut Context,
    mode: &Rc<Cell<usize>>,
    expanded: &Rc<RefCell<Vec<String>>>,
    chosen: &Rc<RefCell<String>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    let selected = chosen.borrow().clone();
    let rows = demo_outline_rows(&expanded.borrow());
    let in_columns = mode.get() == 1;
    column(context).width(712.0).gap(12.0).center().children(|context| {
        // MyGo's Segmented: a track of the surface color, a pixel of border,
        // 2 of padding and 2 between segments; the chosen segment is filled
        // with the background color and casts a small shadow (`ui/toggle.go`
        // coreSegmented, `pressedColor`). Segments pad 4 by 12.
        // The track pads Space(0.5)=2 and adds its one-pixel border to the
        // layout, as MyGo's borders take room (`padX`).
        row(context)
            .gap(2.0)
            .padding(3.0)
            .background(theme.surface, 6.0)
            .border(theme.border, 6.0)
            .children(|context| {
                for (index, label) in ["Outline", "In columns"].iter().enumerate() {
                    let id = format!("docs-demo-outline-mode-{index}");
                    let active = mode.get() == index;
                    let segment = context
                        .clickable(&id)
                        .padding_xy(12.0, 4.0)
                        .background(
                            if active { theme.background } else { Color::TRANSPARENT },
                            4.0,
                        )
                        .draw(if active {
                            vec![DrawCommand::Shadow {
                                x: 0.0,
                                y: 0.0,
                                width: 1.0,
                                height: 1.0,
                                radius: 0.0,
                                offset_x: 0.0,
                                offset_y: 1.0,
                                blur: 2.0,
                                color: Color::rgba(0, 0, 0, 31),
                            }]
                        } else {
                            Vec::new()
                        })
                        .children(|context| {
                            text(context, label).font_size(14.0);
                        });
                    if segment.clicked {
                        mode.set(index);
                        probe.set(1);
                    }
                }
            });
        if in_columns {
            // MyGo's OutlineTable: Name grows, Kind is 120 and Size 90,
            // both ending at their right edge.
            container(context)
                .size(712.0, 206.0)
                .padding(1.0)
                .border(theme.border, 8.0)
                .clip()
                .children(|context| {
                    column(context).width(710.0).children(|context| {
                        row(context).width(710.0).height(32.0).children(|context| {
                            let widths = [500.0_f32, 120.0, 90.0];
                            for (index, title) in ["Name", "Kind", "Size"].iter().enumerate() {
                                let mut head = row(context)
                                    .width(widths[index])
                                    .height(32.0)
                                    .padding_xy(10.0, 6.0)
                                    .align(AlignItems::Center);
                                if index == 2 {
                                    head = head.justify(JustifyContent::FlexEnd);
                                }
                                head.children(|context| {
                                    text(context, title)
                                        .font_size(14.0)
                                        .weight(600)
                                        .color(theme.muted);
                                });
                            }
                        });
                        container(context).width(710.0).height(1.0).draw([
                            DrawCommand::Rect {
                                x: 0.0,
                                y: 0.0,
                                width: 710.0,
                                height: 1.0,
                                color: theme.border,
                            },
                        ]);
                        for row_data in &rows {
                            let id = format!("docs-demo-outline-row-{}", row_data.path);
                            let is_selected = selected == row_data.path;
                            let size_text = outline_size(row_data.path);
                            let cells = [
                                outline_base_name(row_data.path),
                                outline_kind(row_data.path),
                                size_text.as_str(),
                            ];
                            let widths = [500.0_f32, 120.0, 90.0];
                            let click = context
                                .clickable(&id)
                                .size(710.0, 32.0)
                                .background(
                                    if is_selected { theme.accent } else { Color::TRANSPARENT },
                                    6.0,
                                )
                                .children(|context| {
                                    row(context).width(710.0).height(32.0).children(|context| {
                                        for (column_index, cell) in cells.iter().enumerate() {
                                            let mut cell_row = row(context)
                                                .width(widths[column_index])
                                                .height(32.0)
                                                .padding_xy(10.0, 6.0)
                                                .align(AlignItems::Center);
                                            if column_index == 2 {
                                                cell_row =
                                                    cell_row.justify(JustifyContent::FlexEnd);
                                            }
                                            let depth = row_data.depth;
                                            let branch = row_data.branch;
                                            let open = row_data.open;
                                            cell_row.children(|context| {
                                                let color = if is_selected {
                                                    theme.on_accent
                                                } else {
                                                    theme.foreground
                                                };
                                                if column_index == 0 {
                                                    if depth > 0 {
                                                        container(context)
                                                            .width(16.0 * depth as f32);
                                                    }
                                                    // The arrow's 16-pixel
                                                    // box, then the icon and
                                                    // the name, 8 apart.
                                                    if branch {
                                                        container(context).size(16.0, 16.0).draw([
                                                            DrawCommand::SvgPath {
                                                                x: 0.0,
                                                                y: 0.0,
                                                                width: 16.0,
                                                                height: 16.0,
                                                                path: if open {
                                                                    "M12 6L8 10L4 6".to_string()
                                                                } else {
                                                                    "M6 4L10 8L6 12".to_string()
                                                                },
                                                                fill: None,
                                                                stroke: Some(if is_selected {
                                                                    theme.on_accent
                                                                } else {
                                                                    theme.muted
                                                                }),
                                                                stroke_width: 1.5,
                                                            },
                                                        ]);
                                                    } else {
                                                        container(context).size(16.0, 16.0);
                                                    }
                                                    row(context)
                                                        .gap(8.0)
                                                        .align(AlignItems::Center)
                                                        .children(|context| {
                                                            icon(
                                                                context,
                                                                &format!("{id}-icon-{column_index}"),
                                                                if branch { "folder" } else { "file" },
                                                                16.0,
                                                                color,
                                                            );
                                                            text(context, cell)
                                                                .font_size(14.0)
                                                                .color(color);
                                                        });
                                                } else {
                                                    text(context, cell).font_size(14.0).color(color);
                                                }
                                            });
                                        }
                                    });
                                });
                            if click.clicked {
                                *chosen.borrow_mut() = row_data.path.to_string();
                                probe.set(2);
                            }
                        }
                    });
                });
        } else {
            // The plain outline: `Height(206).Padding(4).Border(1).Radius(8)`
            // in the demo; a row pads 4 by 8 with 4 between parts, its indent
            // 16 per level, and is 25 tall (`ui/outline.go`: Gap Space(1),
            // Padding Space(1), Space(2); indent Space(4) per level).
            container(context)
                .size(712.0, 206.0)
                .padding(5.0)
                .border(theme.border, 8.0)
                .clip()
                .children(|context| {
                    column(context).width(702.0).children(|context| {
                        for row_data in &rows {
                            let id = format!("docs-demo-outline-row-{}", row_data.path);
                            let is_selected = selected == row_data.path;
                            let click = context
                                .clickable(&id)
                                .size(702.0, 25.0)
                                .background(
                                    if is_selected { theme.accent } else { Color::TRANSPARENT },
                                    6.0,
                                )
                                .padding_x(8.0)
                                .align(AlignItems::Center)
                                .gap(4.0)
                                .children(|context| {
                                    if row_data.depth > 0 {
                                        container(context)
                                            .width(16.0 * row_data.depth as f32);
                                    }
                                    let color = if is_selected {
                                        theme.on_accent
                                    } else {
                                        theme.foreground
                                    };
                                    if row_data.branch {
                                        container(context).size(16.0, 16.0).draw([
                                            DrawCommand::SvgPath {
                                                x: 0.0,
                                                y: 0.0,
                                                width: 16.0,
                                                height: 16.0,
                                                path: if row_data.open {
                                                    "M12 6L8 10L4 6".to_string()
                                                } else {
                                                    "M6 4L10 8L6 12".to_string()
                                                },
                                                fill: None,
                                                stroke: Some(if is_selected {
                                                    theme.on_accent
                                                } else {
                                                    theme.muted
                                                }),
                                                stroke_width: 1.5,
                                            },
                                        ]);
                                    } else {
                                        container(context).size(16.0, 16.0);
                                    }
                                    icon(
                                        context,
                                        &format!("{id}-icon"),
                                        if row_data.branch { "folder" } else { "file" },
                                        16.0,
                                        color,
                                    );
                                    text(context, outline_base_name(row_data.path))
                                        .font_size(14.0)
                                        .color(color);
                                });
                            if click.clicked {
                                *chosen.borrow_mut() = row_data.path.to_string();
                                probe.set(2);
                            }
                        }
                    });
                });
        }
        text(
            context,
            &format!("Chose {selected}. Right and Left open and close the item chosen"),
        )
        .font_size(12.0)
        .color(theme.muted);
    });
}

/// The grid view demo's photos: a name and the two colors of its gradient,
/// from MyGo's `collections.go` palette.
const DEMO_PHOTOS: [(&str, (u8, u8, u8), (u8, u8, u8)); 24] = [
    ("Beach.jpg", (253, 230, 138), (56, 189, 248)),
    ("Canyon.jpg", (251, 146, 60), (154, 52, 18)),
    ("Desert.jpg", (252, 211, 77), (217, 119, 6)),
    ("Forest.jpg", (74, 222, 128), (22, 101, 52)),
    ("Glacier.jpg", (224, 242, 254), (96, 165, 250)),
    ("Harbor.jpg", (147, 197, 253), (30, 58, 138)),
    ("Island.jpg", (94, 234, 212), (15, 118, 110)),
    ("Lagoon.jpg", (103, 232, 249), (14, 116, 144)),
    ("Meadow.jpg", (190, 242, 100), (77, 124, 15)),
    ("Mountains.jpg", (196, 181, 253), (71, 85, 105)),
    ("Northern Lights.jpg", (52, 211, 153), (49, 46, 129)),
    ("Orchard.jpg", (252, 165, 165), (101, 163, 13)),
    ("Prairie.jpg", (253, 224, 71), (163, 163, 163)),
    ("Rainforest.jpg", (34, 197, 94), (6, 78, 59)),
    ("Reef.jpg", (244, 114, 182), (8, 145, 178)),
    ("Savanna.jpg", (253, 186, 116), (161, 98, 7)),
    ("Sunrise.jpg", (254, 202, 202), (245, 158, 11)),
    ("Sunset.jpg", (249, 115, 22), (124, 58, 237)),
    ("Tundra.jpg", (226, 232, 240), (100, 116, 139)),
    ("Valley.jpg", (134, 239, 172), (21, 128, 61)),
    ("Vineyard.jpg", (216, 180, 254), (127, 29, 29)),
    ("Volcano.jpg", (239, 68, 68), (28, 25, 23)),
    ("Waterfall.jpg", (165, 243, 252), (3, 105, 161)),
    ("Wetlands.jpg", (163, 230, 53), (63, 98, 18)),
];

fn grid_view_demo(
    context: &mut Context,
    selected: &Rc<Cell<usize>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    // MyGo's GridView (`ui/gridview.go`, `website/docs-demo/collections.go`):
    // 110-pixel cells at least, 104 tall, gap Space(2) = 8, in a 236-pixel box
    // with 4 of padding inside its 1-pixel border. Each row pads 4 vertically
    // and rows sit 4 apart. A cell keeps a transparent 2-pixel border that
    // holds its content in place, and a chosen one fills with the pressed
    // surface and borders in the border color. Inside, the gradient card
    // margins 6 at the top and sides and 2 below, its name 12px centered.
    let chosen = selected.get();
    let columns = 6_usize;
    let gap = 8.0_f32;
    let content_width = 702.0_f32;
    let cell_width = (content_width - gap * (columns as f32 - 1.0)) / columns as f32;
    column(context).width(712.0).gap(12.0).center().children(|context| {
        container(context)
            .size(712.0, 236.0)
            .padding(5.0)
            .border(theme.border, 8.0)
            .clip()
            .children(|context| {
                // The rows overflow the box, as MyGo's scroll container lets
                // them: the clip cuts the second row's last pixels rather
                // than squeezing the rows shorter.
                column(context).width(content_width).height(228.0).gap(4.0).children(|context| {
                    for row_start in (0..DEMO_PHOTOS.len()).step_by(columns) {
                        row(context)
                            .width(content_width)
                            .gap(gap)
                            .padding_y(4.0)
                            .children(|context| {
                                for index in row_start..(row_start + columns).min(DEMO_PHOTOS.len())
                                {
                                    let (name, from, to) = DEMO_PHOTOS[index];
                                    let id = format!("docs-demo-grid-view-{index}");
                                    let is_selected = index == chosen;
                                    let cell = context
                                        .clickable(&id)
                                        .column()
                                        .size(cell_width, 104.0)
                                        .background(
                                            if is_selected {
                                                theme.surface_pressed
                                            } else {
                                                Color::TRANSPARENT
                                            },
                                            6.0,
                                        )
                                        .padding(2.0)
                                        .draw(if is_selected {
                                            vec![DrawCommand::SvgPath {
                                                x: 1.0,
                                                y: 1.0,
                                                width: cell_width - 2.0,
                                                height: 102.0,
                                                path: rounded_rect_path(cell_width - 2.0, 102.0, 5.0),
                                                fill: None,
                                                stroke: Some(theme.border),
                                                stroke_width: 2.0,
                                            }]
                                        } else {
                                            Vec::new()
                                        })
                                        .children(|context| {
                                            // MyGo's 135° gradient runs from the
                                            // box's center by
                                            // (|W·dx| + |H·dy|)/2 each way, so
                                            // its ends sit outside the corners
                                            // (`ui/paint.go` gradient).
                                            let card_width = cell_width - 4.0 - 12.0;
                                            // The cell's 104 pixels hold its
                                            // two-pixel border, the card's 6
                                            // and 2 margins and the name's
                                            // 19-pixel line box.
                                            let card_height = 104.0 - 4.0 - 8.0 - 19.0;
                                            let (dx, dy) = (0.707_106_77_f32, 0.707_106_77_f32);
                                            let half = (card_width * dx + card_height * dy) / 2.0;
                                            let start_x = 0.5 - dx * half / card_width;
                                            let start_y = 0.5 - dy * half / card_height;
                                            let end_x = 0.5 + dx * half / card_width;
                                            let end_y = 0.5 + dy * half / card_height;
                                            container(context)
                                                .width(card_width)
                                                .flex_grow(1.0)
                                                .margin_each(6.0, 6.0, 2.0, 6.0)
                                                .draw([DrawCommand::LinearGradient {
                                                    x: 0.0,
                                                    y: 0.0,
                                                    width: card_width,
                                                    height: card_height,
                                                    radius: myrust_ui::Radius::all(6.0),
                                                    start: Point::new(start_x, start_y),
                                                    end: Point::new(end_x, end_y),
                                                    stops: vec![
                                                        GradientStop {
                                                            offset: 0.0,
                                                            color: Color::rgb(from.0, from.1, from.2),
                                                        },
                                                        GradientStop {
                                                            offset: 1.0,
                                                            color: Color::rgb(to.0, to.1, to.2),
                                                        },
                                                    ],
                                                }]);
                                            // MyGo's name centers across the
                                            // cell in an 18-pixel line box
                                            // and pads 6 at the sides and 4
                                            // below (`website/docs-demo/collections.go`).
                                            // Its node is as wide as its text
                                            // plus both paddings, so centering
                                            // happens in the box inside them.
                                            text(context, name)
                                                .single_line()
                                                .font_size(12.0)
                                                .color(theme.foreground)
                                                .align(myrust_ui::TextAlignment::Center)
                                                .max_width(cell_width - 4.0 - 12.0)
                                                .line_height(19.0)
                                                .padding_each(0.0, 6.0, 4.0, 6.0);
                                        });
                                    if cell.clicked {
                                        selected.set(index);
                                        probe.set(index as i32 + 1);
                                    }
                                }
                            });
                    }
                });
            });
        text(
            context,
            &format!(
                "1 of {} chosen. The arrows move in both directions; drag photos to reorder them",
                DEMO_PHOTOS.len()
            ),
        )
        .font_size(12.0)
        .color(theme.muted);
    });
}

/// The tile colors of MyGo's grid demo (`website/docs-demo/containers.go`
/// containersTiles).
const DEMO_GRID_TILES: [(u8, u8, u8); 8] = [
    (0xf9, 0x73, 0x16),
    (0xea, 0xb3, 0x08),
    (0x22, 0xc5, 0x5e),
    (0x14, 0xb8, 0xa6),
    (0x3b, 0x82, 0xf6),
    (0x8b, 0x5c, 0xf6),
    (0xec, 0x48, 0x99),
    (0xef, 0x44, 0x44),
];

/// The state of MyGo's grid demo: how many tiles it shows, in how many
/// columns, and the profile form's text.
struct GridDemoState {
    tiles: usize,
    columns: usize,
    email: String,
    bio: String,
}

/// The full weekday name of a day, as MyGo's `Format("Monday")` prints it.
fn weekday_full(year: i32, month: u32, day: u32) -> &'static str {
    ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
        [myrust_ui::weekday_monday(year, month, day) as usize]
}

/// An SVG path for a rounded rectangle of the given size.
fn rounded_rect_path(width: f32, height: f32, radius: f32) -> String {
    let r = radius;
    format!(
        "M{r} 0H{w1}A{r} {r} 0 0 1 {w} {r}V{h1}A{r} {r} 0 0 1 {w1} {h}H{r}A{r} {r} 0 0 1 0 {h1}V{r}A{r} {r} 0 0 1 {r} 0Z",
        r = r,
        w1 = width - r,
        w = width,
        h1 = height - r,
        h = height,
    )
}


fn scroll_demo(context: &mut Context, scroll: &Rc<Cell<f32>>, probe: &Rc<Cell<i32>>) {
    // MyGo's scroll demo (`website/docs-demo/containers.go` containersScroll):
    // a row of three buttons over a 150-tall log that follows its end unless
    // the user scrolled up from it, its lines 12px monospace, padding 10
    // vertically and 12 across, gap 4, on the surface with a border and
    // radius 8, stretched across the preview.
    let theme = context.theme().clone();
    let state = Rc::clone(scroll);

    // MyGo stretches the log across the preview's column
    // (`AlignSelf(Stretch)` in containersScroll).
    column(context)
        .width(712.0)
        .gap(12.0)
        .align(AlignItems::Center)
        .children(|context| {
        row(context).gap(8.0).children(|context| {
            if button(context, "Add a line").clicked {
                probe.set(probe.get() + 1);
            }
            if button(context, "Top").clicked {
                state.set(0.0);
                probe.set(-1);
            }
            if button(context, "End").clicked {
                state.set(f32::MAX);
                probe.set(-2);
            }
        });
        // MyGo's scroll viewport is 150 DIPs tall, its content scrolling
        // inside: the box's height is the viewport, the border takes no
        // room of its own in MyGo's layout, so the padding is inside it.
        let view = scroll_view_styled(
            context,
            "docs-demo-scroll",
            150.0,
            ScrollStyle {
                background: Some(theme.surface),
                border: Some(theme.border),
                radius: 8.0,
                padding: Edges::new(10.0, 12.0, 10.0, 12.0),
                gap: 4.0,
                initial_offset: Some(f32::MAX),
            },
            |context| {
                for index in 0..DEMO_SCROLL_LINES {
                    text(context, &format!("Built package {} of 12", index + 1))
                        .font_size(12.0)
                        .line_height(14.0)
                        .mono()
                        .color(theme.foreground);
                }
            },
        );
        state.set(view.scroll);
        // The status line is centered under the log, as the preview's
        // column centers what is narrower than it.
        container(context)
            .width(712.0)
            .justify(JustifyContent::Center)
            .children(|context| {
            text(context, "At the end: new lines scroll into view")
                .font_size(12.0)
                .color(theme.muted);
        });
    });
}

/// How many lines MyGo's scroll demo starts with.
const DEMO_SCROLL_LINES: usize = 12;

fn grid_demo(context: &mut Context, state: &Rc<RefCell<GridDemoState>>, probe: &Rc<Cell<i32>>) {
    // MyGo's grid demo (`website/docs-demo/containers.go` containersGrid):
    // a row of two columns 32 apart. The left one, 260 wide, holds a grid of
    // equal columns, gap 8, of 36-tall tiles rounded 8, colored from a
    // palette and numbered; below it a row of three buttons, gap 8. The
    // right one is a 300-wide grid whose first track fits its content and
    // whose second takes the rest, gap 16 across and 8 down: a Profile
    // heading spanning both columns, then Email and its input, then Bio and
    // its text area.
    let theme = context.theme().clone();
    let mut state = state.borrow_mut();
    let tiles = state.tiles;
    let columns = state.columns;
    // The preview's own column centers the demo's row (`docs-demo/main.go`
    // preview: Column Fill Center Gap 12 Padding 20); padding 20 inside a
    // 712-wide column centers the same as none inside the full window.
    row(context).gap(32.0).align(AlignItems::FlexStart).children(|context| {
        column(context).width(260.0).gap(10.0).children(|context| {
            grid(context).columns(columns).gap(8.0).children(|context| {
                for index in 0..tiles {
                    let (r, g, b) = DEMO_GRID_TILES[index % DEMO_GRID_TILES.len()];
                    container(context)
                        .height(36.0)
                        .background(Color::rgb(r, g, b), 8.0)
                        .center()
                        .children(|context| {
                            text(context, &format!("{}", index + 1))
                                .bold()
                                .color(Color::WHITE);
                        });
                }
            });
            row(context).gap(8.0).children(|context| {
                if button_disabled(context, "Add", tiles >= 8).clicked {
                    state.tiles = (tiles + 1).min(8);
                    probe.set(state.tiles as i32);
                }
                if button_disabled(context, "Remove", tiles <= 1).clicked {
                    state.tiles = tiles.saturating_sub(1).max(1);
                    probe.set(state.tiles as i32);
                }
                let label = format!("Columns: {}", columns);
                if button(context, &label).clicked {
                    state.columns = 2 + (columns - 1) % 3;
                    probe.set(state.columns as i32);
                }
            });
        });
        // The right grid's first track fits its labels; its second takes
        // the rest (`website/docs-demo/containers.go` ColumnTracks). Each
        // label is a cell of the 300-wide grid, padded 7 from its top
        // (`Padding(7, 0, 0)`), so the track is as wide as the widest.
        let label_width = ["Email", "Bio"]
            .iter()
            .map(|label| myrust_ui::host_text_width(label, 14.0).unwrap_or(34.49))
            .fold(0.0_f32, f32::max);
        let field_width = (300.0 - label_width - 16.0).max(1.0);
        column(context).width(300.0).gap(8.0).children(|context| {
            text(context, "Profile").bold().color(theme.foreground);
            row(context).gap(16.0).align(AlignItems::FlexStart).children(|context| {
                text(context, "Email")
                    .color(theme.muted)
                    .width(label_width)
                    .padding_each(7.0, 0.0, 0.0, 0.0);
                container(context).width(field_width).children(|context| {
                    let mut email = state.email.clone();
                    text_field(context, "docs-demo-grid-email", &mut email);
                    state.email = email;
                });
            });
            row(context).gap(16.0).align(AlignItems::FlexStart).children(|context| {
                text(context, "Bio")
                    .color(theme.muted)
                    .width(label_width)
                    .padding_each(7.0, 0.0, 0.0, 0.0);
                container(context).width(field_width).children(|context| {
                    let mut bio = state.bio.clone();
                    text_area_sized_placeholder(
                        context,
                        "docs-demo-grid-bio",
                        &mut bio,
                        4,
                        79.0,
                        "A line or two",
                    );
                    state.bio = bio;
                });
            });
        });
    });
}

/// MyGo's split demo state: the sidebar's DIP width, the editor's DIP
/// height and the file the sidebar shows open.
#[derive(Debug, Clone, Copy)]
struct SplitDemoState {
    sidebar: f32,
    editor: f32,
    open: usize,
}

impl Default for SplitDemoState {
    fn default() -> Self {
        Self {
            sidebar: 180.0,
            editor: 120.0,
            open: 0,
        }
    }
}

fn split_demo(context: &mut Context, state: &Rc<RefCell<SplitDemoState>>, probe: &Rc<Cell<i32>>) {
    // MyGo's split demo (`website/docs-demo/containers.go`): a sidebar of
    // files beside an editor split vertically over a terminal, the whole
    // 200 tall inside a one-pixel border.
    const FILES: [&str; 4] = ["main.go", "app.go", "view.go", "go.mod"];
    let theme = context.theme().clone();
    let mut demo = *state.borrow();
    let selection = Color::rgba(37, 99, 235, 64);
    let mut sidebar = demo.sidebar;
    let mut editor = demo.editor;
    let open = demo.open;
    container(context)
        .width(712.0)
        .height(200.0)
        .border(theme.border, 8.0)
        // MyGo's border takes a pixel of the layout (`padX`), so the panes
        // start inside it rather than painting over it.
        .padding(1.0)
        .clip()
        .children(|context| {
            split_sized(
                context,
                "docs-demo-split",
                &mut sidebar,
                SplitOrientation::Horizontal,
                40.0,
                6.0,
                |context| {
                    column(context)
                        .fill_parent()
                        .padding(8.0)
                        .gap(2.0)
                        .background(theme.surface, 0.0)
                        .children(|context| {
                            for (index, name) in FILES.iter().enumerate() {
                                let id = format!("docs-demo-split-file-{index}");
                                let row = context
                                    .clickable(&id)
                                    .gap(6.0)
                                    .padding_xy(8.0, 5.0)
                                    .background(
                                        if index == open {
                                            selection
                                        } else {
                                            Color::TRANSPARENT
                                        },
                                        6.0,
                                    )
                                    .align(AlignItems::Center)
                                    .children(|context| {
                                        icon(
                                            context,
                                            &format!("{id}-icon"),
                                            "file",
                                            14.0,
                                            theme.foreground,
                                        );
                                        text(context, name).single_line();
                                    });
                                if row.clicked {
                                    demo.open = index;
                                }
                            }
                        });
                },
                |context| {
                    split_sized(
                        context,
                        "docs-demo-split-editor",
                        &mut editor,
                        SplitOrientation::Vertical,
                        40.0,
                        6.0,
                        |context| {
                            column(context).fill_parent().padding(12.0).gap(6.0).children(|context| {
                                text(context, FILES[open]).bold();
                                text(context, "package main\n\nfunc main() {}")
                                    .mono()
                                    .font_size(12.0)
                                    .color(theme.muted);
                            });
                        },
                        |context| {
                            row(context)
                                .fill_parent()
                                .align(AlignItems::FlexStart)
                                .padding_xy(12.0, 10.0)
                                .gap(6.0)
                                .background(theme.surface, 0.0)
                                .children(|context| {
                                    icon(
                                        context,
                                        "docs-demo-split-term-icon",
                                        "terminal",
                                        14.0,
                                        theme.muted,
                                    );
                                    text(context, "go run .").mono().font_size(12.0);
                                });
                        },
                    );
                },
            );
        });
    text(
        context,
        &format!(
            "Sidebar {:.0} DIPs wide, editor {:.0} DIPs high · double-click to reset",
            demo.sidebar, demo.editor
        ),
    )
    .font_size(12.0)
    .color(theme.muted);
    demo.sidebar = sidebar;
    demo.editor = editor;
    let changed = demo.sidebar != state.borrow().sidebar
        || demo.editor != state.borrow().editor
        || demo.open != state.borrow().open;
    if changed {
        *state.borrow_mut() = demo;
        probe.set(demo.sidebar as i32);
    }
}

fn collapsible_demo(
    context: &mut Context,
    advanced: &Rc<Cell<bool>>,
    verbose: &Rc<Cell<bool>>,
    experimental: &Rc<Cell<bool>>,
    details: &Rc<Cell<bool>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    // MyGo's demo (`website/docs-demo/containers.go`): a 320-wide column
    // gap 10 with a Collapsible of two checkboxes, then a CollapsibleBase
    // whose trigger is a turning chevron and the label Details.
    column(context)
        .width(320.0)
        .gap(10.0)
        .children(|context| {
            let mut open = advanced.get();
            let state = collapsible(context, "docs-demo-collapsible", &mut open, "Advanced", |context| {
                let mut value = verbose.get();
                if checkbox_labeled(context, "docs-demo-verbose", "Verbose logging", &mut value).changed {
                    verbose.set(value);
                    probe.set(if value { 1 } else { 0 });
                }
                let mut value = experimental.get();
                if checkbox_labeled(
                    context,
                    "docs-demo-experimental",
                    "Experimental features",
                    &mut value,
                )
                .changed
                {
                    experimental.set(value);
                    probe.set(if value { 1 } else { 0 });
                }
            });
            if state.changed {
                advanced.set(open);
                probe.set(if open { 1 } else { 0 });
            }
            // The second disclosure builds its own trigger, whose chevron
            // turns as the panel opens (`CollapsibleBase`).
            let mut open = details.get();
            let state = collapsible_custom(
                context,
                "docs-demo-details",
                &mut open,
                "Details",
                |context| {
                    text(
                        context,
                        "CollapsibleBase grows its panel with Progress, for a look of your own.",
                    )
                    .font_size(theme.font_sizes.body)
                    .color(theme.muted);
                },
            );
            if state.changed {
                details.set(open);
                probe.set(if open { 1 } else { 0 });
            }
        });
}

fn accordion_demo(
    context: &mut Context,
    section: &Rc<RefCell<usize>>,
    name: &Rc<RefCell<String>>,
    sync: &Rc<Cell<bool>>,
    analytics: &Rc<Cell<bool>>,
    crashes: &Rc<Cell<bool>>,
    beta: &Rc<Cell<bool>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    let mut section = section.borrow_mut();
    let mut opened = None;
    let status_text = |section: usize| {
        if section >= 3 {
            "Every section closed".to_string()
        } else {
            format!(
                "{} open: opening another closes it",
                ["General", "Privacy", "Advanced"][section]
            )
        }
    };
    let status = status_text(*section);
    // MyGo's demo (`website/docs-demo/containers.go` containersAccordion):
    // one section open at a time in a 380-wide accordion, the status
    // centered beneath it.
    column(context)
        .width(380.0)
        .gap(12.0)
        .align(AlignItems::Center)
        .children(|context| {
        context.accordion_group("docs-demo-accordion", |context| {
            let titles = ["General", "Privacy", "Advanced"];
            for (index, title) in titles.iter().enumerate() {
                let mut open = *section == index;
                let key = format!("docs-demo-accordion-{index}");
                let response = context.accordion_item(&key, title, &mut open, |context| {
                    match index {
                        0 => {
                            column(context).gap(6.0).children(|context| {
                                text(context, "Device name").weight(500).color(theme.foreground);
                                let mut value = name.borrow_mut();
                                if text_field(context, "docs-demo-accordion-name", &mut value)
                                    .changed
                                {
                                    probe.set(value.len() as i32);
                                }
                            });
                            let mut checked = sync.get();
                            if checkbox_labeled(
                                context,
                                "docs-demo-accordion-sync",
                                "Sync settings",
                                &mut checked,
                            )
                            .changed
                            {
                                sync.set(checked);
                            }
                        }
                        1 => {
                            let mut value = analytics.get();
                            if checkbox_labeled(
                                context,
                                "docs-demo-accordion-analytics",
                                "Share analytics",
                                &mut value,
                            )
                            .changed
                            {
                                analytics.set(value);
                            }
                            let mut value = crashes.get();
                            if checkbox_labeled(
                                context,
                                "docs-demo-accordion-crashes",
                                "Send crash reports",
                                &mut value,
                            )
                            .changed
                            {
                                crashes.set(value);
                            }
                        }
                        _ => {
                            let mut value = beta.get();
                            if checkbox_labeled(
                                context,
                                "docs-demo-accordion-beta",
                                "Join the beta channel",
                                &mut value,
                            )
                            .changed
                            {
                                beta.set(value);
                            }
                        }
                    }
                });
                if response.changed {
                    opened = Some(if open { index } else { usize::MAX });
                }
            }
        });
        // MyGo's preview column centers each child, so the status line sits
        // centered under the accordion rather than at its left edge.
        text(context, &status).font_size(12.0).color(theme.muted);
    });
    if let Some(next) = opened {
        *section = if next == usize::MAX { usize::MAX } else { next };
        probe.set(*section as i32 + 1);
    }
}

/// The form demo's state, as MyGo's `containersForm` keeps it.
struct FormDemoState {
    name: String,
    email: String,
    plan: usize,
    mail: bool,
    sound: bool,
}

impl Default for FormDemoState {
    fn default() -> Self {
        // MyGo starts on "Ada Lovelace", "ada", "Pro", mail on, sound off.
        Self {
            name: "Ada Lovelace".to_string(),
            email: "ada".to_string(),
            plan: 1,
            mail: true,
            sound: false,
        }
    }
}

const FORM_PLANS: [&str; 3] = ["Free", "Pro", "Team"];

fn form_demo(context: &mut Context, state: &Rc<RefCell<FormDemoState>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let mut demo = state.borrow_mut();
    // MyGo's demo (`website/docs-demo/containers.go` containersForm): a
    // 400-wide column holding a form of four fields, the last a fieldset.
    column(context).width(400.0).children(|context| {
        // MyGo's `alignLabels` makes every label as wide as the widest
        // field label (`ui/form.go` alignLabels): a fieldset's legend is
        // not one, but the fields inside it are, so "Sound" sets the width.
        let label_width = ["Name", "Email", "Plan", "Sound"]
            .iter()
            .map(|label| myrust_ui::host_text_width(label, 14.0).unwrap_or(38.84))
            .fold(0.0_f32, f32::max);
        context.set_form_label_width(label_width);
        form_group(context, "docs-demo-form", |context| {
            let invalid = if demo.email.is_empty() || demo.email.contains('@') {
                None
            } else {
                Some("Enter an email address.")
            };
            form_row(
                context,
                "docs-demo-form-name",
                "Name",
                None,
                None,
                |context| {
                    if text_field(context, "docs-demo-form-name-input", &mut demo.name).changed {
                        probe.set(demo.name.len() as i32);
                    }
                },
            );
            form_row(
                context,
                "docs-demo-form-email",
                "Email",
                Some("For receipts."),
                invalid,
                |context| {
                    // MyGo marks the input's own border while the field
                    // reports an error (`ui/form.go` inputBorder).
                    if text_field_invalid(
                        context,
                        "docs-demo-form-email-input",
                        &mut demo.email,
                        invalid.is_some(),
                    )
                    .changed
                    {
                        probe.set(demo.email.len() as i32);
                    }
                },
            );
            form_row(
                context,
                "docs-demo-form-plan",
                "Plan",
                None,
                None,
                |context| {
                    // MyGo's field makes the select fill its column: the
                    // form is 400 wide, its label column the widest label
                    // and a Space(3) gap between.
                    let width = 400.0 - label_width - theme.spacing.base * 3.0;
                    let response = select_sized(
                        context,
                        "docs-demo-form-plan-select",
                        &mut demo.plan,
                        &FORM_PLANS,
                        width,
                    );
                    if response.changed {
                        probe.set(response.selected as i32 + 1);
                    }
                },
            );
            // The fieldset: a legend, then its own fields.
            column(context)
                .gap(theme.spacing.base * 3.0)
                .margin_each(theme.spacing.base * 3.0, 0.0, 0.0, 0.0)
                .children(|context| {
                    text(context, "Notifications").weight(600);
                    form_row(
                        context,
                        "docs-demo-form-mail",
                        "Email",
                        None,
                        None,
                        |context| {
                            let mut checked = demo.mail;
                            if checkbox_labeled(
                                context,
                                "docs-demo-form-mail-check",
                                "Weekly summary",
                                &mut checked,
                            )
                            .changed
                            {
                                demo.mail = checked;
                            }
                        },
                    );
                    form_row(
                        context,
                        "docs-demo-form-sound",
                        "Sound",
                        None,
                        None,
                        |context| {
                            let mut on = demo.sound;
                            if switch(context, "docs-demo-form-sound-switch", &mut on).changed {
                                demo.sound = on;
                            }
                        },
                    );
                });
        });
    });
}

fn sidebar_demo(context: &mut Context, selected: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    // MyGo's sidebar demo (`website/docs-demo/navigation.go`): a 240-tall
    // frame with a border, radius 10 and clipping, holding a 200-wide
    // sidebar and its page.
    let theme = context.theme().clone();
    let mut selected = selected.borrow_mut();
    // MyGo's frame is `AlignSelf(Stretch)`, so it takes the preview
    // column's whole width rather than a width of its own.
    // MyGo's border takes layout room (`ui/layout.go` padX = pad + border),
    // so the frame's children sit a pixel inside its edge.
    column(context)
        .size(712.0, 240.0)
        .padding(1.0)
        .background(theme.background, 10.0)
        .border(theme.border, 10.0)
        .clip()
        .children(|context| {
            // MyGo's `Fill()` is 100% of the parent's box; the frame is a
            // column, so growing takes its height and stretching its width.
            row(context).fill_parent().align(AlignItems::Stretch).children(|context| {
                container(context)
                    .width(200.0)
                    .background(theme.surface, 0.0)
                    .children(|context| {
                        let mailboxes = [
                            SidebarItem::new("inbox", "Inbox").icon("inbox").badge("3"),
                            SidebarItem::new("drafts", "Drafts").icon("draft"),
                            SidebarItem::new("sent", "Sent").icon("send"),
                        ];
                        let tags = [
                            SidebarItem::new("work", "Work").icon("tag"),
                            SidebarItem::new("family", "Family").icon("tag"),
                        ];
                        let sections = [
                            SidebarSection::new("Mailboxes", &mailboxes),
                            SidebarSection::new("Tags", &tags),
                        ];
                        let response = sidebar(context, "docs-demo-sidebar", &mut selected, &sections);
                        if response.changed || response.clicked_id.is_some() {
                            probe.set(selected.len() as i32);
                        }
                    });
                // MyGo's page is `Column(c).Grow(1).Padding(16).Gap(8)
                // .AlignItems(Start)` (`website/docs-demo/navigation.go`).
                container(context)
                    .column()
                    .flex_grow(1.0)
                    .padding(16.0)
                    .gap(8.0)
                    .background(theme.background, 0.0)
                    .align(AlignItems::FlexStart)
                    .children(|context| {
                        let name = match selected.as_str() {
                            "inbox" => "Inbox",
                            "drafts" => "Drafts",
                            "sent" => "Sent",
                            "work" => "Work",
                            _ => "Family",
                        };
                        text(context, name).font_size(16.0).bold().color(theme.foreground);
                        if selected.as_str() == "inbox" {
                            text(context, "3 unread messages").font_size(12.0).color(theme.muted);
                            if button(context, "Mark all as read").clicked {
                                probe.set(99);
                            }
                        } else {
                            text(context, "No messages").font_size(12.0).color(theme.muted);
                        }
                    });
            });
        });
}

fn breadcrumbs_demo(context: &mut Context, count: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    stack(context, 460.0, 10.0, |context| {
        let items = [
            BreadcrumbItem::new("macintosh-hd", "Macintosh HD"),
            BreadcrumbItem::new("users", "Users"),
            BreadcrumbItem::new("ada", "ada"),
            BreadcrumbItem::new("documents", "Documents"),
        ];
        let response = breadcrumbs(context, "docs-demo-crumbs", &items);
        if response.clicked.is_some() {
            count.set(count.get() + 1);
            probe.set(count.get());
        }
        row(context).gap(8.0).children(|context| {
            if button_with_icon(context, "Notes", "folder").clicked {
                count.set(count.get() + 1);
                probe.set(count.get());
            }
            if button_with_icon(context, "Projects", "folder").clicked {
                count.set(count.get() + 1);
                probe.set(count.get());
            }
        });
        // MyGo's status line is the path joined from its second element
        // (`navigation.go`: "/" + strings.Join(path[1:], "/")).
        text(context, "/Users/ada/Documents")
            .font_size(12.0)
            .color(Color::rgb(113, 113, 122));
    });
}

fn history_demo(context: &mut Context, router_state: &Rc<RefCell<RouterState>>, probe: &Rc<Cell<i32>>) {
    // MyGo's demo (`website/docs-demo/navigation.go` history-buttons): a
    // 200-tall frame with a border and radius 10, its toolbar of the back
    // and forward buttons and the page's title, a divider, then the route's
    // own column.
    let theme = context.theme().clone();
    let mut state = router_state.borrow_mut();
    // MyGo's frame is `AlignSelf(Stretch)`, taking the preview's width, and
    // `Height(200)` tall; the preview column centers it, leaving the page
    // above and below.
    column(context)
        .size(712.0, 200.0)
        .padding(1.0)
        .background(theme.background, 10.0)
        .border(theme.border, 10.0)
        .clip()
        .children(|context| {
            // `Toolbar(c, ...).Padding(6, 8)`: six above and below, eight
            // across, its items eight apart.
            row(context)
                .padding_each(6.0, 8.0, 6.0, 8.0)
                .gap(8.0)
                .align(AlignItems::Center)
                .children(|context| {
                    let buttons = back_forward_buttons(context, "docs-demo-history", &mut state);
                    if buttons.back_clicked || buttons.forward_clicked {
                        probe.set(state.history().len() as i32 + 10);
                    } else {
                        probe.set(state.history().len() as i32);
                    }
                    text(context, page_title(state.current()))
                        .bold()
                        .color(theme.foreground);
                });
            // `ui.Divider(c)`: a one-pixel rule across.
            container(context)
                .height(1.0)
                .background(theme.border, 0.0);
            // `router.View`: a column padded 16 with a gap of 10, on the
            // background; MyGo's frame is `Height(200)`, which the preview
            // column's `Center` shrinks to what the page holds.
            container(context)
                .column()
                .padding(16.0)
                .gap(10.0)
                .background(theme.background, 0.0)
                .align(AlignItems::FlexStart)
                .children(|context| {
                    let page = state.current().to_string();
                    match page.as_str() {
                        "/" | "" => {
                            text(context, "Open a page, then go back and forward")
                                .font_size(12.0)
                                .color(theme.muted);
                            if link(context, "docs-demo-history-notes", "Notes").clicked {
                                state.push("/notes");
                            }
                            if link(context, "docs-demo-history-files", "Files").clicked {
                                state.push("/files");
                            }
                        }
                        "/notes" => {
                            if link(context, "docs-demo-history-groceries", "Groceries").clicked {
                                state.push("/notes/groceries");
                            }
                            if link(context, "docs-demo-history-ideas", "Ideas").clicked {
                                state.push("/notes/ideas");
                            }
                        }
                        "/files" => {
                            if link(context, "docs-demo-history-report", "Report.pdf").clicked {
                                state.push("/files/report");
                            }
                        }
                        other if other.starts_with("/notes/") => {
                            let id = other.trim_start_matches("/notes/");
                            text(context, &format!("The note “{id}”"))
                                .color(theme.foreground);
                        }
                        _ => {
                            text(context, "A file").color(theme.foreground);
                        }
                    }
                });
        });
}

/// The title MyGo's router gives a page (`website/docs-demo/navigation.go`
/// history-buttons).
fn page_title(page: &str) -> &'static str {
    match page {
        "/notes" => "Notes",
        "/files" => "Files",
        other if other.starts_with("/notes/") => "Note",
        other if other.is_empty() => "Home",
        "/" => "Home",
        _ => "File",
    }
}

fn dialog_demo(
    context: &mut Context,
    open: &Rc<Cell<bool>>,
    result: &Rc<Cell<i32>>,
    name_cell: &Rc<RefCell<String>>,
    draft: &Rc<RefCell<String>>,
    probe: &Rc<Cell<i32>>,
) {
    let theme = context.theme().clone();
    stack(context, 320.0, 10.0, |context| {
        // MyGo's demo (`website/docs-demo/dialogs.go`): a "Rename…" button
        // opens a modal titled Rename with the folder's name in a field and
        // Cancel and Rename below, and the folder's name shows under it.
        let mut name = name_cell.borrow().clone();
        let mut is_open = open.get();
        if button(context, "Rename…").clicked {
            *draft.borrow_mut() = name.clone();
            is_open = true;
        }
        let mut draft_value = draft.borrow().clone();
        let response = dialog(context, "docs-demo-dialog", &mut is_open, 320.0, |context| {
            text(context, "Rename").bold().color(theme.foreground);
            text_field(context, "docs-demo-dialog-field", &mut draft_value);
            let mut action = DialogAction::None;
            row(context).gap(8.0).justify(JustifyContent::FlexEnd).children(|context| {
                if button(context, "Cancel").clicked {
                    action = DialogAction::Cancel;
                }
                if button_primary(context, "Rename").clicked {
                    action = DialogAction::Confirm;
                }
            });
            action
        });
        *draft.borrow_mut() = draft_value.clone();
        open.set(response.open);
        if response.confirmed {
            name = draft_value;
            *name_cell.borrow_mut() = name.clone();
        }
        if response.open {
            probe.set(10);
        }
        if response.confirmed || response.cancelled {
            probe.set(if response.confirmed { 1 } else { 2 });
        }
        text(context, &format!("The folder is “{}”", name))
            .font_size(12.0)
            .color(theme.muted);
    });
}

fn alert_demo(
    context: &mut Context,
    open: &Rc<Cell<bool>>,
    result: &Rc<Cell<i32>>,
    probe: &Rc<Cell<i32>>,
) {
    // MyGo's demo (`website/docs-demo/dialogs.go` alert-dialog): a Delete
    // button that asks, then either deletes the note or reports the cancel.
    stack(context, 320.0, 10.0, |context| {
        let mut is_open = open.get();
        if button(context, "Delete").clicked {
            is_open = true;
        }
        let response = alert_dialog(
            context,
            "docs-demo-alert",
            &mut is_open,
            "Delete “Notes”?",
            "You can't undo this.",
            // The panel draws cancel on the left and confirm on the right
            // (`ui/dialog.rs` alert_dialog), which is the order MyGo's demo
            // passes its buttons in: Cancel, then Delete.
            "Delete",
            "Cancel",
        );
        open.set(response.open);
        if response.open {
            probe.set(10);
        }
        if response.confirmed {
            result.set(1);
        } else if response.cancelled {
            result.set(2);
        }
        if result.get() != 0 {
            probe.set(result.get());
        }
        if result.get() == 2 {
            let muted = context.theme().muted;
            text(context, "Canceled").font_size(12.0).color(muted);
        }
    });
}

fn popover_demo(context: &mut Context, state: &Rc<RefCell<PopoverDemoState>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    // MyGo's demo (`website/docs-demo/dialogs.go` popover): a row filling
    // the preview, a Dimensions button whose popover sets a layer's size,
    // and the layer itself centered beside it. The popover starts open so
    // that the page shows its panel.
    row(context)
        .fill_parent()
        .align_self(AlignItems::Stretch)
        .gap(24.0)
        .align(AlignItems::FlexStart)
        .children(|context| {
            let mut demo = state.borrow_mut();
            let trigger = button_with_icon(context, "Dimensions", "ruler");
            if trigger.clicked {
                demo.open = !demo.open;
            }
            let mut open = demo.open;
            let (mut width, mut height, mut radius) =
                (demo.width, demo.height, demo.radius);
            if open {
                let anchor = trigger
                    .bounds
                    .unwrap_or_else(|| myrust_ui::Rect::new(20.0, 20.0, 130.0, 30.0));
                let mut overlay = OverlayState::new(true);
                popover(context, "docs-demo-popover", &mut overlay, anchor, |context| {
                    // Column(c).Width(240).Gap(12).Padding(4) inside MyGo's
                    // stylePanel, which pads Space(1) and edges in Border.
                    column(context)
                        .width(240.0)
                        .gap(12.0)
                        .padding(4.0)
                        .children(|context| {
                            column(context).gap(2.0).children(|context| {
                                text(context, "Dimensions").weight(700);
                                text(context, "Set the size of the layer.")
                                    .font_size(12.0)
                                    .color(theme.muted);
                            });
                            // MyGo's grid: FitContent, Fr(1), FitContent
                            // across 10px gaps, inside the 240-wide column
                            // (`website/docs-demo/dialogs.go` popover). The
                            // tracks are shared, so every row's label starts
                            // at the widest one and every slider lines up.
                            let labels = ["Width", "Height", "Radius"];
                            let label_width = labels
                                .iter()
                                .map(|label| {
                                    myrust_ui::host_text_width(label, 12.0).unwrap_or(0.0)
                                })
                                .fold(0.0_f32, f32::max);
                            let slider_width = (232.0 - label_width - 20.0 - 24.0).max(0.0);
                            column(context).gap(10.0).children(|context| {
                            for (label, value, lo, hi) in [
                                ("Width", &mut width, 40.0_f32, 260.0_f32),
                                ("Height", &mut height, 30.0, 150.0),
                                ("Radius", &mut radius, 0.0, 48.0),
                            ] {
                                // MyGo aligns each grid cell's text to
                                // its center (`AlignSelf(ui.Center)`).
                                row(context)
                                    .gap(10.0)
                                    .align(AlignItems::Center)
                                    .children(|context| {
                                    text(context, label)
                                        .font_size(12.0)
                                        .color(theme.muted)
                                        .width(label_width);
                                    // MyGo's `Slider(...).Step(2)` snaps the
                                    // drag to twos without drawing tick marks,
                                    // so the track stays one 20px row.
                                    // MyGo's grid row is as tall as its
                                    // slider, 20 (`ui/widgets.go` slider).
                                    container(context)
                                        .width(slider_width)
                                        .height(20.0)
                                        .children(|context| {
                                            slider(
                                                context,
                                                &format!("docs-demo-popover-{label}"),
                                                value,
                                                lo,
                                                hi,
                                            );
                                            *value = (*value / 2.0).round() * 2.0;
                                        });
                                    text(context, &format!("{:.0}", *value))
                                        .font_size(12.0)
                                        .width(24.0);
                                });
                            }
                            });
                        });
                });
                if !overlay.open {
                    demo.open = false;
                }
                probe.set(10);
            }
            demo.width = width;
            demo.height = height;
            demo.radius = radius;
            // The layer the popover describes, centered in what is left:
            // MyGo's `Column(c).Grow(1).AlignSelf(Stretch).Center()`.
            column(context)
                .fill_parent()
                .align_self(AlignItems::Stretch)
                .center()
                .children(|context| {
                    column(context)
                        .size(width, height)
                        .background(theme.accent, radius)
                        .center()
                        .children(|context| {
                            text(context, &format!("{:.0} × {:.0}", width, height))
                                .font_size(12.0)
                                .color(theme.on_accent);
                        });
                });
        });
}

fn tooltip_demo(context: &mut Context, seen: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    stack(context, 360.0, 10.0, |context| {
        let mut triggers = Vec::new();
        // MyGo's demo (`website/docs-demo/dialogs.go` tooltip): Share,
        // Bookmark, Settings, and Save, each an icon button with a tip.
        row(context).gap(8.0).children(|context| {
            for (index, (icon_name, tip)) in [
                ("share", "Share with others"),
                ("bookmark", "Add a bookmark"),
                ("gear", "Settings"),
                ("save", "Save the note"),
            ]
            .iter()
            .enumerate()
            {
                let trigger = icon_button(context, icon_name);
                if let Some(bounds) = trigger.bounds {
                    triggers.push((
                        format!("docs-demo-tooltip-{index}"),
                        bounds,
                        trigger.id,
                        tip.to_string(),
                    ));
                }
            }
        });
        text(context, "Rest the pointer on a button")
            .font_size(12.0)
            .color(theme.muted);
        let mut visible = false;
        for (id, anchor, target, tip) in triggers {
            visible |= tooltip_for(context, &id, anchor, &target, &tip).visible;
        }
        if visible {
            seen.set(true);
            probe.set(10);
        }
    });
}

fn toast_demo(context: &mut Context, queue: &Rc<RefCell<ToastQueue>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    // MyGo's demo (`website/docs-demo/dialogs.go` toast) builds its controls
    // in a column that grows to fill the preview, centered across and at
    // its top, which leaves the room below for the toasts.
    column(context)
        .fill_parent()
        .gap(12.0)
        .align(AlignItems::Center)
        .children(|context| {
        row(context).gap(8.0).children(|context| {
            if button(context, "Save").clicked {
                queue.borrow_mut().push_message(context, "Saved");
            }
            if button(context, "Delete a note").clicked {
                queue.borrow_mut().push_message(context, "Note deleted");
            }
            if button(context, "Upload").clicked {
                queue.borrow_mut().push_message(context, "Upload complete");
            }
        });
        text(context, "3 notes")
            .font_size(12.0)
            .color(theme.muted);
    });
    let mut queue = queue.borrow_mut();
    let response = toasts(context, "docs-demo-toast", &mut queue);
    probe.set(response.visible as i32);
}

fn progress_demo(context: &mut Context, done: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    // MyGo's demo (`website/docs-demo/status.go` progress): a 320-wide
    // column of two bars over their labels, then the buttons that move
    // them, all in the preview's centered column.
    const TOTAL: i32 = 5;
    let uploaded = done.get();
    column(context).width(320.0).gap(8.0).children(|context| {
        text(context, &format!("Uploaded {uploaded} of {TOTAL} files"))
            .font_size(12.0)
            .color(theme.foreground);
        progress(context, "docs-demo-progress", uploaded as f32, TOTAL as f32);
        container(context)
            .margin_each(4.0, 0.0, 4.0, 0.0)
            .children(|context| {
                text(context, "Connecting to the server…")
                    .font_size(12.0)
                    .color(theme.foreground);
            });
        progress_indeterminate(context, "docs-demo-progress-connecting");
    });
    row(context).gap(8.0).children(|context| {
        if button_disabled(context, "Upload a file", uploaded == TOTAL).clicked {
            done.set((uploaded + 1).min(TOTAL));
            probe.set(done.get());
        }
        if button_disabled(context, "Reset", uploaded == 0).clicked {
            done.set(0);
            probe.set(0);
        }
    });
}

fn spinner_demo(context: &mut Context, loading: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    // MyGo's demo (`website/docs-demo/status.go` spinner): a 24-high row of
    // the spinner and its message, then the button that stops it.
    let spinning = loading.get();
    row(context).gap(8.0).height(24.0).align(AlignItems::Center).children(|context| {
        if spinning {
            spinner(context, "docs-demo-spinner");
            text(context, "Loading messages…")
                .font_size(theme.font_sizes.body)
                .color(theme.foreground);
        } else {
            text(context, "12 messages")
                .font_size(theme.font_sizes.body)
                .color(theme.foreground);
        }
    });
    let label = if spinning { "Stop" } else { "Reload" };
    if button(context, label).clicked {
        loading.set(!spinning);
        probe.set(if spinning { 1 } else { 0 });
    }
}

fn meter_demo(context: &mut Context, state: &Rc<RefCell<MeterDemoState>>, probe: &Rc<Cell<i32>>) {
    let theme = context.theme().clone();
    let mut demo = state.borrow_mut();
    // MyGo's demo (`website/docs-demo/status.go` meter): a 320-wide column
    // of two meters, each over a slider, its label and value above.
    const SIZE: f32 = 500.0;
    column(context).width(320.0).gap(6.0).children(|context| {
        row(context).children(|context| {
            text(context, "Disk").font_size(12.0);
            spacer(context);
            text(context, &format!("{:.0} of {:.0} GB", demo.disk, SIZE))
                .font_size(12.0)
                .color(theme.muted);
        });
        // Warning at 80% and critical at 95% of the disk.
        meter_levels(
            context,
            "docs-demo-meter-disk",
            demo.disk,
            0.0,
            SIZE,
            Some(0.8 * SIZE),
            Some(0.95 * SIZE),
        );
        if slider(context, "docs-demo-meter-disk-slider", &mut demo.disk, 0.0, SIZE).changed {
            probe.set(demo.disk as i32);
        }
        row(context)
            .margin_each(4.0, 0.0, 4.0, 0.0)
            .children(|context| {
                text(context, "Battery").font_size(12.0);
                spacer(context);
                text(context, &format!("{:.0}%", demo.battery))
                    .font_size(12.0)
                    .color(theme.muted);
            });
        // Warning at 20% and critical at 10%: a low charge is the danger.
        meter_levels(
            context,
            "docs-demo-meter-battery",
            demo.battery,
            0.0,
            100.0,
            Some(20.0),
            Some(10.0),
        );
        if slider(context, "docs-demo-meter-battery-slider", &mut demo.battery, 0.0, 100.0).changed
        {
            probe.set(demo.battery as i32);
        }
    });
}

fn badge_demo(context: &mut Context, unread: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    // MyGo's badge demo (`website/docs-demo/status.go`): a row 24 apart
    // holding a 200-by-130 sidebar rounded 8 on the surface, and a column 8
    // apart of a button and a status line.
    let theme = context.theme().clone();
    let mut mailbox = "drafts".to_string();
    row(context).gap(24.0).align(AlignItems::Center).children(|context| {
        container(context)
            .size(200.0, 130.0)
            .background(theme.surface, 8.0)
            .children(|context| {
                let mailboxes = [
                    SidebarItem::new("inbox", "Inbox").icon("inbox").badge("3"),
                    SidebarItem::new("drafts", "Drafts").icon("draft").badge("New").badge_accent(),
                    SidebarItem::new("sent", "Sent").icon("send"),
                ];
                let sections = [SidebarSection::new("Mailboxes", &mailboxes)];
                let response = sidebar(context, "docs-demo-badge-sidebar", &mut mailbox, &sections);
                if response.changed && mailbox == "inbox" {
                    unread.set(0);
                }
            });
        column(context).gap(8.0).align(AlignItems::FlexStart).children(|context| {
            if button(context, "Receive a message").clicked {
                unread.set(unread.get() + 1);
                probe.set(unread.get());
            }
            if unread.get() > 0 {
                text(
                    context,
                    &format!("{} unread: open the inbox to read them", unread.get()),
                )
                .font_size(12.0)
                .color(theme.muted);
            } else {
                text(context, "No unread messages")
                    .font_size(12.0)
                    .color(theme.muted);
            }
        });
    });
}

/// MyGo's chart demo state: the kind of chart the segmented control
/// chooses, and the data the Shuffle button replaces.
#[derive(Debug, Clone)]
struct ChartDemoState {
    kind: usize,
    search: Vec<f64>,
    direct: Vec<f64>,
    temps: Vec<f64>,
}

/// The meter demo's disk and battery levels, as MyGo's demo keeps them.
struct MeterDemoState {
    disk: f32,
    battery: f32,
}

impl Default for MeterDemoState {
    fn default() -> Self {
        // MyGo starts with 430 of 500 GB used and a 64% battery.
        Self {
            disk: 430.0,
            battery: 64.0,
        }
    }
}

/// The popover demo's layer dimensions, as MyGo's demo keeps them.
struct PopoverDemoState {
    open: bool,
    width: f32,
    height: f32,
    radius: f32,
}

impl Default for PopoverDemoState {
    fn default() -> Self {
        // MyGo starts the popover open on a 160 × 96 layer with a 12 radius.
        Self {
            open: true,
            width: 160.0,
            height: 96.0,
            radius: 12.0,
        }
    }
}

impl Default for ChartDemoState {
    fn default() -> Self {
        Self {
            kind: 0,
            search: vec![42.0, 58.0, 51.0, 74.0, 88.0, 69.0],
            direct: vec![18.0, 22.0, 30.0, 26.0, 35.0, 41.0],
            temps: vec![6.0, 5.0, 4.5, 4.0, 5.0, 7.5, 10.0, 12.5, 14.0, 15.0, 14.5, 12.0, 9.5],
        }
    }
}

const CHART_MONTHS: [&str; 6] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun"];
const CHART_SHARES: [f64; 4] = [48.0, 27.0, 15.0, 10.0];
const CHART_NAMES: [&str; 4] = ["macOS", "Windows", "Linux", "Browsers"];

fn chart_demo(context: &mut Context, state: &Rc<RefCell<ChartDemoState>>, probe: &Rc<Cell<i32>>) {
    // MyGo's chart demo (`website/docs-demo/charts.go`): a segmented control
    // and a Shuffle button over a 440 by 200 plot.
    let theme = context.theme().clone();
    let mut demo = state.borrow().clone();
    row(context).gap(12.0).children(|context| {
        if segmented(context, "docs-demo-chart-kind", &mut demo.kind, &["Bars", "Line", "Stacked", "Donut"]).changed {
            probe.set(demo.kind as i32);
        }
        if demo.kind < 3 && button(context, "Shuffle").clicked {
            for (index, value) in demo.search.iter_mut().enumerate() {
                *value = f64::from(20 + ((index * 37 + 11) % 80) as u32);
            }
            for (index, value) in demo.direct.iter_mut().enumerate() {
                *value = f64::from(10 + ((index * 23 + 7) % 40) as u32);
            }
            for (index, value) in demo.temps.iter_mut().enumerate() {
                let angle = index as f64 / 12.0 * std::f64::consts::PI;
                *value = 4.0 + 11.0 * angle.sin() + ((index * 13 + 5) % 30) as f64 / 10.0;
            }
            probe.set(10 + demo.kind as i32);
        }
    });
    column(context).width(440.0).height(200.0).children(|context| match demo.kind {
        0 => chart_bars(context, &theme, &demo.search),
        1 => chart_line(context, &theme, &demo.temps),
        2 => chart_stacked(context, &theme, &demo.search, &demo.direct),
        _ => chart_donut(context, &theme),
    });
    *state.borrow_mut() = demo;
}

/// The demo's axis label style: 11px muted text on one line.
fn chart_label_style(theme: &Theme) -> AxisStyle {
    AxisStyle {
        font_size: 11.0,
        color: theme.muted,
        gap_x: 8.0,
        gap_y: 6.0,
    }
}

fn chart_bars(context: &mut Context, theme: &Theme, search: &[f64]) {
    let band = Band::new(&CHART_MONTHS).padding(0.35, 0.2);
    let (_, hi) = extent(search);
    let scale = Linear::new(0.0, hi).nice(4);
    let ticks = scale.ticks(4);
    let style = chart_label_style(theme);
    let accent = theme.accent;
    plot(context, "docs-demo-chart", &ticks, &band.ticks(), style, |context, area| {
        for tick in &ticks {
            let y = area.y + (1.0 - tick.at) * area.height;
            grid_line(context,
                &format!("docs-demo-chart-grid-{}", tick.label),
                area.x,
                y,
                area.x + area.width,
                y,
                theme.border,
            );
        }
        for (index, value) in search.iter().enumerate() {
            let span = band.band(index);
            let y = scale.span(0.0, *value);
            myrust_ui::chart::rect(
                context,
                &format!("docs-demo-chart-bar-{index}"),
                span,
                y,
                accent,
                4.0,
            );
        }
    });
}

fn chart_line(context: &mut Context, theme: &Theme, temps: &[f64]) {
    let (lo, hi) = extent(temps);
    let scale = Linear::new(lo, hi).nice(4);
    let ticks = scale.ticks(4);
    let style = chart_label_style(theme);
    let accent = theme.accent;
    // MyGo's line chart spans one day from March 5, 2026 (`charts.go`
    // chartDay), its points two hours apart.
    const DAY_MS: i64 = 86_400_000;
    let start = 1_772_668_800_000_i64; // 2026-03-05T00:00:00Z
    let time = TimeScale::new(start, start + DAY_MS);
    let bottom = time.ticks(6);
    let xs: Vec<f32> = (0..temps.len())
        .map(|index| time.at(start + index as i64 * 2 * 3_600_000))
        .collect();
    let ys: Vec<f32> = temps.iter().map(|value| scale.at(*value)).collect();
    plot(context, "docs-demo-chart", &ticks, &bottom, style, |context, area| {
        for tick in &ticks {
            let y = area.y + (1.0 - tick.at) * area.height;
            grid_line(context,
                &format!("docs-demo-chart-grid-{}", tick.label),
                area.x,
                y,
                area.x + area.width,
                y,
                theme.border,
            );
        }
        let fill = area_path(area, &xs, None, &ys, Curve::Monotone);
        let stroke = line_path(area, &xs, &ys, Curve::Monotone);
        path_mark(
            context,
            "docs-demo-chart-area",
            filled_path(fill, Color::rgba(accent.r, accent.g, accent.b, 38)),
        );
        path_mark(
            context,
            "docs-demo-chart-line",
            stroked_path(stroke, 2.0, accent),
        );
    });
}

fn chart_stacked(context: &mut Context, theme: &Theme, search: &[f64], direct: &[f64]) {
    let band = Band::new(&CHART_MONTHS).padding(0.35, 0.2);
    let stacked = chart_stack(&[search.to_vec(), direct.to_vec()]);
    let hi = stacked[1]
        .iter()
        .map(|segment| segment.hi)
        .fold(0.0_f64, f64::max);
    let scale = Linear::new(0.0, hi).nice(4);
    let ticks = scale.ticks(4);
    let style = chart_label_style(theme);
    let colors = [theme.accent, Color::rgba(theme.accent.r, theme.accent.g, theme.accent.b, 115)];
    let names = ["Search", "Direct"];
    row(context).gap(14.0).margin_each(0.0, 0.0, 8.0, 0.0).children(|context| {
        for (index, name) in names.iter().enumerate() {
            row(context).gap(6.0).align(AlignItems::Center).children(|context| {
                container(context)
                    .size(10.0, 10.0)
                    .background(colors[index], 3.0);
                text(context, name).font_size(12.0).color(theme.muted);
            });
        }
    });
    // MyGo's stacked chart gives the plot the room its legend leaves
    // (`charts.go` chartStacked: the legend's row, then a plot that grows).
    let legend_height = measured_text_height(14.0) + 8.0;
    plot_sized(
        context,
        "docs-demo-chart",
        &ticks,
        &band.ticks(),
        style,
        200.0 - legend_height,
        |context, area| {
        for tick in &ticks {
            let y = area.y + (1.0 - tick.at) * area.height;
            grid_line(context,
                &format!("docs-demo-chart-grid-{}", tick.label),
                area.x,
                y,
                area.x + area.width,
                y,
                theme.border,
            );
        }
        for (series_index, series) in stacked.iter().enumerate() {
            for (index, segment) in series.iter().enumerate() {
                myrust_ui::chart::rect(
                    context,
                    &format!("docs-demo-chart-stack-{series_index}-{index}"),
                    band.band(index),
                    scale.span(segment.lo, segment.hi),
                    colors[series_index],
                    0.0,
                );
            }
        }
    });
}

fn chart_donut(context: &mut Context, theme: &Theme) {
    let slices = pie(&CHART_SHARES, 0.03);
    let colors: Vec<Color> = (0..slices.len())
        .map(|index| {
            let amount = index as f32 * 0.22;
            Color::rgb(
                (f32::from(theme.accent.r) + (f32::from(theme.background.r) - f32::from(theme.accent.r)) * amount) as u8,
                (f32::from(theme.accent.g) + (f32::from(theme.background.g) - f32::from(theme.accent.g)) * amount) as u8,
                (f32::from(theme.accent.b) + (f32::from(theme.background.b) - f32::from(theme.accent.b)) * amount) as u8,
            )
        })
        .collect();
    const INNER: f32 = 52.0;
    const OUTER: f32 = 88.0;
    row(context)
        .fill_parent()
        .gap(24.0)
        .align(AlignItems::Center)
        .justify(JustifyContent::Center)
        .children(|context| {
            let mut commands = Vec::new();
            for (index, slice) in slices.iter().enumerate() {
                let radius = OUTER - 4.0;
                commands.push(filled_path(
                    sector_path(OUTER, OUTER, INNER, radius, *slice),
                    colors[index],
                ));
            }
            container(context)
                .size(2.0 * OUTER, 2.0 * OUTER)
                .draw(commands)
                .children(|context| {
                    column(context)
                        .size(2.0 * OUTER, 2.0 * OUTER)
                        .center()
                        .children(|context| {
                            text(context, &format!("{:.0}%", CHART_SHARES[0])).font_size(22.0).bold();
                            text(context, CHART_NAMES[0]).font_size(12.0).color(theme.muted);
                        });
                });
            column(context).gap(6.0).children(|context| {
                for (index, name) in CHART_NAMES.iter().enumerate() {
                    row(context).gap(8.0).align(AlignItems::Center).children(|context| {
                        container(context)
                            .size(10.0, 10.0)
                            .background(colors[index], 3.0);
                        text(context, &format!("{name}  {:.0}%", CHART_SHARES[index])).font_size(12.0);
                    });
                }
            });
        });
}

/// MyGo's demo photo: a sunset over hills, 320×180
/// (`website/docs-demo/status.go` statusSunset).
fn demo_photo_pixels() -> Vec<u8> {
    const W: usize = 320;
    const H: usize = 180;
    // MyGo mixes 8-bit channels, rounding each as statusMix does.
    let mix = |a: (u8, u8, u8), b: (u8, u8, u8), t: f64| {
        let t = t.clamp(0.0, 1.0);
        let channel = |x: u8, y: u8| {
            (f64::from(x) + (f64::from(y) - f64::from(x)) * t + 0.5) as u8
        };
        (channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))
    };
    let top = (30, 41, 99);
    let mid = (219, 39, 119);
    let low = (251, 146, 60);
    let sun = (254, 240, 138);
    let far = (88, 28, 135);
    let near = (30, 27, 75);
    let sun_y = 112.0_f64;
    let mut pixels = Vec::with_capacity(W * H * 4);
    for y in 0..H {
        for x in 0..W {
            let fx = x as f64;
            let fy = y as f64;
            let fraction = fy / 120.0;
            let mut color = mix(top, mid, fraction * 1.6);
            if fraction > 0.6 {
                color = mix(mid, low, (fraction - 0.6) / 0.4);
            }
            let distance = ((fx - 210.0).powi(2) + (fy - sun_y).powi(2)).sqrt();
            color = mix(color, sun, 0.5 - (distance - 30.0));
            let glow = 1.0 - distance / 110.0;
            if glow > 0.0 {
                color = mix(color, sun, glow * glow * 0.35);
            }
            let far_y = 128.0 + 14.0 * (fx / 38.0).sin() + 6.0 * (fx / 13.0).sin();
            if fy > far_y {
                color = mix(color, far, fy - far_y);
            }
            let near_y = 148.0 + 12.0 * (fx / 52.0 + 2.0).sin() + 4.0 * (fx / 17.0).sin();
            if fy > near_y {
                color = mix(color, near, fy - near_y);
            }
            pixels.extend_from_slice(&[color.0, color.1, color.2, 255]);
        }
    }
    pixels
}

/// MyGo's demo logo: a ring on a diagonal gradient, 128×128
/// (`website/docs-demo/status.go` statusLogo).
/// The width of the illustration MyGo's image demo shows, in its own units.
const DEMO_ART_WIDTH: u32 = 120;
/// The height of that illustration.
const DEMO_ART_HEIGHT: u32 = 80;
/// How many samples across each pixel of the illustration, which smooths
/// its edges as MyGo's SVG rasterizer does.
const DEMO_ART_SAMPLES: u32 = 4;

/// Rasterizes MyGo's illustration (`website/docs-demo/status.go`
/// statusArt): a sky gradient behind a sun, two mountains with a snow cap,
/// and a ground band of the text color at a quarter over it, all inside a
/// card rounded 10.
fn demo_art_pixels() -> Vec<u8> {
    let width = (DEMO_ART_WIDTH * 2) as usize;
    let height = (DEMO_ART_HEIGHT * 2) as usize;
    let mut pixels = vec![0_u8; width * height * 4];
    let samples = DEMO_ART_SAMPLES * DEMO_ART_SAMPLES;
    for row in 0..height {
        for column in 0..width {
            let mut red = 0.0_f64;
            let mut green = 0.0_f64;
            let mut blue = 0.0_f64;
            let mut alpha = 0.0_f64;
            for sy in 0..DEMO_ART_SAMPLES {
                for sx in 0..DEMO_ART_SAMPLES {
                    let fx = (column as f64 + (sx as f64 + 0.5) / DEMO_ART_SAMPLES as f64) * 0.5;
                    let fy = (row as f64 + (sy as f64 + 0.5) / DEMO_ART_SAMPLES as f64) * 0.5;
                    if !inside_rounded_card(fx, fy) {
                        continue;
                    }
                    let (r, g, b, a) = demo_art_sample(fx, fy);
                    red += f64::from(r) * a;
                    green += f64::from(g) * a;
                    blue += f64::from(b) * a;
                    alpha += a;
                }
            }
            let offset = (row * width + column) * 4;
            if alpha > 0.0 {
                pixels[offset] = (red / alpha).round() as u8;
                pixels[offset + 1] = (green / alpha).round() as u8;
                pixels[offset + 2] = (blue / alpha).round() as u8;
                pixels[offset + 3] = (alpha / samples as f64 * 255.0).round() as u8;
            }
        }
    }
    pixels
}

/// Returns whether a point of the illustration falls inside its card, which
/// rounds its corners by 10 (`clipPath` of `card`).
fn inside_rounded_card(x: f64, y: f64) -> bool {
    const RADIUS: f64 = 10.0;
    let w = f64::from(DEMO_ART_WIDTH);
    let h = f64::from(DEMO_ART_HEIGHT);
    if x < 0.0 || y < 0.0 || x >= w || y >= h {
        return false;
    }
    let corner_x = if x < RADIUS {
        RADIUS
    } else if x > w - RADIUS {
        w - RADIUS
    } else {
        return true;
    };
    let corner_y = if y < RADIUS {
        RADIUS
    } else if y > h - RADIUS {
        h - RADIUS
    } else {
        return true;
    };
    let dx = x - corner_x;
    let dy = y - corner_y;
    dx * dx + dy * dy <= RADIUS * RADIUS
}

/// Returns the color of the illustration at a point, and how opaque it is:
/// the sky, the sun, the mountains and their snow, then the ground band of
/// the text color at a quarter over them.
fn demo_art_sample(x: f64, y: f64) -> (u8, u8, u8, f64) {
    // The sky gradient runs from #38BDF8 at the top to #E0F2FE at the
    // bottom.
    let t = (y / f64::from(DEMO_ART_HEIGHT)).clamp(0.0, 1.0);
    let mix = |a: (u8, u8, u8), b: (u8, u8, u8), t: f64| {
        let channel = |x: u8, y: u8| {
            (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8
        };
        (channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))
    };
    let mut color = mix((56, 189, 248), (224, 242, 254), t);
    // The sun, a circle of radius 10 at (88, 24).
    if (x - 88.0).powi(2) + (y - 24.0).powi(2) <= 100.0 {
        color = (250, 204, 21);
    }
    // The mountains: (0,62) (30,30) (52,52) (66,40) (92,62).
    if inside_polygon(x, y, &[(0.0, 62.0), (30.0, 30.0), (52.0, 52.0), (66.0, 40.0), (92.0, 62.0)]) {
        color = (22, 163, 74);
    }
    // The snow cap on the first peak: (30,30) (38,38) (30,42) (22,38).
    if inside_polygon(x, y, &[(30.0, 30.0), (38.0, 38.0), (30.0, 42.0), (22.0, 38.0)]) {
        color = (240, 253, 244);
    }
    // The ground band of the text color at a quarter, y 62 to 80.
    if y >= 62.0 {
        color = mix(color, (24, 24, 27), 0.25);
    }
    (color.0, color.1, color.2, 1.0)
}

/// Returns whether a point falls inside a polygon, by the even-odd rule.
fn inside_polygon(x: f64, y: f64, points: &[(f64, f64)]) -> bool {
    let mut inside = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (xi, yi) = points[i];
        let (xj, yj) = points[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn demo_logo_pixels() -> Vec<u8> {
    const N: usize = 128;
    let mix = |a: (u8, u8, u8), b: (u8, u8, u8), t: f64| {
        let t = t.clamp(0.0, 1.0);
        let channel = |x: u8, y: u8| {
            (f64::from(x) + (f64::from(y) - f64::from(x)) * t + 0.5) as u8
        };
        (channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))
    };
    let a = (99, 102, 241);
    let b = (236, 72, 153);
    let white = (255, 255, 255);
    let mut pixels = Vec::with_capacity(N * N * 4);
    for y in 0..N {
        for x in 0..N {
            let fx = x as f64 + 0.5;
            let fy = y as f64 + 0.5;
            let mut color = mix(a, b, (fx + fy) / (2.0 * N as f64));
            let ring = 9.0 - (((fx - 64.0).powi(2) + (fy - 64.0).powi(2)).sqrt() - 34.0).abs();
            color = mix(color, white, ring + 0.5);
            pixels.extend_from_slice(&[color.0, color.1, color.2, 255]);
        }
    }
    pixels
}

/// Draws the 96-square portrait MyGo's avatar demo shows
/// (`website/docs-demo/status.go` statusPortrait): a head and body of skin
/// over a diagonal teal-to-blue gradient.
fn demo_portrait_pixels() -> Vec<u8> {
    const N: usize = 96;
    let mix = |a: (u8, u8, u8), b: (u8, u8, u8), t: f64| {
        let t = t.clamp(0.0, 1.0);
        let channel = |x: u8, y: u8| {
            (f64::from(x) + (f64::from(y) - f64::from(x)) * t + 0.5) as u8
        };
        (channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))
    };
    let (a, b) = ((45, 212, 191), (59, 130, 246));
    let skin = (255, 237, 213);
    let mut pixels = Vec::with_capacity(N * N * 4);
    for y in 0..N {
        for x in 0..N {
            let fx = x as f64 + 0.5;
            let fy = y as f64 + 0.5;
            let mut color = mix(a, b, (fx + fy) / (2.0 * N as f64));
            let head = 20.0 - ((fx - 48.0).powi(2) + (fy - 38.0).powi(2)).sqrt();
            let body = 34.0 - (((fx - 48.0) * 0.8).powi(2) + (fy - 100.0).powi(2)).sqrt();
            color = mix(color, skin, head.max(body) + 0.5);
            pixels.extend_from_slice(&[color.0, color.1, color.2, 255]);
        }
    }
    pixels
}

fn image_demo(context: &mut Context, fit: &Rc<Cell<usize>>, gray: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    // MyGo's image demo (`website/docs-demo/status.go`): a row 20 apart of a
    // 64-square logo fit contained and rounded 12, a 200-by-130 photo on the
    // surface with the chosen fit and radius 8 (gray when the box is
    // checked), and a 120-wide illustration; then a row 12 apart of the fit
    // segments and the grayscale box.
    let theme = context.theme().clone();
    let photo = demo_photo_pixels();
    let logo = demo_logo_pixels();
    let fits = [
        ImageFit::Contain,
        ImageFit::Cover,
        ImageFit::Fill,
        ImageFit::ScaleDown,
        ImageFit::NaturalSize,
    ];
    row(context).gap(20.0).align(AlignItems::Center).children(|context| {
        image_styled(
            context,
            "docs-demo-image-logo",
            ImageData::new(&logo, 128, 128),
            64.0,
            64.0,
            ImageStyle {
                fit: ImageFit::Contain,
                radius: 12.0,
                grayscale: false,
            },
        );
        container(context)
            .size(200.0, 130.0)
            .background(theme.surface, 8.0)
            .children(|context| {
                image_styled(
                    context,
                    "docs-demo-image-photo",
                    ImageData::new(&photo, 320, 180),
                    200.0,
                    130.0,
                    ImageStyle {
                        fit: fits[fit.get().min(fits.len() - 1)],
                        radius: 8.0,
                        grayscale: gray.get(),
                    },
                );
            });
        // MyGo's third picture is an SVG illustration 120 by 80
        // (`website/docs-demo/status.go` statusArt), not a bitmap.
        let art = demo_art_pixels();
        image_styled(
            context,
            "docs-demo-image-art",
            // The raster is twice the illustration's own size, so that its
            // edges smooth at a device scale of two.
            ImageData::new(&art, DEMO_ART_WIDTH * 2, DEMO_ART_HEIGHT * 2),
            120.0,
            80.0,
            ImageStyle {
                fit: ImageFit::Contain,
                radius: 10.0,
                grayscale: false,
            },
        );
    });
    row(context).gap(12.0).align(AlignItems::Center).children(|context| {
        let mut index = fit.get();
        if segmented(
            context,
            "docs-demo-image-fit",
            &mut index,
            &["Contain", "Cover", "Fill box", "Scale down", "Natural size"],
        )
        .changed
        {
            fit.set(index);
            probe.set(index as i32);
        }
        let mut checked = gray.get();
        if checkbox_labeled(context, "docs-demo-image-gray", "Grayscale", &mut checked).changed {
            gray.set(checked);
            probe.set(100 + i32::from(checked));
        }
    });
}

fn icon_demo(context: &mut Context, _alternate: &Rc<Cell<bool>>, _probe: &Rc<Cell<i32>>) {
    // MyGo's icon demo (`website/docs-demo/status.go`): a save icon with its
    // label, four 24px icons in the status colors, then two buttons whose
    // icons sit before their labels.
    let theme = context.theme().clone();
    let danger = Color::rgb(220, 38, 38);
    let warning = Color::rgb(217, 119, 6);
    let accent = theme.accent;
    let success = Color::rgb(22, 163, 74);
    row(context).gap(6.0).align(AlignItems::Center).children(|context| {
        icon(context, "docs-demo-icon-save", "save", 14.0, theme.foreground);
        text(context, "Save");
    });
    row(context).gap(16.0).align(AlignItems::Center).children(|context| {
        icon(context, "docs-demo-icon-danger", "save", 24.0, danger);
        icon(context, "docs-demo-icon-warning", "star", 24.0, warning);
        icon(context, "docs-demo-icon-accent", "heart", 24.0, accent);
        icon(context, "docs-demo-icon-success", "bell", 24.0, success);
    });
    row(context).gap(8.0).children(|context| {
        button_primary_with_icon(context, "Save", "save");
        button_with_icon(context, "Syncing", "loader");
    });
}

fn avatar_demo(context: &mut Context, name: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    // MyGo's avatar demo (`website/docs-demo/status.go`): a row 8 apart of
    // avatars, then a row 8 apart of an avatar and the name's field.
    let mut name = name.borrow_mut();
    // The first avatar shows MyGo's portrait bitmap rather than initials.
    let portrait = demo_portrait_pixels();
    row(context).gap(8.0).align(AlignItems::Center).children(|context| {
        avatar(
            context,
            "docs-demo-avatar-katherine",
            "Katherine Johnson",
            None,
            Some(ImageData::new(&portrait, 96, 96)),
            0.0,
            AvatarShape::Circle,
        );
        for (index, person) in ["Grace Hopper", "Alan Turing", "Margaret Hamilton", "Linus"]
            .iter()
            .enumerate()
        {
            avatar(
                context,
                &format!("docs-demo-avatar-{index}"),
                person,
                None,
                None,
                0.0,
                AvatarShape::Circle,
            );
        }
    });
    row(context).gap(8.0).align(AlignItems::Center).children(|context| {
        avatar(context, "docs-demo-avatar-name", &name, None, None, 0.0, AvatarShape::Circle);
        container(context).width(200.0).children(|context| {
            text_field_placeholder(context, "docs-demo-avatar-field", &mut name, "A name");
        });
        probe.set(name.len() as i32);
    });
}

fn showcase_demo(
    context: &mut Context,
    frame: &Rc<Cell<u64>>,
    probe: &Rc<Cell<i32>>,
    toasts_state: &Rc<RefCell<ToastQueue>>,
) {
    // The landing page behind the site's Demo tab. The chart advances with
    // the frame counter so the page keeps moving while it is on screen.
    let theme = context.theme().clone();
    let tick = frame.get().wrapping_add(1);
    frame.set(tick);
    let values: Vec<f32> = (0..30)
        .map(|i| {
            let phase = (tick as f32 * 0.06 + i as f32 * 0.28).sin();
            (0.5 + 0.42 * phase).clamp(0.0, 1.0)
        })
        .collect();
    // Three equal columns inside the 28px page padding with 12px gaps. The
    // cards take an explicit width and height so the grid rows stay even and
    // the descriptions wrap inside the card instead of spilling out.
    let page_width = context.available_size().width;
    let card_width = ((page_width - 56.0 - 24.0) / 3.0).floor();
    let card_text_width = (card_width - 28.0).max(1.0);

    column(context).fill().padding(28.0).gap(26.0).children(|context| {
        // Hero
        column(context).gap(12.0).children(|context| {
            text(context, "Build native apps in Rust.").font_size(30.0).bold();
            text(
                context,
                "An immediate-mode UI engine — no HTML, no JavaScript, no WebView.",
            )
            .font_size(13.0)
            .color(theme.muted);
            row(context).gap(10.0).children(|context| {
                if button_primary(context, "Get started").clicked {
                    probe.set(probe.get() + 1);
                    toasts_state
                        .borrow_mut()
                        .push_message(context, "Opening the docs…");
                }
                if button(context, "View on GitHub").clicked {
                    probe.set(probe.get() + 1);
                    toasts_state
                        .borrow_mut()
                        .push_message(context, "Opening GitHub repository…");
                }
            });
        });

        // Feature cards
        grid(context).columns(3).gap(12.0).children(|context| {
            let features: &[(&str, &str, &str)] = &[
                (
                    "home",
                    "Native UI",
                    "Self-drawn windows: flex layout, TTF glyphs, hover, focus.",
                ),
                (
                    "code",
                    "Typed IPC",
                    "Call Rust from TypeScript with auto-generated bindings.",
                ),
                (
                    "settings",
                    "Desktop APIs",
                    "Menus, tray, clipboard, global shortcuts.",
                ),
                (
                    "star",
                    "No unsafe",
                    "myrust-core enforces #![forbid(unsafe_code)].",
                ),
                (
                    "loader",
                    "Small binary",
                    "Ships as a single stripped executable under 2 MB.",
                ),
                (
                    "terminal",
                    "Testable",
                    "NativeUi::render() runs views headlessly in CI.",
                ),
            ];
            for (idx, (icon_name, title, desc)) in features.iter().enumerate() {
                container(context)
                    .width(card_width)
                    .height(112.0)
                    .padding(14.0)
                    .background(theme.surface, 10.0)
                    .border(theme.border, 10.0)
                    .children(|context| {
                        column(context).gap(8.0).children(|context| {
                            icon(
                                context,
                                &format!("docs-demo-showcase-icon-{idx}"),
                                icon_name,
                                18.0,
                                theme.accent,
                            );
                            text(context, *title).font_size(13.0).bold();
                            text(context, *desc)
                                .font_size(11.0)
                                .color(theme.muted)
                                .max_width(card_text_width);
                        });
                    });
            }
        });

        // Metrics and the live chart
        row(context).gap(28.0).align(AlignItems::FlexEnd).children(|context| {
            column(context).gap(14.0).children(|context| {
                for (num, label) in [
                    ("1.9 MB", "Binary, stripped"),
                    ("6 MB", "Memory, idle UI"),
                    ("0%", "CPU at idle"),
                ] {
                    column(context).gap(2.0).children(|context| {
                        text(context, num).font_size(24.0).bold().color(theme.accent);
                        text(context, label).font_size(11.0).color(theme.muted);
                    });
                }
            });
            container(context).width(380.0).height(110.0).children(|context| {
                line_chart(
                    context,
                    "docs-demo-showcase-chart",
                    &values,
                    110.0,
                    Some(0.0),
                    Some(1.0),
                );
            });
        });

        // Toasts paint above the rest of the page.
        toasts(
            context,
            "docs-demo-showcase-toasts",
            &mut toasts_state.borrow_mut(),
        );
    });
}

#[no_mangle]
pub extern "C" fn demo_probe() -> i32 {
    APP.with(|cell| {
        cell.borrow()
            .as_ref()
            .map_or(-1, |app| app.probe.get())
    })
}

#[no_mangle]
pub extern "C" fn demo_selected() -> i32 {
    APP.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|app| app.selected.as_ref())
            .map_or(-1, |selected| selected.borrow().to_string().parse().unwrap_or(-1))
    })
}

#[no_mangle]
pub extern "C" fn demo_tick() -> u32 {
    APP.with(|cell| {
        cell.borrow()
            .as_ref()
            .map_or(0, |app| app.frame.get() as u32)
    })
}


#[no_mangle]
pub extern "C" fn demo_pointer(kind: u32, x: f32, y: f32) {
    APP.with(|cell| {
        let app = cell.borrow();
        let Some(app) = app.as_ref() else { return };
        let event = match kind {
            1 => myrust_ui::UiEvent::PointerPressed {
                x,
                y,
                button: myrust_ui::PointerButton::Left,
            },
            2 => myrust_ui::UiEvent::PointerReleased {
                x,
                y,
                button: myrust_ui::PointerButton::Left,
            },
            _ => myrust_ui::UiEvent::PointerMoved { x, y },
        };
        let _ = app.ui.input(event);
    });
}

#[no_mangle]
pub extern "C" fn demo_secondary(x: f32, y: f32) {
    APP.with(|cell| {
        let app = cell.borrow();
        let Some(app) = app.as_ref() else { return };
        let _ = app.ui.input(myrust_ui::UiEvent::PointerPressed {
            x,
            y,
            button: myrust_ui::PointerButton::Right,
        });
    });
}

#[no_mangle]
pub extern "C" fn demo_wheel(dx: f32, dy: f32) {
    APP.with(|cell| {
        let app = cell.borrow();
        let Some(app) = app.as_ref() else { return };
        let _ = app.ui.input(myrust_ui::UiEvent::Wheel { delta_x: dx, delta_y: dy });
    });
}

#[no_mangle]
pub extern "C" fn demo_key(code: u32) {
    let key = match code {
        1 => myrust_ui::Key::Tab,
        2 => myrust_ui::Key::Enter,
        3 => myrust_ui::Key::Space,
        4 => myrust_ui::Key::ArrowLeft,
        5 => myrust_ui::Key::ArrowRight,
        6 => myrust_ui::Key::ArrowUp,
        7 => myrust_ui::Key::ArrowDown,
        8 => myrust_ui::Key::Escape,
        9 => myrust_ui::Key::Backspace,
        10 => myrust_ui::Key::Delete,
        11 => myrust_ui::Key::Home,
        12 => myrust_ui::Key::End,
        13 => myrust_ui::Key::PageUp,
        14 => myrust_ui::Key::PageDown,
        _ => return,
    };
    APP.with(|cell| {
        let app = cell.borrow();
        let Some(app) = app.as_ref() else { return };
        let _ = app.ui.input(myrust_ui::UiEvent::Key { key });
    });
}

#[no_mangle]
pub extern "C" fn demo_text(codepoint: u32) {
    let Some(character) = char::from_u32(codepoint) else {
        return;
    };
    APP.with(|cell| {
        let app = cell.borrow();
        let Some(app) = app.as_ref() else { return };
        let _ = app.ui.input(myrust_ui::UiEvent::TextInput {
            text: character.to_string(),
        });
    });
}
