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
    checkbox, checkbox_labeled, checkbox_indeterminate, code_editor, collapsible, color_picker, combobox, column, container, context_menu,    date_input, decimal_input, dialog, diff_viewer, drag_source, drop_target, editable_text,
    find_bar, form_field, format_date, format_number, frame_stats_panel, grid, grid_view, inspector,
    link, list, menu_button, number_input, outline, overlay, popover, radio_group, range_slider,
    rating, rich_text, row, router, scroll_view, search_field, segmented, select, sidebar, slider,
    spinner, split_view, stepper, switch, tabs, table, text, text_area, text_field, text_field_password, time_input, toasts,
    token_field, toggle, toolbar, tooltip_for, transition, tree, avatar, badge, bar_chart, icon,
    image, line_chart, meter, progress, progress_indeterminate, Animation, AccordionMode, AccordionSection,
    AccordionState, AutocompleteState, AvatarShape, BreadcrumbItem, CalendarState,
    CodeEditorState, ColorPickerState, ComboboxState, ContextMenuState, DateInputState, DateStyle,
    DecimalInputOptions, DialogAction, DiffViewerState, Direction, Color, Context,
    DragState, DrawCommand, Duration, Easing, EditableTextState, FontFace,
    GridViewState, ImageData, InspectorItem, InspectorTree, ListRow, ListState, Locale, MenuState,
    NativeUi,
    NumberInputOptions, OverlayState, Page, PaintSummary, RangeSliderState, Role,
    RouterState, SidebarItem, SidebarSection, Size, SplitOrientation, SplitViewState, TableColumn,
    TableRow, TableState, Theme, ToastQueue, ToggleGroup, ToolbarItem, ToolbarState,
    Transform, TransitionState, TreeState, TreeNode,
};

thread_local! {
    static APP: RefCell<Option<DemoApp>> = const { RefCell::new(None) };
    static COMMAND_BUFFER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

static FONT: OnceLock<Option<Arc<FontFace>>> = OnceLock::new();

fn stack<F: FnOnce(&mut Context)>(context: &mut Context, width: f32, gap: f32, children: F) {
    column(context).fill().center().children(|context| {
        column(context)
            .width(width)
            .gap(gap.max(20.0))
            .center()
            .children(children);
    });
}

struct DemoApp {
    slug: u32,
    ui: NativeUi,
    frame: Rc<Cell<u64>>,
    selected: Option<Rc<RefCell<usize>>>,
    probe: Rc<Cell<i32>>,
}

impl DemoApp {
    fn new(slug: u32) -> Self {
        let frame = Rc::new(Cell::new(0_u64));
        let selected = (slug == 0).then(|| Rc::new(RefCell::new(0_usize)));
        let probe = Rc::new(Cell::new(0_i32));
        let ui = match slug {
            0 => {
                let view_selected = Rc::clone(selected.as_ref().expect("tabs selection"));
                let view_frame = Rc::clone(&frame);
                let view_probe = Rc::clone(&probe);
                let notify = Rc::new(Cell::new(true));
                let sounds = Rc::new(Cell::new(false));
                let view_notify = Rc::clone(&notify);
                let view_sounds = Rc::clone(&sounds);
                NativeUi::new(move |context| {
                    tabs_demo(
                        context,
                        &view_selected,
                        &view_notify,
                        &view_sounds,
                        &view_frame,
                        &view_probe,
                    );
                })
            }
            1 => {
                let copied = Rc::new(Cell::new(false));
                let mode = Rc::new(Cell::new(2_usize));
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
                let drag = Rc::new(RefCell::new(DragState::new()));
                let drops = Rc::new(Cell::new(0_i32));
                let view_drag = Rc::clone(&drag);
                let view_drops = Rc::clone(&drops);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    drag_demo(context, &view_drag, &view_drops, &view_probe);
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
                let target = Rc::new(Cell::new(0.5_f32));
                let view_target = Rc::clone(&target);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    motion_demo(context, &view_target, &view_probe);
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
                let view_count = Rc::clone(&count);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    button_demo(context, &view_count, &view_probe);
                })
            }
            25 => {
                let count = Rc::new(Cell::new(0_i32));
                let view_count = Rc::clone(&count);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    link_demo(context, &view_count, &view_probe);
                })
            }
            26 => {
                let state = Rc::new(RefCell::new(MenuState::default()));
                let selected = Rc::new(Cell::new(-1_i32));
                let view_state = Rc::clone(&state);
                let view_selected = Rc::clone(&selected);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    menu_demo(context, &view_state, &view_selected, &view_probe);
                })
            }
            27 => {
                let state = Rc::new(RefCell::new(ContextMenuState::default()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    context_demo(context, &view_state, &view_probe);
                })
            }
            28 => {
                let group = Rc::new(RefCell::new(ToggleGroup {
                    active: Some("docs-demo-toggle-left".to_string()),
                }));
                let view_group = Rc::clone(&group);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    toggle_demo(context, &view_group, &view_probe);
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
                let selected = Rc::new(Cell::new(-1_i32));
                let view_selected = Rc::clone(&selected);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    toolbar_demo(context, &view_selected, &view_probe);
                })
            }
            31 => {
                let values = Rc::new(RefCell::new([true, false, true, true, false, true]));
                let indeterminate = Rc::new(Cell::new(true));
                let view_values = Rc::clone(&values);
                let view_indeterminate = Rc::clone(&indeterminate);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    checkbox_demo(context, &view_values, &view_indeterminate, &view_probe);
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
                let selected = Rc::new(Cell::new(0_i32));
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
                let state = Rc::new(RefCell::new(RangeSliderState::new(20.0, 80.0)));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    range_demo(context, &view_state, &view_probe);
                })
            }
            37 => {
                let value = Rc::new(Cell::new(0_i64));
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
                let value = Rc::new(RefCell::new("Ada Lovelace".to_string()));
                let password = Rc::new(RefCell::new("hunter2".to_string()));
                let notes = Rc::new(RefCell::new("Line one\nLine two".to_string()));
                let view_value = Rc::clone(&value);
                let view_password = Rc::clone(&password);
                let view_notes = Rc::clone(&notes);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    text_input_demo(context, &view_value, &view_password, &view_notes, &view_probe)
                })
            }
            40 => {
                let value = Rc::new(RefCell::new("hello".to_string()));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| primitives_demo(context, &view_value, &view_probe))
            }
            41 => {
                let value = Rc::new(RefCell::new("42".to_string()));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| number_demo(context, &view_value, &view_probe))
            }
            42 => {
                let value = Rc::new(RefCell::new(String::new()));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| decimal_demo(context, &view_value, &view_probe))
            }
            43 => {
                let value = Rc::new(RefCell::new(String::new()));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| search_demo(context, &view_value, &view_probe))
            }
            44 => {
                let query = Rc::new(RefCell::new("a".to_string()));
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
                let text = Rc::new(RefCell::new("a".to_string()));
                let state = Rc::new(RefCell::new(AutocompleteState {
                    highlight: 0,
                    ..AutocompleteState::default()
                }));
                let view_text = Rc::clone(&text);
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| autocomplete_demo(context, &view_text, &view_state, &view_probe))
            }
            46 => {
                let tokens = Rc::new(RefCell::new(vec![
                    "Rust".to_string(),
                    "Native UI".to_string(),
                ]));
                let input = Rc::new(RefCell::new(String::new()));
                let view_tokens = Rc::clone(&tokens);
                let view_input = Rc::clone(&input);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| token_demo(context, &view_tokens, &view_input, &view_probe))
            }
            47 => {
                let text = Rc::new(RefCell::new("Rename me".to_string()));
                let state = Rc::new(RefCell::new(EditableTextState::new()));
                let view_text = Rc::clone(&text);
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| editable_demo(context, &view_text, &view_state, &view_probe))
            }
            48 => {
                 let query = Rc::new(RefCell::new("button".to_string()));
                let current = Rc::new(Cell::new(0_usize));
                let view_query = Rc::clone(&query);
                let view_current = Rc::clone(&current);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| find_demo(context, &view_query, &view_current, &view_probe))
            }
            49 => {
                let source = Rc::new(Cell::new(true));
                let view_source = Rc::clone(&source);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| rich_text_demo(context, &view_source, &view_probe))
            }
            50 => {
                let state = Rc::new(RefCell::new(CodeEditorState::default()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| code_demo(context, &view_state, &view_probe))
            }
            51 => {
                let state = Rc::new(RefCell::new(DiffViewerState::default()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| diff_demo(context, &view_state, &view_probe))
            }
            52 => {
                let state = Rc::new(RefCell::new(CalendarState::new(2024, 2, Some(10))));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| calendar_demo(context, &view_state, &view_probe))
            }
            53 => {
                let text = Rc::new(RefCell::new("2024-02-29".to_string()));
                let state = Rc::new(RefCell::new(DateInputState::default()));
                let view_text = Rc::clone(&text);
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| date_demo(context, &view_text, &view_state, &view_probe))
            }
            54 => {
                let text = Rc::new(RefCell::new("09:30".to_string()));
                let view_text = Rc::clone(&text);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| time_demo(context, &view_text, &view_probe))
            }
            55 => {
                let color = Rc::new(RefCell::new(Color::rgb(34, 211, 238)));
                let state = Rc::new(RefCell::new(ColorPickerState {
                    open: true,
                    hex: "#22D3EEFF".to_string(),
                    hue: 189,
                    saturation: 86,
                    value: 93,
                    alpha: 255,
                }));
                let view_color = Rc::clone(&color);
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| color_demo(context, &view_color, &view_state, &view_probe))
            }
            56 => {
                let state = Rc::new(RefCell::new(ListState {
                    selected_id: "r2".to_string(),
                }));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| list_demo(context, &view_state, &view_probe))
            }
            57 => {
                let state = Rc::new(RefCell::new(TableState {
                    selected_id: "budget".to_string(),
                    ..TableState::default()
                }));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| table_demo(context, &view_state, &view_probe))
            }
            58 => {
                let state = Rc::new(RefCell::new(TreeState {
                    selected_id: "button".to_string(),
                }));
                let expanded = Rc::new(RefCell::new(vec![
                    "mygo".to_string(),
                    "ui".to_string(),
                ]));
                let view_state = Rc::clone(&state);
                let view_expanded = Rc::clone(&expanded);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| tree_demo(context, &view_state, &view_expanded, &view_probe, false))
            }
            59 => {
                let state = Rc::new(RefCell::new(TreeState {
                    selected_id: "docs".to_string(),
                }));
                let expanded = Rc::new(RefCell::new(vec![
                    "mygo".to_string(),
                    "docs".to_string(),
                ]));
                let view_state = Rc::clone(&state);
                let view_expanded = Rc::clone(&expanded);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| tree_demo(context, &view_state, &view_expanded, &view_probe, true))
            }
            60 => {
                let state = Rc::new(RefCell::new(GridViewState::default()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| grid_view_demo(context, &view_state, &view_probe))
            }
            61 => {
                let scroll = Rc::new(Cell::new(0.0_f32));
                let view_scroll = Rc::clone(&scroll);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| scroll_demo(context, &view_scroll, &view_probe))
            }
            62 => {
                let columns = Rc::new(Cell::new(3_usize));
                let view_columns = Rc::clone(&columns);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| grid_demo(context, &view_columns, &view_probe))
            }
            63 => {
                let state = Rc::new(RefCell::new(SplitViewState::default()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| split_demo(context, &view_state, &view_probe))
            }
            64 => {
                let open = Rc::new(Cell::new(true));
                let view_open = Rc::clone(&open);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| collapsible_demo(context, &view_open, &view_probe))
            }
            65 => {
                let state = Rc::new(RefCell::new(AccordionState {
                    open: vec!["one".to_string()],
                    mode: AccordionMode::Single,
                }));
                let value = Rc::new(RefCell::new("Ada's Mac".to_string()));
                let notify = Rc::new(Cell::new(true));
                let view_state = Rc::clone(&state);
                let view_value = Rc::clone(&value);
                let view_notify = Rc::clone(&notify);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    accordion_demo(context, &view_state, &view_value, &view_notify, &view_probe)
                })
            }
            66 => {
                let value = Rc::new(RefCell::new(String::new()));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| form_demo(context, &view_value, &view_probe))
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
                let router_state = Rc::new(RefCell::new(RouterState::new("home")));
                let view_router = Rc::clone(&router_state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| history_demo(context, &view_router, &view_probe))
            }
            70 => {
                let open = Rc::new(Cell::new(false));
                let result = Rc::new(Cell::new(0_i32));
                let view_open = Rc::clone(&open);
                let view_result = Rc::clone(&result);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| dialog_demo(context, &view_open, &view_result, &view_probe))
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
                let open = Rc::new(RefCell::new(OverlayState::new(false)));
                let view_open = Rc::clone(&open);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| popover_demo(context, &view_open, &view_probe))
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
                let value = Rc::new(Cell::new(20.0_f32));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| progress_demo(context, &view_value, &view_probe))
            }
            76 => {
                let spinning = Rc::new(Cell::new(true));
                let view_spinning = Rc::clone(&spinning);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| spinner_demo(context, &view_spinning, &view_probe))
            }
            77 => {
                let value = Rc::new(Cell::new(20.0_f32));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| meter_demo(context, &view_value, &view_probe))
            }
            78 => {
                let count = Rc::new(Cell::new(0_i32));
                let view_count = Rc::clone(&count);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| badge_demo(context, &view_count, &view_probe))
            }
            79 => {
                let line = Rc::new(Cell::new(false));
                let view_line = Rc::clone(&line);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| chart_demo(context, &view_line, &view_probe))
            }
            80 => {
                let color = Rc::new(Cell::new(false));
                let view_color = Rc::clone(&color);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| image_demo(context, &view_color, &view_probe))
            }
            81 => {
                let name = Rc::new(Cell::new(false));
                let view_name = Rc::clone(&name);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| icon_demo(context, &view_name, &view_probe))
            }
            82 => {
                let initials = Rc::new(Cell::new(false));
                let view_initials = Rc::clone(&initials);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| avatar_demo(context, &view_initials, &view_probe))
            }
            _ => {
                let value = Rc::new(Cell::new(20.0_f32));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| progress_demo(context, &view_value, &view_probe))
            }
        }
        .with_theme(Theme::dark());
        Self {
            slug,
            ui,
            frame,
            selected,
            probe,
        }
    }
}

fn tabs_demo(
    context: &mut Context,
    selected: &Rc<RefCell<usize>>,
    notify: &Rc<Cell<bool>>,
    sounds: &Rc<Cell<bool>>,
    _frame: &Rc<Cell<u64>>,
    probe: &Rc<Cell<i32>>,
) {
    context.set_widgets_disabled(DEMO_DISABLED.load(Ordering::Relaxed));
    column(context).fill().padding(16.0).gap(8.0).children(|context| {
        let mut item = selected.borrow_mut();
        let state = tabs(
            context,
            "docs-demo-tabs",
            &mut item,
            &["General", "Appearance", "Advanced"],
        );
        if *item == 0 {
            let mut checked = notify.get();
            if checkbox_labeled(context, "docs-demo-tab-notify", "Show notifications", &mut checked).changed {
                notify.set(checked);
            }
            let mut checked = sounds.get();
            if checkbox_labeled(context, "docs-demo-tab-sounds", "Play sounds", &mut checked).changed {
                sounds.set(checked);
            }
        }
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
    drag: &Rc<RefCell<DragState>>,
    drops: &Rc<Cell<i32>>,
    probe: &Rc<Cell<i32>>,
) {
    column(context)
        .fill()
        .padding(18.0)
        .gap(12.0)
        .children(|context| {
            text(context, "Drag and drop")
                .font_size(22.0)
                .bold()
                .color(Color::rgb(241, 245, 249));
            row(context).gap(10.0).children(|context| {
                let mut state = drag.borrow_mut();
                drag_source(context, "docs-demo-drag", "Drag me", "card-42", &mut state);
                let target = drop_target(context, "docs-demo-drop", "Drop here", &mut state);
                if let Some(drop) = target.dropped {
                    drops.set(drops.get() + 1);
                    probe.set(drops.get());
                    let _ = drop;
                }
            });
            text(context, &format!("drops = {}", drops.get()))
                .font_size(12.0)
                .color(Color::rgb(34, 211, 238));
        });
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

const MARKDOWN: &str = r#"# MyRust Markdown

A small, predictable Markdown renderer for native UI.

## Supported

- headings from level 1 to 6
- paragraphs with inline code, bold, italic and links
- unordered and ordered lists
- fenced code blocks with an optional language label
- pipe tables with a header row

## Cost

| Block | Cost |
| --- | --- |
| heading | O(1) |
| paragraph | O(n) |

```rust
let blocks = parse_markdown(source);
for block in &blocks {
    render(context, block);
}
```

## Accessibility

The Markdown root is a Group labelled by its stable id.

Each inline span is a Label whose value is plain, bold, italic, code or link.
"#;

fn markdown_demo(
    context: &mut Context,
    frame: &Rc<Cell<u64>>,
    copied: &Rc<Cell<bool>>,
    mode: &Rc<Cell<usize>>,
    probe: &Rc<Cell<i32>>,
) {
    let tick = frame.get();
    frame.set(tick.wrapping_add(1));
    let lines: Vec<&str> = MARKDOWN.lines().collect();
    let mut selected = mode.get();
    let reveal = ((tick as usize)
        .saturating_mul([1_usize, 2, 8, 12][selected.min(3)]))
    .max(1)
    .min(lines.len());
    if reveal < lines.len() {
        context.request_animation_frame();
    }
    let body = lines[..reveal].join("\n");
    let height = (context.available_size().height - 88.0).max(80.0);
    column(context).fill().padding(16.0).gap(8.0).children(|context| {
        row(context).gap(8.0).children(|context| {
            if button(context, "Stream it").clicked {
                frame.set(0);
                copied.set(false);
                probe.set(1);
            }
            container(context).width(280.0).children(|context| {
                let response = segmented(context, "docs-demo-md-mode", &mut selected, &["Word", "Sentence", "Line", "Paragraph"]);
                if response.changed {
                    mode.set(selected);
                    frame.set(0);
                    probe.set(selected as i32 + 2);
                }
            });
            text(context, if reveal == lines.len() { "Markdown · complete" } else { "Markdown · streaming" })
                .font_size(12.0)
                .color(Color::rgb(161, 161, 169));
            if button(context, if copied.get() { "Copied!" } else { "Copy" }).clicked {
                copied.set(true);
                probe.set(10);
            }
        });
        scroll_view(context, "docs-demo-markdown", height, |context| {
            context.markdown("docs-demo-markdown-body", &body);
        });
        text(context, "MyRust native Markdown demo · line-by-line stream")
            .font_size(11.0)
            .color(Color::rgb(161, 161, 169));
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

fn text_width(font: Option<&FontFace>, text: &str, font_size: f32) -> f32 {
    let font_size = font_size.max(1.0);
    match font {
        Some(font) => text
            .chars()
            .map(|character| font.glyph_advance(character, font_size))
            .sum(),
        None => text.chars().count() as f32 * font_size * 0.6,
    }
}

fn text_line_height(font: Option<&FontFace>, font_size: f32) -> f32 {
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
                buffer.push(7);
                for value in [x, y, width, height] {
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
            DrawCommand::Line { x1, y1, x2, y2, width, color } => {
                buffer.push(3);
                for value in [*x1, *y1, *x2, *y2, *width] { buffer.extend_from_slice(&value.to_le_bytes()); }
                push_color(&mut buffer, *color);
            }
            DrawCommand::Text { x, y, text, font_size, weight, color, max_width, alignment } => {
                let mut draw_x = *x;
                if let Some(max_width) = *max_width {
                    if max_width.is_finite() && max_width > 0.0 {
                        let widest = text
                            .split('\n')
                            .map(|line| text_width(font, line, *font_size))
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
                let line_height = text_line_height(font, *font_size);
                for (index, line) in text.split('\n').enumerate() {
                    buffer.push(4);
                    for value in [draw_x, *y + index as f32 * line_height, *font_size] {
                        buffer.extend_from_slice(&value.to_le_bytes());
                    }
                    buffer.extend_from_slice(&weight.to_le_bytes());
                    push_color(&mut buffer, *color);
                    buffer.extend_from_slice(&max_width.unwrap_or(f32::NAN).to_le_bytes());
                    buffer.push(match alignment {
                        myrust_ui::TextAlignment::Start => 0,
                        myrust_ui::TextAlignment::Center => 1,
                        myrust_ui::TextAlignment::End => 2,
                    });
                    let bytes = line.as_bytes();
                    buffer.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                    buffer.extend_from_slice(bytes);
                }
            }
            DrawCommand::SvgPath { x, y, width, height, path, fill, stroke, stroke_width } => {
                buffer.push(5);
                for value in [*x, *y, *width, *height] { buffer.extend_from_slice(&value.to_le_bytes()); }
                match fill {
                    Some(color) => { buffer.push(1); push_color(&mut buffer, *color); }
                    None => buffer.push(0),
                }
                match stroke {
                    Some(color) => { buffer.push(1); push_color(&mut buffer, *color); }
                    None => buffer.push(0),
                }
                buffer.extend_from_slice(&stroke_width.to_le_bytes());
                let bytes = path.as_bytes();
                buffer.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                buffer.extend_from_slice(bytes);
            }
            DrawCommand::LinearGradient { x, y, width, height, start, end, stops } => {
                buffer.push(6);
                for value in [*x, *y, *width, *height, start.x, start.y, end.x, end.y] {
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
    moved: &Rc<Cell<bool>>,
    state: &Rc<RefCell<TransitionState>>,
    probe: &Rc<Cell<i32>>,
) {
    let is_moved = moved.get();
    let mut transition_state = state.borrow_mut();
    let target = if is_moved {
        myrust_ui::Rect::new(90.0, 10.0, 70.0, 40.0)
    } else {
        myrust_ui::Rect::new(10.0, 10.0, 70.0, 40.0)
    };
    let frame = transition(context, &mut transition_state, target);
    stack(context, 320.0, 10.0, |context| {
        if button(context, "Move box").clicked {
            moved.set(!is_moved);
            probe.set(if is_moved { 0 } else { 1 });
        }
        container(context)
            .size(180.0, 60.0)
            .draw([
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: 180.0,
                    height: 60.0,
                    radius: myrust_ui::Radius::all(10.0),
                    color: Color::rgb(24, 24, 27),
                },
                DrawCommand::RoundedRect {
                    x: frame.bounds.x,
                    y: frame.bounds.y,
                    width: frame.bounds.width,
                    height: frame.bounds.height,
                    radius: myrust_ui::Radius::all(8.0),
                    color: Color::rgb(34, 211, 238),
                },
            ]);
        text(context, &format!("eased progress {:.2}", frame.progress))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn motion_demo(context: &mut Context, target: &Rc<Cell<f32>>, probe: &Rc<Cell<i32>>) {
    stack(context, 320.0, 10.0, |context| {
        if button(context, "Animate value").clicked {
            let next = if target.get() > 0.5 { 0.0 } else { 1.0 };
            target.set(next);
            probe.set(if next > 0.5 { 1 } else { 0 });
        }
        let value = context.animate_with(
            "docs-demo-motion",
            target.get(),
            Animation::new(Duration::from_millis(300), Easing::EaseInOutCubic),
        );
        container(context).size(240.0, 80.0).draw([
            DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: 240.0,
                height: 80.0,
                radius: myrust_ui::Radius::all(10.0),
                color: Color::rgb(15, 23, 42),
            },
            DrawCommand::RoundedRect {
                x: 12.0 + value.clamp(0.0, 1.0) * 160.0,
                y: 12.0,
                width: 56.0,
                height: 56.0,
                radius: myrust_ui::Radius::all(8.0),
                color: Color::rgb(34, 211, 238),
            },
        ]);
        text(context, &format!("animated value {:.2}", value))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn transforms_demo(context: &mut Context, rotated: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    stack(context, 400.0, 10.0, |context| {
        let is_rotated = rotated.get();
        if button(context, "Toggle transform").clicked {
            rotated.set(!is_rotated);
            probe.set(if is_rotated { 0 } else { 1 });
        }
        context.transform(
            Transform {
                translation: myrust_ui::Point::new(if is_rotated { 20.0 } else { 0.0 }, 4.0),
                scale: myrust_ui::Point::new(1.0, 1.0),
                rotation: if is_rotated { 0.18 } else { 0.0 },
            },
            |context| {
                column(context)
                    .size(300.0, 120.0)
                    .center()
                    .draw([DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: 300.0,
                        height: 120.0,
                        radius: myrust_ui::Radius::all(12.0),
                        color: Color::rgb(30, 41, 59),
                    }])
                    .children(|context| {
                        text(context, if is_rotated { "Back" } else { "Front" })
                            .font_size(18.0)
                            .bold()
                            .color(Color::rgb(244, 244, 245));
                    });
            },
        );
        text(
            context,
            if is_rotated {
                "transform: translated 20px, rotated 0.18 rad"
            } else {
                "transform: identity"
            },
        )
        .font_size(12.0)
        .color(Color::rgb(148, 163, 184));
    });
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


fn button_demo(context: &mut Context, count: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    context.set_widgets_disabled(DEMO_DISABLED.load(Ordering::Relaxed));
    stack(context, 360.0, 16.0, |context| {
        row(context).gap(8.0).children(|context| {
            if button(context, "Cancel").clicked {
                count.set(0);
                probe.set(0);
            }
            if button_primary_with_icon(context, "Save", "check").clicked {
                count.set(count.get() + 1);
                probe.set(count.get());
            }
            if button(context, "Clear").clicked {
                count.set(0);
                probe.set(0);
            }
        });
        text(context, &format!("Saved {} time(s)", count.get()))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn link_demo(context: &mut Context, count: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    stack(context, 260.0, 10.0, |context| {
        if link(context, "docs-demo-link", "Read the docs").clicked {
            count.set(count.get() + 1);
            probe.set(count.get());
        }
        text(context, &format!("activated {} time(s)", count.get()))
            .font_size(13.0)
            .color(Color::rgb(34, 211, 238));
    });
}

fn menu_demo(
    context: &mut Context,
    menu: &Rc<RefCell<MenuState>>,
    selected: &Rc<Cell<i32>>,
    probe: &Rc<Cell<i32>>,
) {
    stack(context, 280.0, 10.0, |context| {
        let mut state = menu.borrow_mut();
        let mut response = None;
        row(context).gap(8.0).children(|context| {
            response = Some(menu_button(context, "docs-demo-menu", "Sort by", &mut state, &["Name", "Date", "Size"]));
            let _ = button(context, "…");
        });
        if let Some(index) = response.and_then(|response| response.selected) {
            selected.set(index as i32);
            probe.set(index as i32 + 1);
        }
        if selected.get() >= 0 {
            let name = ["Name", "Date", "Size"][selected.get().clamp(0, 2) as usize];
            text(context, &format!("Sorted by {}", name))
                .font_size(12.0)
                .color(Color::rgb(148, 163, 184));
        } else {
            text(context, "Choose a sort order; the trailing button is overflow")
                .font_size(12.0)
                .color(Color::rgb(148, 163, 184));
        }
    });
}

fn context_demo(
    context: &mut Context,
    menu: &Rc<RefCell<ContextMenuState>>,
    probe: &Rc<Cell<i32>>,
) {
    column(context).fill().padding(18.0).gap(10.0).children(|context| {
        text(context, "Context menu")
            .font_size(20.0)
            .bold()
            .color(Color::rgb(241, 245, 249));
        container(context)
            .size(300.0, 80.0)
            .padding(12.0)
            .draw([DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: 300.0,
                height: 80.0,
                radius: myrust_ui::Radius::all(8.0),
                color: Color::rgb(30, 41, 59),
            }])
            .children(|context| {
                text(context, "Right-click anywhere in this area")
                    .font_size(12.0)
                    .color(Color::rgb(148, 163, 184));
            });
        let mut state = menu.borrow_mut();
        let area = myrust_ui::Rect::new(18.0, 54.0, 300.0, 80.0);
        let response = context_menu(context, "docs-demo-context", &mut state, area, &["Inspect", "Duplicate", "Delete"]);
        if response.opened {
            probe.set(10);
        }
        if response.selected.is_some() {
            probe.set(response.selected.unwrap_or(0) as i32 + 11);
        }
    });
}

fn toggle_demo(context: &mut Context, group: &Rc<RefCell<ToggleGroup>>, probe: &Rc<Cell<i32>>) {
    stack(context, 320.0, 10.0, |context| {
        let mut group = group.borrow_mut();
        row(context).gap(8.0).children(|context| {
            let mut left = group.active.as_deref() == Some("docs-demo-toggle-left");
            let mut right = group.active.as_deref() == Some("docs-demo-toggle-right");
            let mut underline = group.active.as_deref() == Some("docs-demo-toggle-underline");
            let left_state = toggle(context, "docs-demo-toggle-left", "Bold", &mut left, Some(&mut group));
            let right_state = toggle(context, "docs-demo-toggle-right", "Italic", &mut right, Some(&mut group));
            let underline_state = toggle(context, "docs-demo-toggle-underline", "Underline", &mut underline, Some(&mut group));
            if left_state.changed || right_state.changed || underline_state.changed {
                probe.set(if group.active.is_some() { 1 } else { 0 });
            }
        });
        text(context, &format!("group = {:?}; one toggle stays active", group.active))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn segmented_demo(context: &mut Context, selected: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    stack(context, 340.0, 10.0, |context| {
        let mut value = selected.get().max(0) as usize;
        let response = segmented(context, "docs-demo-segmented", &mut value, &["One", "Two", "Three"]);
        if response.changed {
            selected.set(value as i32);
            probe.set(value as i32 + 1);
        }
        text(context, &format!("selected = {}", value))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn toolbar_demo(context: &mut Context, selected: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    stack(context, 360.0, 10.0, |context| {
        let items = [
            ToolbarItem::new("cut", "Cut", 90.0),
            ToolbarItem::new("copy", "Copy", 90.0),
            ToolbarItem::new("paste", "Paste", 90.0),
        ];
        let mut state = ToolbarState::new();
        let response = toolbar(context, "docs-demo-toolbar", &mut state, &items, 330.0, |context, index| {
            button(context, items[index].label);
        });
        if response.overflow_open {
            probe.set(10);
        }
        if let Some(index) = response.selected {
            selected.set(index as i32);
            probe.set(index as i32 + 1);
        }
        container(context)
            .width(360.0)
            .padding(12.0)
            .draw([DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: 360.0,
                height: 92.0,
                radius: myrust_ui::Radius::all(8.0),
                color: Color::rgb(17, 24, 39),
            }])
            .children(|context| {
                column(context).gap(6.0).children(|context| {
                    text(context, "Chapter 1: the fox jumps over the lazy dog.")
                        .font_size(14.0)
                        .color(Color::rgb(226, 232, 240));
                    if link(context, "docs-demo-toolbar-link", "Next chapter").clicked {
                        probe.set(99);
                    }
                });
            });
        text(context, &format!("selection = {}", selected.get()))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}


fn checkbox_demo(
    context: &mut Context,
    values: &Rc<RefCell<[bool; 6]>>,
    indeterminate: &Rc<Cell<bool>>,
    probe: &Rc<Cell<i32>>,
) {
    context.set_widgets_disabled(DEMO_DISABLED.load(Ordering::Relaxed));
    stack(context, 340.0, 6.0, |context| {
        let mut values = values.borrow_mut();
        for (index, label) in [
            "Sync across devices",
            "Show hidden files",
            "Notifications",
            "Mail",
            "Calendar",
            "Messages",
        ]
        .iter()
        .enumerate()
        {
            let id = format!("docs-demo-checkbox-{index}");
            let mut checked = values[index];
            let mut changed = false;
            if index >= 3 {
                row(context).gap(0.0).children(|context| {
                    container(context).size(24.0, 1.0);
                    container(context).width(316.0).children(|context| {
                        changed = checkbox_labeled(context, &id, label, &mut checked).changed;
                    });
                });
            } else if index == 2 {
                changed = checkbox_indeterminate(
                    context,
                    &id,
                    label,
                    &mut checked,
                    indeterminate.get(),
                ).changed;
                if changed {
                    indeterminate.set(false);
                }
            } else {
                changed = checkbox_labeled(context, &id, label, &mut checked).changed;
            }
            if changed {
                values[index] = checked;
                probe.set(index as i32 + 1);
            }
        }
        let count = values.iter().filter(|checked| **checked).count();
        text(context, &format!("{count} of 6 checked · each box is caller-owned"))
            .font_size(11.0)
            .color(Color::rgb(161, 161, 169));
    });
}

fn switch_demo(
    context: &mut Context,
    wifi: &Rc<Cell<bool>>,
    bluetooth: &Rc<Cell<bool>>,
    probe: &Rc<Cell<i32>>,
) {
    context.set_widgets_disabled(DEMO_DISABLED.load(Ordering::Relaxed));
    column(context).fill().center().children(|context| {
        column(context).width(280.0).gap(8.0).children(|context| {
            row(context).gap(8.0).children(|context| {
                container(context).width(220.0).children(|context| {
                    text(context, "Wi-Fi").font_size(13.0).color(Color::rgb(244, 244, 245));
                });
                let mut on = wifi.get();
                if switch(context, "docs-demo-switch", &mut on).changed {
                    wifi.set(on);
                    probe.set(if on { 1 } else { 0 });
                }
            });
            row(context).gap(8.0).children(|context| {
                container(context).width(220.0).children(|context| {
                    text(context, "Bluetooth").font_size(13.0).color(Color::rgb(244, 244, 245));
                });
                let mut on = bluetooth.get();
                if switch(context, "docs-demo-switch-bluetooth", &mut on).changed {
                    bluetooth.set(on);
                    probe.set(if on { 3 } else { 2 });
                }
            });
            text(
                context,
                &format!(
                    "Wi-Fi {}, Bluetooth {}",
                    if wifi.get() { "on" } else { "off" },
                    if bluetooth.get() { "on" } else { "off" }
                ),
            )
            .font_size(11.0)
            .color(Color::rgb(161, 161, 169));
        });
    });
}

fn radio_demo(
    context: &mut Context,
    selected: &Rc<Cell<i32>>,
    align: &Rc<Cell<i32>>,
    probe: &Rc<Cell<i32>>,
) {
    stack(context, 460.0, 10.0, |context| {
        row(context).gap(28.0).children(|context| {
            container(context).width(200.0).children(|context| {
                column(context).gap(6.0).children(|context| {
                    text(context, "Size")
                        .font_size(12.0)
                        .color(Color::rgb(148, 163, 184));
                    let mut value = selected.get().max(0) as usize;
                    let response = radio_group(context, "docs-demo-radio", &mut value, &["Small", "Medium", "Large"]);
                    if response.changed {
                        selected.set(value as i32);
                        probe.set(value as i32 + 1);
                    }
                });
            });
            container(context).width(200.0).children(|context| {
                column(context).gap(6.0).children(|context| {
                    text(context, "Alignment")
                        .font_size(12.0)
                        .color(Color::rgb(148, 163, 184));
                    let mut value = align.get().max(0) as usize;
                    let response = radio_group(context, "docs-demo-radio-align", &mut value, &["Left", "Center", "Right"]);
                    if response.changed {
                        align.set(value as i32);
                        probe.set(value as i32 + 10);
                    }
                });
            });
        });
        text(
            context,
            &format!(
                "{}, aligned {}",
                ["small", "medium", "large"][selected.get().clamp(0, 2) as usize],
                ["left", "center", "right"][align.get().clamp(0, 2) as usize]
            ),
        )
        .font_size(12.0)
        .color(Color::rgb(148, 163, 184));
    });
}

fn select_demo(context: &mut Context, selected: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().center().children(|context| {
        column(context).width(140.0).gap(4.0).children(|context| {
            text(context, "Plan").font_size(13.0).color(Color::rgb(244, 244, 245));
            let mut value = selected.get().max(0) as usize;
            let response = select(
                context,
                "docs-demo-select",
                &mut value,
                &["Basic", "Pro", "Enterprise"],
            );
            if response.open {
                probe.set(10);
            }
            if response.changed {
                selected.set(value as i32);
                probe.set(value as i32 + 1);
            }
            let (name, price) = match value {
                0 => ("Basic", "$4 a month"),
                1 => ("Pro", "$12 a month"),
                _ => ("Enterprise", "$29 a month"),
            };
            text(context, &format!("{name}: {price}"))
                .font_size(11.0)
                .color(Color::rgb(161, 161, 169));
        });
    });
}

fn slider_demo(
    context: &mut Context,
    volume: &Rc<Cell<f32>>,
    quality: &Rc<Cell<f32>>,
    probe: &Rc<Cell<i32>>,
) {
    context.set_widgets_disabled(DEMO_DISABLED.load(Ordering::Relaxed));
    column(context).fill().center().children(|context| {
        column(context).width(340.0).gap(5.0).children(|context| {
            column(context).width(340.0).gap(0.0).children(|context| {
                text(context, &format!("Volume: {:.0}", volume.get()))
                    .font_size(13.0)
                    .color(Color::rgb(161, 161, 169));
                let mut current = volume.get();
                let state = slider(context, "docs-demo-slider", &mut current, 0.0, 100.0);
                if state.changed {
                    volume.set(current);
                    probe.set((volume.get() + quality.get()) as i32);
                }
                let ticks = [0.0_f32, 48.0, 97.0, 145.0, 194.0, 242.0, 291.0, 340.0];
                container(context).size(340.0, 6.0).draw(
                    ticks.iter().map(|x| DrawCommand::Line {
                        x1: *x, y1: 0.0, x2: *x, y2: 4.0, width: 1.0,
                        color: Color::rgb(63, 63, 69),
                    }).collect::<Vec<_>>(),
                );
            });
            column(context).width(340.0).gap(0.0).children(|context| {
                text(context, &format!("Quality: {:.0}%", quality.get()))
                    .font_size(13.0)
                    .color(Color::rgb(161, 161, 169));
                let mut current = quality.get();
                let state = slider(context, "docs-demo-slider-quality", &mut current, 0.0, 100.0);
                if state.changed {
                    quality.set(current);
                    probe.set((volume.get() + quality.get()) as i32);
                }
                let ticks = [0.0_f32, 48.0, 97.0, 145.0, 194.0, 242.0, 291.0, 340.0];
                container(context).size(340.0, 6.0).draw(
                    ticks.iter().map(|x| DrawCommand::Line {
                        x1: *x, y1: 0.0, x2: *x, y2: 4.0, width: 1.0,
                        color: Color::rgb(63, 63, 69),
                    }).collect::<Vec<_>>(),
                );
            });
        });
    });
}

fn range_demo(context: &mut Context, state: &Rc<RefCell<RangeSliderState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().center().children(|context| {
        column(context).width(320.0).gap(8.0).children(|context| {
            let mut range = state.borrow_mut();
            let response = range_slider(context, "docs-demo-range", &mut range, 0.0, 100.0);
            if response.changed {
                probe.set(range.low.round() as i32 + range.high.round() as i32);
            }
            text(context, &format!("{:.0}..{:.0}", range.low, range.high))
                .font_size(12.0)
                .color(Color::rgb(34, 211, 238));
        });
    });
}

fn stepper_demo(context: &mut Context, value: &Rc<Cell<i64>>, probe: &Rc<Cell<i32>>) {
    stack(context, 240.0, 10.0, |context| {
        let mut current = value.get();
        let response = stepper(context, "docs-demo-stepper", &mut current, 0, 10, 1);
        if response.changed {
            value.set(current);
            probe.set(current as i32);
        }
        text(context, &format!("value {current}"))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn rating_demo(context: &mut Context, value: &Rc<Cell<u32>>, probe: &Rc<Cell<i32>>) {
    stack(context, 260.0, 10.0, |context| {
        let mut current = value.get();
        let response = rating(context, "docs-demo-rating", &mut current, 5);
        if response.changed {
            value.set(current);
            probe.set(current as i32);
        }
        text(context, &format!("{current} of 5 stars"))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}


fn text_input_demo(
    context: &mut Context,
    value: &Rc<RefCell<String>>,
    password: &Rc<RefCell<String>>,
    notes: &Rc<RefCell<String>>,
    probe: &Rc<Cell<i32>>,
) {
    column(context).fill().center().children(|context| {
        column(context).width(320.0).gap(5.0).children(|context| {
            let mut name = value.borrow_mut();
            if text_field(context, "docs-demo-text-name", &mut name).changed {
                probe.set(name.len() as i32);
            }
            let mut secret = password.borrow_mut();
            if text_field_password(context, "docs-demo-text-password", &mut secret).changed {
                probe.set(secret.len() as i32);
            }
            let mut note = notes.borrow_mut();
            if text_area(context, "docs-demo-text-notes", &mut note, 3).changed {
                probe.set(note.lines().count() as i32);
            }
            text(
                context,
                "Enter inserts a line in Notes · password is masked",
            )
            .font_size(11.0)
            .color(Color::rgb(161, 161, 169));
        });
    });
}

fn primitives_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    stack(context, 360.0, 8.0, |context| {
        let mut text_value = value.borrow_mut();
        let field = text_field(context, "docs-demo-primitive", &mut text_value);
        let _ = field;
        if button(context, "Select first two").clicked {
            context.set_text_input_selection("docs-demo-primitive", 0, 2);
            probe.set(1);
        }
        text(context, "TextInputState exposes cursor, selection, bounds and cursor point.")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn number_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    stack(context, 320.0, 8.0, |context| {
        text(context, "Number").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut text_value = value.borrow_mut();
        let state = number_input(context, "docs-demo-number", &mut text_value, NumberInputOptions::default().allow_negative(true));
        if state.changed {
            probe.set(state.value.unwrap_or(0.0).round() as i32);
        }
    });
}

fn decimal_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    stack(context, 320.0, 8.0, |context| {
        text(context, "Decimal").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut text_value = value.borrow_mut();
        let state = decimal_input(context, "docs-demo-decimal", &mut text_value, DecimalInputOptions::default());
        if state.changed {
            probe.set(state.units.unwrap_or(0) as i32);
        }
    });
}

fn search_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    stack(context, 360.0, 8.0, |context| {
        text(context, "Search").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut query = value.borrow_mut();
        let state = search_field(context, "docs-demo-search", &mut query);
        if state.changed {
            probe.set(query.len() as i32);
        }
        container(context)
            .width(360.0)
            .padding(10.0)
            .draw([DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: 360.0,
                height: 84.0,
                radius: myrust_ui::Radius::all(8.0),
                color: Color::rgb(17, 24, 39),
            }])
            .children(|context| {
                column(context).gap(6.0).children(|context| {
                    for (name, path) in [
                        ("button", "src/widgets/button.rs"),
                        ("layout", "src/layout.rs"),
                        ("input", "src/event.rs"),
                    ] {
                        row(context).gap(8.0).children(|context| {
                            text(context, name)
                                .font_size(13.0)
                                .color(Color::rgb(34, 211, 238));
                            text(context, path)
                                .font_size(12.0)
                                .color(Color::rgb(161, 161, 169));
                        });
                    }
                });
            });
    });
}

fn combobox_demo(
    context: &mut Context,
    query: &Rc<RefCell<String>>,
    state: &Rc<RefCell<ComboboxState>>,
    probe: &Rc<Cell<i32>>,
) {
    stack(context, 360.0, 8.0, |context| {
        text(context, "Combobox").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut query = query.borrow_mut();
        let mut state = state.borrow_mut();
        let response = combobox(context, "docs-demo-combo", &mut query, &mut state, &["Apple", "Apricot", "Banana"]);
        if response.open {
            probe.set(10);
        }
        if let Some(index) = response.selected {
            probe.set(index as i32 + 1);
        }
    });
}

fn autocomplete_demo(
    context: &mut Context,
    value: &Rc<RefCell<String>>,
    state: &Rc<RefCell<AutocompleteState>>,
    probe: &Rc<Cell<i32>>,
) {
    stack(context, 360.0, 8.0, |context| {
        text(context, "Autocomplete").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut value = value.borrow_mut();
        let mut state = state.borrow_mut();
        let response = autocomplete(context, "docs-demo-auto", &mut value, &mut state, &["Apple", "Apricot", "Banana"]);
        if response.visible {
            probe.set(10);
        }
        if let Some(index) = response.selected {
            probe.set(index as i32 + 1);
        }
    });
}

fn token_demo(
    context: &mut Context,
    tokens: &Rc<RefCell<Vec<String>>>,
    input: &Rc<RefCell<String>>,
    probe: &Rc<Cell<i32>>,
) {
    stack(context, 420.0, 10.0, |context| {
        text(context, "Tokens").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut tokens = tokens.borrow_mut();
        let mut input = input.borrow_mut();
        let state = token_field(context, "docs-demo-token", &mut tokens, &mut input);
        if state.added || state.removed {
            probe.set(tokens.len() as i32);
        }
        text(context, &format!("{} token(s); Enter adds, Backspace removes", tokens.len()))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn editable_demo(
    context: &mut Context,
    value: &Rc<RefCell<String>>,
    state: &Rc<RefCell<EditableTextState>>,
    probe: &Rc<Cell<i32>>,
) {
    stack(context, 380.0, 10.0, |context| {
        text(context, "Editable text").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut value = value.borrow_mut();
        let mut state = state.borrow_mut();
        let response = editable_text(context, "docs-demo-edit", &mut state, &mut value);
        if response.editing || response.submitted {
            probe.set(1);
        }
        text(context, &format!("value = {:?}", *value))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn find_demo(
    context: &mut Context,
    query: &Rc<RefCell<String>>,
    current: &Rc<Cell<usize>>,
    probe: &Rc<Cell<i32>>,
) {
    stack(context, 440.0, 10.0, |context| {
        text(context, "Find bar").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut query = query.borrow_mut();
        let mut index = current.get();
        let state = find_bar(context, "docs-demo-find", &mut query, &mut index, 5);
        current.set(index);
        if state.changed || state.next_clicked || state.previous_clicked {
            probe.set(index as i32 + 1);
        }
        text(context, &format!("match {} of 5; Enter or arrows move", index + 1))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
        container(context)
            .width(440.0)
            .padding(12.0)
            .draw([DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: 440.0,
                height: 64.0,
                radius: myrust_ui::Radius::all(8.0),
                color: Color::rgb(17, 24, 39),
            }])
            .children(|context| {
                text(
                    context,
                    "The quick brown fox jumps over the lazy dog. The dog sleeps in the sun; the fox runs into the woods.",
                )
                .font_size(14.0)
                .color(Color::rgb(226, 232, 240))
                .max_width(400.0);
            });
    });
}

fn rich_text_demo(context: &mut Context, source: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    stack(context, 540.0, 10.0, |context| {
        text(context, "Rich text (read-only)").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        if button(context, "Toggle emphasis").clicked {
            source.set(!source.get());
            probe.set(if source.get() { 1 } else { 0 });
        }
        let body: &str = if source.get() {
            "A paragraph with **bold**, *italic*, `inline code` and a [link](https://example.com).\n\nRich text is composed into spans; every span becomes a Label with its own role.\n\nSwitch the button to swap the source string."
        } else {
            "Plain paragraph with *italic* emphasis.\n\nNo bold, code or link in this variant.\n\nThe renderer parses inline Markdown only; block layout stays caller-owned."
        };
        rich_text(context, "docs-demo-rich", body);
        text(context, "read-only; inline Markdown subset")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn code_demo(context: &mut Context, state: &Rc<RefCell<CodeEditorState>>, probe: &Rc<Cell<i32>>) {
    stack(context, 560.0, 10.0, |context| {
        text(context, "Code editor (read-only)").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let code = "use myrust_ui::{button, column, text};\n\nfn main() {\n    NativeUi::new(|context| {\n        column(context).padding(16.0).gap(8.0).children(|context| {\n            text(context, \"Hello, MyRust\").font_size(18.0);\n            if button(context, \"Run\").clicked {\n                println!(\"running\");\n            }\n        });\n    });\n}";
        let mut state = state.borrow_mut();
        let response = code_editor(context, "docs-demo-code", code, &mut state);
        if let Some(line) = response.clicked_line {
            probe.set(line as i32 + 1);
        }
        text(context, "Click a line to select it.")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn diff_demo(context: &mut Context, state: &Rc<RefCell<DiffViewerState>>, probe: &Rc<Cell<i32>>) {
    stack(context, 580.0, 10.0, |context| {
        text(context, "Diff viewer").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut state = state.borrow_mut();
        let before = "fn render(c: &mut Context) {\n    let width = 320.0;\n    let visible = false;\n    text(c, \"old label\");\n    if visible { button(c, \"Run\"); }\n}";
        let after = "fn render(c: &mut Context) {\n    let width = 420.0;\n    let visible = true;\n    text(c, \"new label\");\n    if visible { button(c, \"Run\"); }\n}";
        let response = diff_viewer(context, "docs-demo-diff", before, after, &mut state);
        probe.set(if response.layout == myrust_ui::DiffLayout::SideBySide { 1 } else { 0 });
        text(context, "added and removed lines are aligned side by side")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn month_name(month: u32) -> &'static str {
    [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ][(month.saturating_sub(1) as usize).min(11)]
}

fn calendar_demo(context: &mut Context, state: &Rc<RefCell<CalendarState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().center().children(|context| {
        column(context).width(320.0).gap(8.0).children(|context| {
            let mut state = state.borrow_mut();
            let response = calendar(context, "docs-demo-calendar", &mut state);
            if let Some(day) = response.selected {
                probe.set(day as i32);
            }
            let selected = state.selected.unwrap_or(1);
            text(
                context,
                &format!("Selected {} {}, {}", month_name(state.month), selected, state.year),
            )
            .font_size(13.0)
            .color(Color::rgb(148, 163, 184));
        });
    });
}

fn date_demo(
    context: &mut Context,
    value: &Rc<RefCell<String>>,
    state: &Rc<RefCell<DateInputState>>,
    probe: &Rc<Cell<i32>>,
) {
    stack(context, 320.0, 10.0, |context| {
        text(context, "Date input").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut value = value.borrow_mut();
        let mut state = state.borrow_mut();
        let response = date_input(context, "docs-demo-date", &mut value, &mut state);
        if response.open {
            probe.set(2);
        } else if response.valid {
            probe.set(1);
        }
        text(context, &format!("ISO value = {:?}", *value))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn time_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    stack(context, 320.0, 10.0, |context| {
        text(context, "Time input").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut value = value.borrow_mut();
        let response = time_input(context, "docs-demo-time", &mut value);
        if response.changed {
            probe.set(response.hour as i32 * 60 + response.minute as i32);
        }
        text(context, &format!("HH:MM = {:?}", *value))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn color_demo(
    context: &mut Context,
    color: &Rc<RefCell<Color>>,
    state: &Rc<RefCell<ColorPickerState>>,
    probe: &Rc<Cell<i32>>,
) {
    column(context).fill().center().children(|context| {
        row(context).width(520.0).height(420.0).gap(16.0).children(|context| {
            column(context).width(252.0).height(414.0).gap(8.0).children(|context| {
                let mut color = color.borrow_mut();
                let mut state = state.borrow_mut();
                let response = color_picker(context, "docs-demo-color", &mut color, &mut state);
                if response.open {
                    probe.set(10);
                }
                if let Some(index) = response.preset {
                    probe.set(index as i32 + 1);
                }
            });
            let color = *color.borrow();
            container(context)
                .width(240.0)
                .height(180.0)
                .padding(16.0)
                .draw([
                    DrawCommand::RoundedRect {
                        x: 0.0, y: 0.0, width: 240.0, height: 180.0,
                        radius: myrust_ui::Radius::all(8.0), color: Color::rgb(63, 63, 69),
                    },
                    DrawCommand::RoundedRect {
                        x: 1.0, y: 1.0, width: 238.0, height: 178.0,
                        radius: myrust_ui::Radius::all(7.0), color: Color::rgb(39, 39, 42),
                    },
                    DrawCommand::RoundedRect {
                        x: 16.0, y: 48.0, width: 208.0, height: 68.0,
                        radius: myrust_ui::Radius::all(6.0),
                        color: Color::rgba(color.r, color.g, color.b, 56),
                    },
                ])
                .children(|context| {
                    column(context).width(208.0).gap(4.0).children(|context| {
                        text(context, "Tint").font_size(13.0).color(Color::rgb(244, 244, 245));
                        text(context, "Hello").font_size(20.0).bold().color(color);
                        text(context, "Tint on background")
                            .font_size(13.0)
                            .color(Color::rgb(161, 161, 169));
                        text(context, &color.to_hex())
                            .font_size(13.0)
                            .color(Color::rgb(161, 161, 169));
                    });
                });
        });
    });
}

fn list_demo(context: &mut Context, state: &Rc<RefCell<ListState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().center().children(|context| {
        column(context).width(720.0).gap(6.0).children(|context| {
            container(context)
                .width(720.0)
                .height(224.0)
                .draw([
                    DrawCommand::RoundedRect {
                        x: 0.0, y: 0.0, width: 720.0, height: 224.0,
                        radius: myrust_ui::Radius::all(10.0), color: Color::rgb(63, 63, 69),
                    },
                    DrawCommand::RoundedRect {
                        x: 1.0, y: 1.0, width: 718.0, height: 222.0,
                        radius: myrust_ui::Radius::all(9.0), color: Color::rgb(39, 39, 42),
                    },
                ])
                .children(|context| {
                    let rows = [
                        ListRow::new("r0", "Annual Report.pdf").icon("folder").detail("170 KB"),
                        ListRow::new("r1", "Beach.jpg").icon("folder").detail("1.1 MB"),
                        ListRow::new("r2", "Budget 2026.xlsx").icon("folder").detail("22 KB"),
                        ListRow::new("r3", "Cover Letter.docx").icon("folder").detail("959 KB"),
                        ListRow::new("r4", "Design Review.key").icon("folder").detail("810 KB"),
                        ListRow::new("r5", "Expenses.csv").icon("folder").detail("662 KB"),
                        ListRow::new("r6", "Family.jpg").icon("folder").detail("514 KB"),
                    ];
                    let mut state = state.borrow_mut();
                    let response = list(context, "docs-demo-list", &mut state, &rows, 28.0, 196.0);
                    if let Some(id) = response.clicked_id {
                        probe.set(id.trim_start_matches('r').parse().unwrap_or(0) + 1);
                    }
                });
            text(context, "7 files · Restore or delete")
                .font_size(11.0)
                .color(Color::rgb(161, 161, 169));
            row(context).width(720.0).gap(8.0).children(|context| {
                if button(context, "Restore").clicked {
                    probe.set(98);
                }
                if button(context, "Delete").clicked {
                    probe.set(99);
                }
            });
        });
    });
}

fn table_demo(context: &mut Context, state: &Rc<RefCell<TableState>>, probe: &Rc<Cell<i32>>) {
    context.set_widgets_disabled(DEMO_DISABLED.load(Ordering::Relaxed));
    column(context).fill().center().children(|context| {
        column(context).width(720.0).gap(6.0).children(|context| {
            let columns = [
                TableColumn::new("Name", 340.0),
                TableColumn::new("Kind", 240.0),
                TableColumn::new("Size", 140.0),
            ];
            let rows = [
                TableRow::new("annual", &["Annual Report.pdf", "PDF document", "170 KB"]).icon("folder"),
                TableRow::new("beach", &["Beach.jpg", "JPEG image", "1.1 MB"]).icon("folder"),
                TableRow::new("budget", &["Budget 2026.xlsx", "Spreadsheet", "22 KB"]).icon("folder"),
                TableRow::new("cover", &["Cover Letter.docx", "Word document", "959 KB"]).icon("folder"),
                TableRow::new("design", &["Design Review.key", "Keynote", "810 KB"]).icon("folder"),
                TableRow::new("expenses", &["Expenses.csv", "CSV text", "662 KB"]).icon("folder"),
                TableRow::new("notes", &["Meeting Notes.md", "Markdown", "12 KB"]).icon("folder"),
            ];
            let mut state = state.borrow_mut();
            let response = table(context, "docs-demo-table", &mut state, &columns, &rows);
            if response.sort.is_some() {
                probe.set(1);
            }
            text(context, "Click a header to sort; click a row to select.")
                .font_size(12.0)
                .color(Color::rgb(148, 163, 184));
        });
    });
}

fn tree_demo(
    context: &mut Context,
    state: &Rc<RefCell<TreeState>>,
    expanded: &Rc<RefCell<Vec<String>>>,
    probe: &Rc<Cell<i32>>,
    outline_mode: bool,
) {
    column(context).fill().center().children(|context| {
        let mut state = state.borrow_mut();
        let mut expanded = expanded.borrow_mut();
        let open_mygo = expanded.iter().any(|id| id == "mygo");
        let open_ui = expanded.iter().any(|id| id == "ui");
        let open_docs = expanded.iter().any(|id| id == "docs");
        let mut nodes = vec![TreeNode::new("mygo", "mygo", 0, true)];
        if open_mygo {
            nodes.push(TreeNode::new("ui", "ui", 1, true));
            if open_ui {
                nodes.push(TreeNode::new("button", "button.go", 2, false));
                nodes.push(TreeNode::new("list", "list.go", 2, false));
                nodes.push(TreeNode::new("table", "table.go", 2, false));
            }
            nodes.push(TreeNode::new("docs", "docs", 1, true));
            if open_docs {
                nodes.push(TreeNode::new("guide", "guide.md", 2, false));
            }
        }
        nodes.push(TreeNode::new("app", "app.go", 0, false));
        nodes.push(TreeNode::new("readme", "README.md", 0, false));
        let response = if outline_mode {
            outline(
                context,
                "docs-demo-outline",
                &mut state,
                &nodes,
                &mut expanded,
            )
        } else {
            let height = nodes.len() as f32 * 26.0 + 20.0;
            let mut captured = None;
            container(context)
                .width(720.0)
                .padding(10.0)
                .draw([DrawCommand::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 720.0,
                    height,
                    color: Color::rgb(39, 39, 42),
                }])
                .children(|context| {
                    captured = Some(tree(
                        context,
                        "docs-demo-tree",
                        &mut state,
                        &nodes,
                        &mut expanded,
                    ));
                });
            captured.expect("tree response")
        };
        if response.changed || response.toggled_id.is_some() {
            probe.set(1);
        }
    });
}

fn grid_view_demo(context: &mut Context, state: &Rc<RefCell<GridViewState>>, probe: &Rc<Cell<i32>>) {
    stack(context, 620.0, 10.0, |context| {
        let mut state = state.borrow_mut();
        let response = grid_view(
            context,
            "docs-demo-grid-view",
            &mut state,
            8,
            Size::new(190.0, 84.0),
            3,
            10.0,
            280.0,
            |context, index| {
                column(context)
                    .fill()
                    .padding(14.0)
                    .gap(6.0)
                    .draw([
                        DrawCommand::RoundedRect {
                            x: 4.0,
                            y: 4.0,
                            width: 182.0,
                            height: 84.0,
                            radius: myrust_ui::Radius::all(8.0),
                            color: Color::rgb(30, 41, 59),
                        },
                        DrawCommand::Circle {
                            x: 174.0,
                            y: 18.0,
                            radius: 7.0,
                            color: Color::rgb(34, 211, 238),
                        },
                    ])
                    .children(|context| {
                        text(context, &format!("Item {index}"))
                            .font_size(13.0)
                            .bold()
                            .color(Color::rgb(244, 244, 245));
                        text(context, "grid cell")
                            .font_size(11.0)
                            .color(Color::rgb(161, 161, 169));
                    });
            },
        );
        if let Some(index) = response.selected {
            probe.set(index as i32 + 1);
        }
        text(context, &format!("selected = {:?}", response.selected))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn scroll_demo(context: &mut Context, scroll: &Rc<Cell<f32>>, probe: &Rc<Cell<i32>>) {
    stack(context, 520.0, 10.0, |context| {
        let state = scroll_view(context, "docs-demo-scroll", 200.0, |context| {
            for index in 0..8 {
                container(context)
                    .width(460.0)
                    .height(34.0)
                    .padding(8.0)
                    .draw([DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: 460.0,
                        height: 34.0,
                        radius: myrust_ui::Radius::all(6.0),
                        color: if index % 2 == 0 {
                            Color::rgb(30, 41, 59)
                        } else {
                            Color::rgb(17, 24, 39)
                        },
                    }])
                    .children(|context| {
                        text(context, &format!("Scroll row {index}"))
                            .font_size(14.0)
                            .color(Color::rgb(226, 232, 240));
                    });
            }
        });
        scroll.set(state.scroll);
        probe.set(state.scroll as i32);
        text(context, &format!("offset {:.0}px; scroll the list", state.scroll))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn grid_demo(context: &mut Context, columns: &Rc<Cell<usize>>, probe: &Rc<Cell<i32>>) {
    stack(context, 620.0, 10.0, |context| {
        if button(context, "Toggle columns").clicked {
            columns.set(if columns.get() == 3 { 2 } else { 3 });
            probe.set(columns.get() as i32);
        }
        grid(context).columns(columns.get()).gap(10.0).children(|context| {
            for index in 0..6 {
                column(context)
                    .size(190.0, 56.0)
                    .padding(10.0)
                    .gap(2.0)
                    .draw([DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: 190.0,
                        height: 56.0,
                        radius: myrust_ui::Radius::all(6.0),
                        color: Color::rgb(30, 41, 59),
                    }])
                    .children(|context| {
                        text(context, &format!("Cell {index}"))
                            .font_size(12.0)
                            .color(Color::rgb(226, 232, 240));
                        text(context, "widget")
                            .font_size(10.0)
                            .color(Color::rgb(161, 161, 169));
                    });
            }
        });
        text(context, &format!("{} columns", columns.get()))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn split_demo(context: &mut Context, state: &Rc<RefCell<SplitViewState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().center().children(|context| {
        column(context).width(620.0).gap(10.0).children(|context| {
            if button(context, "Toggle split").clicked {
                let mut state = state.borrow_mut();
                state.ratio = if state.ratio < 0.6 { 0.7 } else { 0.3 };
                probe.set((state.ratio * 10.0) as i32);
            }
            container(context).width(620.0).height(180.0).children(|context| {
                let mut state = state.borrow_mut();
                split_view(
                    context,
                    "docs-demo-split",
                    &mut state,
                    SplitOrientation::Horizontal,
                    |context| {
                        container(context)
                            .fill()
                            .draw([DrawCommand::Rect {
                                x: 0.0,
                                y: 0.0,
                                width: 620.0,
                                height: 180.0,
                                color: Color::rgb(24, 24, 27),
                            }])
                            .children(|context| {
                                column(context).fill().padding(14.0).gap(4.0).children(|context| {
                                    text(context, "Navigation")
                                        .font_size(13.0)
                                        .bold()
                                        .color(Color::rgb(244, 244, 245));
                                    text(context, "Overview\nProjects\nActivity")
                                        .font_size(12.0)
                                        .color(Color::rgb(161, 161, 169));
                                });
                            });
                    },
                    |context| {
                        container(context)
                            .fill()
                            .draw([DrawCommand::Rect {
                                x: 0.0,
                                y: 0.0,
                                width: 620.0,
                                height: 180.0,
                                color: Color::rgb(39, 39, 42),
                            }])
                            .children(|context| {
                                column(context).fill().padding(14.0).gap(4.0).children(|context| {
                                    text(context, "Content")
                                        .font_size(13.0)
                                        .bold()
                                        .color(Color::rgb(244, 244, 245));
                                    text(context, "The divider between panes is draggable.")
                                        .font_size(12.0)
                                        .color(Color::rgb(161, 161, 169));
                                });
                            });
                    },
                );
            });
        });
    });
}
fn collapsible_demo(context: &mut Context, open: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    stack(context, 520.0, 8.0, |context| {
        let mut value = open.get();
        let state = collapsible(context, "docs-demo-collapsible", &mut value, "Details", |context| {
            text(
                context,
                "The header toggles this body; the open flag is caller-owned.",
            )
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
            text(
                context,
                "Chevron, label and body are separate nodes; Space and Enter toggle.",
            )
            .font_size(12.0)
            .color(Color::rgb(100, 116, 139));
        });
        if state.changed {
            open.set(value);
            probe.set(if value { 1 } else { 0 });
        }
    });
}

fn accordion_demo(
    context: &mut Context,
    state: &Rc<RefCell<AccordionState>>,
    value: &Rc<RefCell<String>>,
    notify: &Rc<Cell<bool>>,
    probe: &Rc<Cell<i32>>,
) {
    column(context).fill().center().children(|context| {
        column(context).width(420.0).gap(8.0).children(|context| {
            let mut state = state.borrow_mut();
            let mut changed = false;
            let mut open = state.open.iter().any(|id| id == "one");
            let header = "General";
            let response = accordion(
                context,
                "docs-demo-accordion",
                &mut state,
                &[AccordionSection::new("one", header, "Device name")],
            );
            changed |= response.changed;
            if state.open.iter().any(|id| id == "one") {
                open = true;
            }
            if open {
                let mut name = value.borrow_mut();
                if text_field(context, "docs-demo-accordion-name", &mut name).changed {
                    probe.set(name.len() as i32);
                }
                let mut checked = notify.get();
                if checkbox_labeled(
                    context,
                    "docs-demo-accordion-sync",
                    "Sync settings",
                    &mut checked,
                )
                .changed
                {
                    notify.set(checked);
                }
            }
            let rest = [
                AccordionSection::new("two", "Privacy", "Local-only data."),
                AccordionSection::new("three", "Advanced", "Diagnostics."),
            ];
            let response = accordion(context, "docs-demo-accordion-rest", &mut state, &rest);
            changed |= response.changed;
            if changed {
                probe.set(state.open.len() as i32 + 1);
            }
        });
    });
}

fn form_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    stack(context, 420.0, 10.0, |context| {
        form_field(
            context,
            "docs-demo-field",
            "Project name",
            Some("Shown in the window title and recent files."),
            None,
            |context| {
                let mut text = value.borrow_mut();
                if text_field(context, "docs-demo-form-input", &mut text).changed {
                    probe.set(text.len() as i32);
                }
            },
        );
        row(context).gap(8.0).children(|context| {
            if button(context, "Save").clicked {
                probe.set(1);
            }
            if button(context, "Cancel").clicked {
                probe.set(2);
            }
        });
    });
}

fn sidebar_demo(context: &mut Context, selected: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    stack(context, 720.0, 10.0, |context| {
        row(context).width(720.0).height(210.0).gap(0.0).children(|context| {
            container(context)
                .width(220.0)
                .height(210.0)
                .draw([DrawCommand::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 220.0,
                    height: 210.0,
                    color: Color::rgb(31, 31, 35),
                }])
                .children(|context| {
                let workspace = [
                    SidebarItem::new("a", "Overview").icon("home"),
                    SidebarItem::new("b", "Projects").icon("folder").badge("3"),
                    SidebarItem::new("c", "Activity").icon("search"),
                ];
                let settings = [
                    SidebarItem::new("d", "Preferences").icon("settings"),
                    SidebarItem::new("e", "Members").icon("home"),
                ];
                let sections = [
                    SidebarSection::new("Workspace", &workspace),
                    SidebarSection::new("Settings", &settings),
                ];
                let mut selected = selected.borrow_mut();
                let response = sidebar(context, "docs-demo-sidebar", &mut selected, &sections);
                if response.changed || response.clicked_id.is_some() {
                    probe.set(selected.len() as i32);
                }
            });
            container(context)
                .width(500.0)
                .height(210.0)
                .padding(16.0)
                .draw([DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: 500.0,
                    height: 210.0,
                    radius: myrust_ui::Radius::all(8.0),
                    color: Color::rgb(39, 39, 42),
                }])
                .children(|context| {
                    column(context).gap(8.0).children(|context| {
                        text(context, "Projects")
                            .font_size(16.0)
                            .bold()
                            .color(Color::rgb(244, 244, 245));
                        text(context, "3 active projects")
                            .font_size(12.0)
                            .color(Color::rgb(161, 161, 169));
                        if button(context, "Mark all as read").clicked {
                            probe.set(99);
                        }
                        text(context, &format!("selected: {}", selected.borrow()))
                            .font_size(12.0)
                            .color(Color::rgb(34, 211, 238));
                    });
                });
        });
        text(context, "Sections are caller-owned; one item stays selected.")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
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
        text(context, "/Users/ada/Documents · click any crumb or button")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn history_demo(context: &mut Context, router_state: &Rc<RefCell<RouterState>>, probe: &Rc<Cell<i32>>) {
    stack(context, 320.0, 10.0, |context| {
        text(context, "History").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut state = router_state.borrow_mut();
        let buttons = back_forward_buttons(context, "docs-demo-history", &mut state);
        if button(context, "Visit page").clicked {
            state.push("next");
        }
        if buttons.back_clicked || buttons.forward_clicked {
            probe.set(state.history().len() as i32 + 10);
        } else {
            probe.set(state.history().len() as i32);
        }
        text(context, &format!("page: {} · {} entries", state.current(), state.history().len()))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn dialog_demo(
    context: &mut Context,
    open: &Rc<Cell<bool>>,
    result: &Rc<Cell<i32>>,
    probe: &Rc<Cell<i32>>,
) {
    stack(context, 320.0, 10.0, |context| {
        let mut is_open = open.get();
        if button(context, "Open dialog").clicked {
            is_open = true;
        }
        text(context, "A modal with a confirm action.")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
        let response = dialog(context, "docs-demo-dialog", &mut is_open, 320.0, |context| {
            text(context, "Dialog body").font_size(14.0);
            if button(context, "Confirm").clicked {
                DialogAction::Confirm
            } else {
                DialogAction::None
            }
        });
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
    });
}

fn alert_demo(
    context: &mut Context,
    open: &Rc<Cell<bool>>,
    result: &Rc<Cell<i32>>,
    probe: &Rc<Cell<i32>>,
) {
    stack(context, 320.0, 10.0, |context| {
        let mut is_open = open.get();
        if button(context, "Delete…").clicked {
            is_open = true;
        }
        text(context, "A blocking confirmation with two actions.")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
        let response = alert_dialog(
            context,
            "docs-demo-alert",
            &mut is_open,
            "Delete?",
            "This cannot be undone.",
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
    });
}

fn popover_demo(context: &mut Context, open: &Rc<RefCell<OverlayState>>, probe: &Rc<Cell<i32>>) {
    stack(context, 320.0, 10.0, |context| {
        let trigger = button(context, "Open popover");
        text(context, "Anchored to the trigger, dismissed on outside click.")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
        let mut state = open.borrow_mut();
        if trigger.clicked {
            state.open = true;
        }
        if state.open {
            let anchor = trigger
                .bounds
                .unwrap_or_else(|| myrust_ui::Rect::new(18.0, 18.0, 160.0, 30.0));
            popover(context, "docs-demo-popover", &mut state, anchor, |context| {
                text(context, "Popover content").font_size(13.0);
                if button(context, "Close popover").clicked {
                    // caller can close via state after this frame
                }
            });
            probe.set(10);
        }
    });
}

fn tooltip_demo(context: &mut Context, seen: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    stack(context, 360.0, 10.0, |context| {
        let mut triggers = Vec::new();
        row(context).gap(8.0).children(|context| {
            for (index, (label, tip)) in [
                ("", "Share this page"),
                ("", "Save to reading list"),
                ("", "Open settings"),
                ("", "Print this page"),
            ]
            .iter()
            .enumerate()
            {
                let _ = label;
                let trigger = icon_button(context, ["folder", "check", "settings", "search"][index]);
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
        text(
            context,
            "Rest the pointer on a button, or focus one with Tab.",
        )
        .font_size(12.0)
        .color(Color::rgb(148, 163, 184));
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
    stack(context, 480.0, 10.0, |context| {
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
        text(context, "3 notes").font_size(12.0).color(Color::rgb(148, 163, 184));
        let mut queue = queue.borrow_mut();
        let response = toasts(context, "docs-demo-toast", &mut queue);
        probe.set(response.visible as i32);
    });
}

fn progress_demo(context: &mut Context, value: &Rc<Cell<f32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().center().children(|context| {
        column(context).width(340.0).gap(6.0).children(|context| {
            text(context, &format!("Uploaded {} of 5 files", (value.get() / 20.0) as i32 + 1))
                .font_size(14.0)
                .color(Color::rgb(244, 244, 245));
            progress(context, "docs-demo-progress", value.get(), 100.0);
            text(context, "Connecting to the server...")
                .font_size(11.0)
                .color(Color::rgb(161, 161, 169));
            progress_indeterminate(context, "docs-demo-progress-connecting");
            if button(context, "Advance").clicked {
                let next = (value.get() + 25.0).min(100.0);
                value.set(next);
                probe.set(next as i32);
            }
            text(context, &format!("{:.0}% · caller-owned progress state", value.get()))
                .font_size(11.0)
                .color(Color::rgb(161, 161, 169));
        });
    });
}

fn spinner_demo(context: &mut Context, spinning: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().center().gap(10.0).children(|context| {
        if spinning.get() {
            spinner(context, "docs-demo-spinner");
        }
        if button(context, if spinning.get() { "Stop spinner" } else { "Start spinner" }).clicked {
            spinning.set(!spinning.get());
            probe.set(if spinning.get() { 1 } else { 0 });
        }
        text(context, if spinning.get() { "busy" } else { "idle" })
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn meter_demo(context: &mut Context, value: &Rc<Cell<f32>>, probe: &Rc<Cell<i32>>) {
    stack(context, 340.0, 10.0, |context| {
        if button(context, "Change level").clicked {
            let next = if value.get() < 50.0 { 75.0 } else if value.get() < 90.0 { 95.0 } else { 20.0 };
            value.set(next);
            probe.set(next as i32);
        }
        text(context, "CPU").font_size(12.0).color(Color::rgb(148, 163, 184));
        meter(context, "docs-demo-meter", value.get(), 0.0, 100.0, 33.0, 66.0);
        text(context, "Disk").font_size(12.0).color(Color::rgb(148, 163, 184));
        meter(
            context,
            "docs-demo-meter-disk",
            (100.0 - value.get()).max(0.0),
            0.0,
            100.0,
            33.0,
            66.0,
        );
        text(context, &format!("CPU {:.0}% · Disk {:.0}%", value.get(), 100.0 - value.get()))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn badge_demo(context: &mut Context, count: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    stack(context, 260.0, 10.0, |context| {
        if button(context, "Add badge").clicked {
            count.set(count.get() + 1);
            probe.set(count.get());
        }
        row(context).gap(8.0).children(|context| {
            badge(context, &format!("{}", count.get()));
            badge(context, "New");
            badge(context, "Beta");
        });
        text(context, "Short status pills.")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn chart_demo(context: &mut Context, line: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().center().children(|context| {
        column(context).width(620.0).gap(8.0).children(|context| {
            if button(context, "Toggle chart").clicked {
                line.set(!line.get());
                probe.set(if line.get() { 1 } else { 0 });
            }
            let values = [0.2, 0.8, 0.4, 0.9, 0.6, 0.75, 0.35, 0.85, 0.5];
            if line.get() {
                line_chart(context, "docs-demo-chart", &values, 220.0, Some(0.0), Some(1.0));
            } else {
                bar_chart(context, "docs-demo-chart", &values, 220.0, Some(0.0), Some(1.0));
            }
            text(context, "Nine samples; toggle the mark type.")
                .font_size(12.0)
                .color(Color::rgb(148, 163, 184));
        });
    });
}

fn image_demo(context: &mut Context, color: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().center().gap(10.0).children(|context| {
        if button(context, "Toggle palette").clicked {
            color.set(!color.get());
            probe.set(if color.get() { 1 } else { 0 });
        }
        let mut pixels = Vec::with_capacity(16 * 16 * 4);
        for y in 0..16u32 {
            for x in 0..16u32 {
                let warm = color.get();
                let r = if warm { 220 + x * 2 } else { 34 + x * 4 };
                let g = if warm { 80 + y * 8 } else { 120 + y * 6 };
                let b = if warm { 80 + y * 4 } else { 200 - x * 5 };
                pixels.extend_from_slice(&[r as u8, g as u8, b as u8, 255]);
            }
        }
        let data = ImageData::new(&pixels, 16, 16);
        row(context).gap(14.0).children(|context| {
            image(context, "docs-demo-image", data, 220.0, 140.0);
            column(context).gap(10.0).children(|context| {
                image(context, "docs-demo-image-small", ImageData::new(&pixels, 16, 16), 96.0, 96.0);
                image(context, "docs-demo-image-wide", ImageData::new(&pixels, 16, 16), 140.0, 34.0);
            });
        });
        text(context, "Caller-decoded RGBA8; the same pixels at three sizes.")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn icon_demo(context: &mut Context, alternate: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    stack(context, 260.0, 10.0, |context| {
        if button(context, "Toggle icon").clicked {
            alternate.set(!alternate.get());
            probe.set(if alternate.get() { 1 } else { 0 });
        }
        let name = if alternate.get() { "check" } else { "arrow_right" };
        row(context).gap(12.0).children(|context| {
            icon(context, "docs-demo-icon", name, 32.0, Color::rgb(34, 211, 238));
            icon(context, "docs-demo-icon-alt", "check", 32.0, Color::rgb(148, 163, 184));
        });
        text(context, name)
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn avatar_demo(context: &mut Context, alternate: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    stack(context, 360.0, 10.0, |context| {
        if button(context, "Toggle initials").clicked {
            alternate.set(!alternate.get());
            probe.set(if alternate.get() { 1 } else { 0 });
        }
        let (label, initials) = if alternate.get() {
            ("Ada Lovelace", "AL")
        } else {
            ("Grace Hopper", "GH")
        };
        row(context).gap(10.0).children(|context| {
            avatar(context, "docs-demo-avatar", label, initials, 44.0, AvatarShape::Circle);
            avatar(context, "docs-demo-avatar-2", "Margaret Hamilton", "MH", 44.0, AvatarShape::Circle);
            avatar(context, "docs-demo-avatar-3", "Alan Turing", "AT", 44.0, AvatarShape::Circle);
            avatar(context, "docs-demo-avatar-4", "Barbara Liskov", "BL", 44.0, AvatarShape::Circle);
            avatar(context, "docs-demo-avatar-5", "MyRust", "MR", 44.0, AvatarShape::Rounded);
        });
        text(context, &format!("{} · initials fall back when no image is supplied", label))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
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
