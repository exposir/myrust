//! Raw C-ABI wasm entry points for `/docs-demo`.
//!
//! The JavaScript host supplies `env.myrust_now_ms` and reads the RGBA buffer
//! through `demo_buffer_ptr`/`demo_render`. No wasm-bindgen or JS glue is used.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, OnceLock},
};

use myrust_ui::{
    accordion, alert_dialog, autocomplete, back_forward_buttons, breadcrumbs, button, calendar,
    checkbox, code_editor, collapsible, color_picker, combobox, column, container, context_menu,
    date_input, decimal_input, dialog, diff_viewer, drag_source, drop_target, editable_text,
    find_bar, form_field, format_date, format_number, frame_stats_panel, grid, grid_view, inspector,
    link, list, menu_button, number_input, outline, overlay, popover, radio_group, range_slider,
    rating, rich_text, row, router, scroll_view, search_field, segmented, select, sidebar, slider,
    spinner, split_view, stepper, switch, tabs, table, text, text_field, time_input, toasts,
    token_field, toggle, toolbar, tooltip_for, transition, tree, avatar, badge, bar_chart, icon,
    image, line_chart, meter, progress, Animation, AccordionMode, AccordionSection,
    AccordionState, AutocompleteState, AvatarShape, BreadcrumbItem, CalendarState,
    CodeEditorState, ColorPickerState, ComboboxState, ContextMenuState, DateInputState, DateStyle,
    DecimalInputOptions, DialogAction, DiffViewerState, Direction, Color, Context, CpuRasterizer,
    DisplayList, DragState, DrawCommand, Duration, Easing, EditableTextState, FontFace,
    GridViewState, ImageData, InspectorItem, InspectorTree, ListRow, ListState, Locale, MenuState,
    NativeUi,
    NumberInputOptions, OverlayState, Page, PaintSummary, RangeSliderState, RenderSize, Role,
    RouterState, SidebarItem, SidebarSection, Size, SplitOrientation, SplitViewState, TableColumn,
    TableRow, TableState, Theme, ToastQueue, ToggleGroup, ToolbarItem, ToolbarState,
    Transform, TransitionState, TreeState, TreeNode,
};

thread_local! {
    static APP: RefCell<Option<DemoApp>> = const { RefCell::new(None) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

static FONT: OnceLock<Option<Arc<FontFace>>> = OnceLock::new();

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
                NativeUi::new(move |context| {
                    tabs_demo(context, &view_selected, &view_frame, &view_probe);
                })
            }
            1 => {
                let view_frame = Rc::clone(&frame);
                NativeUi::new(move |context| {
                    markdown_demo(context, &view_frame);
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
                let target = Rc::new(Cell::new(0.0_f32));
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
                let group = Rc::new(RefCell::new(ToggleGroup::new()));
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
                let value = Rc::new(Cell::new(false));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    checkbox_demo(context, &view_value, &view_probe);
                })
            }
            32 => {
                let value = Rc::new(Cell::new(false));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    switch_demo(context, &view_value, &view_probe);
                })
            }
            33 => {
                let selected = Rc::new(Cell::new(0_i32));
                let view_selected = Rc::clone(&selected);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    radio_demo(context, &view_selected, &view_probe);
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
                let value = Rc::new(Cell::new(0.0_f32));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    slider_demo(context, &view_value, &view_probe);
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
                let value = Rc::new(Cell::new(0_u32));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| {
                    rating_demo(context, &view_value, &view_probe);
                })
            }
            39 => {
                let value = Rc::new(RefCell::new(String::new()));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| text_input_demo(context, &view_value, &view_probe))
            }
            40 => {
                let value = Rc::new(RefCell::new("hello".to_string()));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| primitives_demo(context, &view_value, &view_probe))
            }
            41 => {
                let value = Rc::new(RefCell::new(String::new()));
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
                let query = Rc::new(RefCell::new(String::new()));
                let state = Rc::new(RefCell::new(ComboboxState::new()));
                let view_query = Rc::clone(&query);
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| combobox_demo(context, &view_query, &view_state, &view_probe))
            }
            45 => {
                let text = Rc::new(RefCell::new(String::new()));
                let state = Rc::new(RefCell::new(AutocompleteState::new()));
                let view_text = Rc::clone(&text);
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| autocomplete_demo(context, &view_text, &view_state, &view_probe))
            }
            46 => {
                let tokens = Rc::new(RefCell::new(Vec::<String>::new()));
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
                let query = Rc::new(RefCell::new(String::new()));
                let current = Rc::new(Cell::new(0_usize));
                let view_query = Rc::clone(&query);
                let view_current = Rc::clone(&current);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| find_demo(context, &view_query, &view_current, &view_probe))
            }
            49 => {
                let source = Rc::new(Cell::new(false));
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
                let state = Rc::new(RefCell::new(ColorPickerState::default()));
                let view_color = Rc::clone(&color);
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| color_demo(context, &view_color, &view_state, &view_probe))
            }
            56 => {
                let state = Rc::new(RefCell::new(ListState::new()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| list_demo(context, &view_state, &view_probe))
            }
            57 => {
                let state = Rc::new(RefCell::new(TableState::new()));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| table_demo(context, &view_state, &view_probe))
            }
            58 => {
                let state = Rc::new(RefCell::new(TreeState::new()));
                let expanded = Rc::new(RefCell::new(Vec::<String>::new()));
                let view_state = Rc::clone(&state);
                let view_expanded = Rc::clone(&expanded);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| tree_demo(context, &view_state, &view_expanded, &view_probe, false))
            }
            59 => {
                let state = Rc::new(RefCell::new(TreeState::new()));
                let expanded = Rc::new(RefCell::new(Vec::<String>::new()));
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
                let columns = Rc::new(Cell::new(2_usize));
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
                let open = Rc::new(Cell::new(false));
                let view_open = Rc::clone(&open);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| collapsible_demo(context, &view_open, &view_probe))
            }
            65 => {
                let state = Rc::new(RefCell::new(AccordionState::new(AccordionMode::Single)));
                let view_state = Rc::clone(&state);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| accordion_demo(context, &view_state, &view_probe))
            }
            66 => {
                let value = Rc::new(RefCell::new(String::new()));
                let view_value = Rc::clone(&value);
                let view_probe = Rc::clone(&probe);
                NativeUi::new(move |context| form_demo(context, &view_value, &view_probe))
            }
            67 => {
                let selected = Rc::new(RefCell::new(String::new()));
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
                let spinning = Rc::new(Cell::new(false));
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

fn tabs_demo(context: &mut Context, selected: &Rc<RefCell<usize>>, frame: &Rc<Cell<u64>>, probe: &Rc<Cell<i32>>) {
    let tick = frame.get();
    frame.set(tick.wrapping_add(1));
    column(context)
        .fill()
        .padding(18.0)
        .gap(14.0)
        .children(|context| {
            myrust_ui::row(context).gap(8.0).children(|context| {
                text(context, "Tabs")
                    .font_size(22.0)
                    .bold()
                    .color(Color::rgb(241, 245, 249));
                let radius = 3.0 + (tick % 3) as f32;
                container(context)
                    .size(10.0, 10.0)
                    .draw([DrawCommand::Circle {
                        x: 5.0,
                        y: 5.0,
                        radius,
                        color: Color::rgb(34, 211, 238),
                    }]);
            });
            text(
                context,
                "Click a tab or use Left/Right. The selected index is caller-owned.",
            )
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
            let mut item = selected.borrow_mut();
            let state = tabs(
                context,
                "docs-demo-tabs",
                &mut item,
                &["General", "Appearance", "Advanced"],
            );
            let body = match *item {
                0 => "General settings live here.",
                1 => "Appearance: theme, density, and contrast.",
                _ => "Advanced: diagnostics and developer tools.",
            };
            text(context, body)
                .font_size(15.0)
                .color(Color::rgb(226, 232, 240));
            text(context, &format!("selected = {}", state.selected))
                .font_size(12.0)
                .color(Color::rgb(34, 211, 238));
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

const MARKDOWN: &str = r#"# MyRust Markdown

A small, predictable Markdown renderer for native UI.

It is deliberately a subset: no HTML, no tables, no code fences.

## Supported

- headings from level 1 to 6
- paragraphs with inline code, bold, italic and links
- unordered and ordered lists
- AccessNode labels for every inline span

## Accessibility

The Markdown root is a Group labelled by its stable id, with a value like "12 blocks".

Each inline span is a Label whose value is plain, bold, italic, code or link.

The renderer is partial: it does not run a CommonMark conformance suite.

It also does not parse fenced code blocks, tables, task lists or footnotes.
"#;

fn markdown_demo(context: &mut Context, frame: &Rc<Cell<u64>>) {
    let tick = frame.get();
    frame.set(tick.wrapping_add(1));
    let height = (context.available_size().height - 24.0).max(80.0);
    column(context)
        .fill()
        .padding(16.0)
        .gap(8.0)
        .children(|context| {
            scroll_view(context, "docs-demo-markdown", height, |context| {
                context.markdown("docs-demo-markdown-body", MARKDOWN);
            });
            text(context, "MyRust native Markdown demo")
                .font_size(11.0)
                .color(Color::rgb(100, 116, 139));
            let radius = 2.0 + (tick % 2) as f32;
            container(context)
                .size(8.0, 8.0)
                .draw([DrawCommand::Circle {
                    x: 4.0,
                    y: 4.0,
                    radius,
                    color: Color::rgb(34, 211, 238),
                }]);
        });
}

fn font() -> Option<Arc<FontFace>> {
    FONT.get_or_init(|| {
        let bytes = include_bytes!("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").to_vec();
        myrust_ui::FontRegistry::load_bytes(bytes).ok().map(Arc::new)
    })
    .clone()
}

#[no_mangle]
pub extern "C" fn demo_render(slug: u32, width: f32, height: f32, dpr: f32) -> u32 {
    if !width.is_finite() || !height.is_finite() || width < 1.0 || height < 1.0 {
        return 0;
    }
    let dpr = if dpr.is_finite() { dpr.clamp(1.0, 3.0) } else { 1.0 };
    let pixel_width = (width * dpr).round().max(1.0) as u32;
    let pixel_height = (height * dpr).round().max(1.0) as u32;
    let mut rgba = Vec::new();
    let rendered = APP.with(|cell| {
        let mut app = cell.borrow_mut();
        if app.as_ref().map(|app| app.slug) != Some(slug) {
            *app = Some(DemoApp::new(slug));
        }
        let Some(app) = app.as_mut() else {
            return false;
        };
        let frame = match app.ui.render(Size::new(width, height)) {
            Ok(frame) => frame,
            Err(_) => return false,
        };
        let mut display = DisplayList::from_commands(frame.display.commands);
        display.commands.insert(
            0,
            DrawCommand::TransformPush {
                transform: Transform {
                    translation: myrust_ui::Point::new(0.0, 0.0),
                    scale: myrust_ui::Point::new(dpr, dpr),
                    rotation: 0.0,
                },
            },
        );
        display.commands.push(DrawCommand::TransformPop);
        let raster = match CpuRasterizer::rasterize_with_font(
            &display,
            RenderSize::new(pixel_width, pixel_height),
            Theme::dark().background,
            font(),
        ) {
            Ok(frame) => frame,
            Err(_) => return false,
        };
        rgba.reserve(raster.pixels.len() * 4);
        for pixel in raster.pixels {
            rgba.push(((pixel >> 16) & 0xff) as u8);
            rgba.push(((pixel >> 8) & 0xff) as u8);
            rgba.push((pixel & 0xff) as u8);
            rgba.push(255);
        }
        true
    });
    if !rendered {
        OUTPUT.with(|output| output.borrow_mut().clear());
        return 0;
    }
    OUTPUT.with(|output| {
        let mut output = output.borrow_mut();
        *output = rgba;
        output.len() as u32
    })
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
    column(context)
        .fill()
        .padding(18.0)
        .gap(10.0)
        .children(|context| {
            if button(context, "Move box").clicked {
                moved.set(!is_moved);
                probe.set(if is_moved { 0 } else { 1 });
            }
            container(context).size(180.0, 60.0).draw([DrawCommand::RoundedRect {
                x: frame.bounds.x,
                y: frame.bounds.y,
                width: frame.bounds.width,
                height: frame.bounds.height,
                radius: myrust_ui::Radius::all(8.0),
                color: Color::rgb(34, 211, 238),
            }]);
            text(context, &format!("progress {:.2}", frame.progress))
                .font_size(12.0)
                .color(Color::rgb(148, 163, 184));
        });
}

fn motion_demo(context: &mut Context, target: &Rc<Cell<f32>>, probe: &Rc<Cell<i32>>) {
    column(context)
        .fill()
        .padding(18.0)
        .gap(10.0)
        .children(|context| {
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
            container(context).size(220.0, 24.0).draw([DrawCommand::RoundedRect {
                x: 0.0,
                y: 0.0,
                width: 220.0 * value.clamp(0.0, 1.0),
                height: 24.0,
                radius: myrust_ui::Radius::all(6.0),
                color: Color::rgb(34, 211, 238),
            }]);
            text(context, &format!("value {:.2}", value))
                .font_size(12.0)
                .color(Color::rgb(148, 163, 184));
        });
}

fn transforms_demo(context: &mut Context, rotated: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context)
        .fill()
        .padding(18.0)
        .gap(10.0)
        .children(|context| {
            let is_rotated = rotated.get();
            if button(context, "Rotate scope").clicked {
                rotated.set(!is_rotated);
                probe.set(if is_rotated { 0 } else { 1 });
            }
            let transform = Transform {
                translation: myrust_ui::Point::new(if is_rotated { 24.0 } else { 0.0 }, 0.0),
                scale: myrust_ui::Point::new(1.0, 1.0),
                rotation: if is_rotated { 0.25 } else { 0.0 },
            };
            context.transform(transform, |context| {
                container(context).size(60.0, 60.0).draw([DrawCommand::Rect {
                    x: 10.0,
                    y: 10.0,
                    width: 40.0,
                    height: 40.0,
                    color: Color::rgb(34, 211, 238),
                }]);
            });
            text(context, if is_rotated { "rotated" } else { "identity" })
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
    column(context).fill().padding(18.0).gap(10.0).children(|context| {
        if button(context, "Click me").clicked {
            count.set(count.get() + 1);
            probe.set(count.get());
        }
        text(context, &format!("clicks = {}", count.get()))
            .font_size(13.0)
            .color(Color::rgb(34, 211, 238));
    });
}

fn link_demo(context: &mut Context, count: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(10.0).children(|context| {
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
    let mut state = menu.borrow_mut();
    let response = menu_button(context, "docs-demo-menu", "Actions", &mut state, &["Cut", "Copy", "Paste"]);
    if let Some(index) = response.selected {
        selected.set(index as i32);
        probe.set(index as i32 + 1);
    }
    text(context, &format!("selected index = {}", selected.get()))
        .font_size(13.0)
        .color(Color::rgb(34, 211, 238));
}

fn context_demo(
    context: &mut Context,
    menu: &Rc<RefCell<ContextMenuState>>,
    probe: &Rc<Cell<i32>>,
) {
    column(context).fill().padding(18.0).gap(10.0).children(|context| {
        let mut state = menu.borrow_mut();
        let area = myrust_ui::Rect::new(18.0, 56.0, 300.0, 80.0);
        let response = context_menu(context, "docs-demo-context", &mut state, area, &["Inspect", "Duplicate", "Delete"]);
        if response.opened {
            probe.set(10);
        }
        if response.selected.is_some() {
            probe.set(response.selected.unwrap_or(0) as i32 + 11);
        }
        text(context, "Right-click the area")
            .font_size(13.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn toggle_demo(context: &mut Context, group: &Rc<RefCell<ToggleGroup>>, probe: &Rc<Cell<i32>>) {
    let mut group = group.borrow_mut();
    let mut left = group.active.as_deref() == Some("docs-demo-toggle-left");
    let mut right = group.active.as_deref() == Some("docs-demo-toggle-right");
    let left_state = toggle(context, "docs-demo-toggle-left", "Left", &mut left, Some(&mut group));
    let right_state = toggle(context, "docs-demo-toggle-right", "Right", &mut right, Some(&mut group));
    if left_state.changed || right_state.changed {
        probe.set(if group.active.is_some() { 1 } else { 0 });
    }
    text(context, &format!("group = {:?}", group.active))
        .font_size(13.0)
        .color(Color::rgb(34, 211, 238));
}

fn segmented_demo(context: &mut Context, selected: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        let mut value = selected.get().max(0) as usize;
        let response = segmented(context, "docs-demo-segmented", &mut value, &["One", "Two", "Three"]);
        if response.changed {
            selected.set(value as i32);
            probe.set(value as i32 + 1);
        }
    });
}

fn toolbar_demo(context: &mut Context, selected: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        let items = [
            ToolbarItem::new("cut", "Cut", 90.0),
            ToolbarItem::new("copy", "Copy", 90.0),
            ToolbarItem::new("paste", "Paste", 90.0),
        ];
        let mut state = ToolbarState::new();
        let response = toolbar(context, "docs-demo-toolbar", &mut state, &items, 260.0, |context, index| {
            button(context, items[index].label);
        });
        if response.overflow_open {
            probe.set(10);
        }
        if let Some(index) = response.selected {
            selected.set(index as i32);
            probe.set(index as i32 + 1);
        }
        text(context, &format!("overflow selection = {}", selected.get()))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}


fn checkbox_demo(context: &mut Context, value: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        let mut checked = value.get();
        if checkbox(context, "Accept terms", &mut checked).changed {
            value.set(checked);
            probe.set(if checked { 1 } else { 0 });
        }
        text(context, "CheckBox label is the fixed string \"checkbox\".")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn switch_demo(context: &mut Context, value: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        let mut on = value.get();
        if switch(context, "docs-demo-switch", &mut on).changed {
            value.set(on);
            probe.set(if on { 1 } else { 0 });
        }
        text(context, if value.get() { "on" } else { "off" })
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn radio_demo(context: &mut Context, selected: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        let mut value = selected.get().max(0) as usize;
        let response = radio_group(context, "docs-demo-radio", &mut value, &["Small", "Medium", "Large"]);
        if response.changed {
            selected.set(value as i32);
            probe.set(value as i32 + 1);
        }
        text(context, &format!("selected = {}", value))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn select_demo(context: &mut Context, selected: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        let mut value = selected.get().max(0) as usize;
        let response = select(context, "docs-demo-select", &mut value, &["One", "Two", "Three"]);
        if response.open {
            probe.set(10);
        }
        if response.changed {
            selected.set(value as i32);
            probe.set(value as i32 + 1);
        }
        text(context, &format!("selected = {}", value))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn slider_demo(context: &mut Context, value: &Rc<Cell<f32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        container(context).width(240.0).children(|context| {
            let mut current = value.get();
            let response = slider(context, "docs-demo-slider", &mut current, 0.0, 100.0);
            if response.changed {
                value.set(current);
                probe.set(current.round() as i32);
            }
            text(context, &format!("value {:.0}", current))
                .font_size(12.0)
                .color(Color::rgb(34, 211, 238));
        });
    });
}

fn range_demo(context: &mut Context, state: &Rc<RefCell<RangeSliderState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        container(context).width(240.0).children(|context| {
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
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        let mut current = value.get();
        let response = stepper(context, "docs-demo-stepper", &mut current, 0, 10, 1);
        if response.changed {
            value.set(current);
            probe.set(current as i32);
        }
    });
}

fn rating_demo(context: &mut Context, value: &Rc<Cell<u32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        let mut current = value.get();
        let response = rating(context, "docs-demo-rating", &mut current, 5);
        if response.changed {
            value.set(current);
            probe.set(current as i32);
        }
    });
}


fn text_input_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        text(context, "Text input").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut text_value = value.borrow_mut();
        let field = text_field(context, "docs-demo-text", &mut text_value);
        if field.changed {
            probe.set(text_value.len() as i32);
        }
        text(context, &format!("{} characters", text_value.len()))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn primitives_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
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
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        text(context, "Number").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut text_value = value.borrow_mut();
        let state = number_input(context, "docs-demo-number", &mut text_value, NumberInputOptions::default().allow_negative(true));
        if state.changed {
            probe.set(state.value.unwrap_or(0.0).round() as i32);
        }
    });
}

fn decimal_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        text(context, "Decimal").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut text_value = value.borrow_mut();
        let state = decimal_input(context, "docs-demo-decimal", &mut text_value, DecimalInputOptions::default());
        if state.changed {
            probe.set(state.units.unwrap_or(0) as i32);
        }
    });
}

fn search_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        text(context, "Search").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut query = value.borrow_mut();
        let state = search_field(context, "docs-demo-search", &mut query);
        if state.changed {
            probe.set(query.len() as i32);
        }
    });
}

fn combobox_demo(
    context: &mut Context,
    query: &Rc<RefCell<String>>,
    state: &Rc<RefCell<ComboboxState>>,
    probe: &Rc<Cell<i32>>,
) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
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
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
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
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        text(context, "Tokens").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut tokens = tokens.borrow_mut();
        let mut input = input.borrow_mut();
        let state = token_field(context, "docs-demo-token", &mut tokens, &mut input);
        if state.added || state.removed {
            probe.set(tokens.len() as i32);
        }
    });
}

fn editable_demo(
    context: &mut Context,
    value: &Rc<RefCell<String>>,
    state: &Rc<RefCell<EditableTextState>>,
    probe: &Rc<Cell<i32>>,
) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        text(context, "Editable").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut value = value.borrow_mut();
        let mut state = state.borrow_mut();
        let response = editable_text(context, "docs-demo-edit", &mut state, &mut value);
        if response.editing || response.submitted {
            probe.set(1);
        }
    });
}

fn find_demo(
    context: &mut Context,
    query: &Rc<RefCell<String>>,
    current: &Rc<Cell<usize>>,
    probe: &Rc<Cell<i32>>,
) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        text(context, "Find").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        let mut query = query.borrow_mut();
        let mut index = current.get();
        let state = find_bar(context, "docs-demo-find", &mut query, &mut index, 5);
        current.set(index);
        if state.changed || state.next_clicked || state.previous_clicked {
            probe.set(index as i32 + 1);
        }
    });
}

fn rich_text_demo(context: &mut Context, source: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        text(context, "Rich text (read-only)").font_size(20.0).bold().color(Color::rgb(241, 245, 249));
        if button(context, "Toggle emphasis").clicked {
            source.set(!source.get());
            probe.set(if source.get() { 1 } else { 0 });
        }
        let text = if source.get() {
            "**Bold** and `code` with a [link](https://example.com)."
        } else {
            "Plain paragraph with *italic* emphasis."
        };
        rich_text(context, "docs-demo-rich", text);
    });
}

fn code_demo(context: &mut Context, state: &Rc<RefCell<CodeEditorState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        let code = "fn main() {\n    let x = 42;\n    // read-only\n}";
        let mut state = state.borrow_mut();
        let response = code_editor(context, "docs-demo-code", code, &mut state);
        if let Some(line) = response.clicked_line {
            probe.set(line as i32 + 1);
        }
    });
}

fn diff_demo(context: &mut Context, state: &Rc<RefCell<DiffViewerState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        let mut state = state.borrow_mut();
        let response = diff_viewer(
            context,
            "docs-demo-diff",
            "fn main() {\n    let x = 1;\n}",
            "fn main() {\n    let x = 2;\n}",
            &mut state,
        );
        probe.set(if response.layout == myrust_ui::DiffLayout::SideBySide { 1 } else { 0 });
    });
}

fn calendar_demo(context: &mut Context, state: &Rc<RefCell<CalendarState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        let mut state = state.borrow_mut();
        let response = calendar(context, "docs-demo-calendar", &mut state);
        if let Some(day) = response.selected {
            probe.set(day as i32);
        }
    });
}

fn date_demo(
    context: &mut Context,
    text: &Rc<RefCell<String>>,
    state: &Rc<RefCell<DateInputState>>,
    probe: &Rc<Cell<i32>>,
) {
    column(context).fill().padding(18.0).children(|context| {
        container(context).width(300.0).children(|context| {
            let mut text = text.borrow_mut();
            let mut state = state.borrow_mut();
            let response = date_input(context, "docs-demo-date", &mut text, &mut state);
            if response.open {
                probe.set(2);
            } else if response.valid {
                probe.set(1);
            }
        });
    });
}

fn time_demo(context: &mut Context, text: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        container(context).width(280.0).children(|context| {
            let mut text = text.borrow_mut();
            let response = time_input(context, "docs-demo-time", &mut text);
            if response.changed {
                probe.set(response.hour as i32 * 60 + response.minute as i32);
            }
        });
    });
}

fn color_demo(
    context: &mut Context,
    color: &Rc<RefCell<Color>>,
    state: &Rc<RefCell<ColorPickerState>>,
    probe: &Rc<Cell<i32>>,
) {
    column(context).fill().padding(18.0).children(|context| {
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
}

fn list_demo(context: &mut Context, state: &Rc<RefCell<ListState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        let rows = [
            ListRow::new("r0", "Row 0"),
            ListRow::new("r1", "Row 1"),
            ListRow::new("r2", "Row 2"),
            ListRow::new("r3", "Row 3"),
            ListRow::new("r4", "Row 4"),
        ];
        let mut state = state.borrow_mut();
        let response = list(context, "docs-demo-list", &mut state, &rows, 28.0, 140.0);
        if let Some(id) = response.clicked_id {
            probe.set(id.trim_start_matches('r').parse().unwrap_or(0) + 1);
        }
    });
}

fn table_demo(context: &mut Context, state: &Rc<RefCell<TableState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        let columns = [TableColumn::new("Name", 120.0), TableColumn::new("Value", 80.0)];
        let rows = [
            TableRow::new("a", &["Alpha", "1"]),
            TableRow::new("b", &["Beta", "2"]),
        ];
        let mut state = state.borrow_mut();
        let response = table(context, "docs-demo-table", &mut state, &columns, &rows);
        if response.sort.is_some() {
            probe.set(1);
        }
    });
}

fn tree_demo(
    context: &mut Context,
    state: &Rc<RefCell<TreeState>>,
    expanded: &Rc<RefCell<Vec<String>>>,
    probe: &Rc<Cell<i32>>,
    outline_mode: bool,
) {
    column(context).fill().padding(18.0).children(|context| {
        let nodes = [
            TreeNode::new("root", "Root", 0, true),
            TreeNode::new("child", "Child", 1, false),
            TreeNode::new("other", "Other", 0, false),
        ];
        let mut state = state.borrow_mut();
        let mut expanded = expanded.borrow_mut();
        let response = if outline_mode {
            outline(context, "docs-demo-outline", &mut state, &nodes, &mut expanded)
        } else {
            tree(context, "docs-demo-tree", &mut state, &nodes, &mut expanded)
        };
        if response.changed || response.toggled_id.is_some() {
            probe.set(1);
        }
    });
}

fn grid_view_demo(context: &mut Context, state: &Rc<RefCell<GridViewState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        let mut state = state.borrow_mut();
        let response = grid_view(
            context,
            "docs-demo-grid-view",
            &mut state,
            8,
            Size::new(54.0, 42.0),
            3,
            8.0,
            150.0,
            |context, index| {
                text(context, &format!("Item {index}"))
                    .font_size(11.0)
                    .color(Color::rgb(241, 245, 249));
            },
        );
        if let Some(index) = response.selected {
            probe.set(index as i32 + 1);
        }
    });
}

fn scroll_demo(context: &mut Context, scroll: &Rc<Cell<f32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        let state = scroll_view(context, "docs-demo-scroll", 140.0, |context| {
            for index in 0..8 {
                container(context).size(300.0, 28.0).draw([DrawCommand::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 300.0,
                    height: 28.0,
                    color: Color::rgb(30, 41, 59),
                }]);
                text(context, &format!("Scroll row {index}"))
                    .font_size(14.0)
                    .color(Color::rgb(226, 232, 240));
            }
        });
        scroll.set(state.scroll);
        probe.set(state.scroll as i32);
    });
}

fn grid_demo(context: &mut Context, columns: &Rc<Cell<usize>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        if button(context, "Toggle columns").clicked {
            columns.set(if columns.get() == 2 { 3 } else { 2 });
            probe.set(columns.get() as i32);
        }
        grid(context).columns(columns.get()).gap(8.0).children(|context| {
            for index in 0..6 {
                container(context).size(50.0, 30.0).draw([DrawCommand::Rect {
                    x: 0.0, y: 0.0, width: 50.0, height: 30.0, color: Color::rgb(34, 211, 238),
                }]);
                let _ = index;
            }
        });
    });
}

fn split_demo(context: &mut Context, state: &Rc<RefCell<SplitViewState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        if button(context, "Toggle split").clicked {
            let mut state = state.borrow_mut();
            state.ratio = if state.ratio < 0.6 { 0.7 } else { 0.3 };
            probe.set((state.ratio * 10.0) as i32);
        }
        let mut state = state.borrow_mut();
        split_view(
            context,
            "docs-demo-split",
            &mut state,
            SplitOrientation::Horizontal,
            |context| {
                text(context, "Left pane").font_size(12.0);
            },
            |context| {
                text(context, "Right pane").font_size(12.0);
            },
        );
        probe.set((state.ratio * 10.0) as i32);
    });
}

fn collapsible_demo(context: &mut Context, open: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        let mut value = open.get();
        let state = collapsible(context, "docs-demo-collapsible", &mut value, "Details", |context| {
            text(context, "Collapsible body").font_size(12.0).color(Color::rgb(148, 163, 184));
        });
        if state.changed {
            open.set(value);
            probe.set(if value { 1 } else { 0 });
        }
    });
}

fn accordion_demo(context: &mut Context, state: &Rc<RefCell<AccordionState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        let sections = [
            AccordionSection::new("one", "One", "First section"),
            AccordionSection::new("two", "Two", "Second section"),
        ];
        let mut state = state.borrow_mut();
        let response = accordion(context, "docs-demo-accordion", &mut state, &sections);
        if response.changed {
            probe.set(state.open.len() as i32 + 1);
        }
    });
}

fn form_demo(context: &mut Context, value: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        form_field(
            context,
            "docs-demo-field",
            "Label",
            Some("Description"),
            None,
            |context| {
                let mut text = value.borrow_mut();
                if text_field(context, "docs-demo-form-input", &mut text).changed {
                    probe.set(text.len() as i32);
                }
            },
        );
    });
}

fn sidebar_demo(context: &mut Context, selected: &Rc<RefCell<String>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        let items = [
            SidebarItem::new("a", "Alpha"),
            SidebarItem::new("b", "Beta"),
            SidebarItem::new("c", "Gamma"),
        ];
        let sections = [SidebarSection::new("Group", &items)];
        let mut selected = selected.borrow_mut();
        let response = sidebar(context, "docs-demo-sidebar", &mut selected, &sections);
        if response.changed || response.clicked_id.is_some() {
            probe.set(selected.len() as i32);
        }
    });
}

fn breadcrumbs_demo(context: &mut Context, count: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        let items = [
            BreadcrumbItem::new("home", "Home"),
            BreadcrumbItem::new("docs", "Docs"),
            BreadcrumbItem::new("current", "Current"),
        ];
        let response = breadcrumbs(context, "docs-demo-crumbs", &items);
        if response.clicked.is_some() {
            count.set(count.get() + 1);
            probe.set(count.get());
        }
    });
}

fn history_demo(context: &mut Context, router_state: &Rc<RefCell<RouterState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
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
        text(context, &format!("page: {}", state.current()))
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
    let mut is_open = open.get();
    if button(context, "Open dialog").clicked {
        is_open = true;
    }
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
}

fn alert_demo(
    context: &mut Context,
    open: &Rc<Cell<bool>>,
    result: &Rc<Cell<i32>>,
    probe: &Rc<Cell<i32>>,
) {
    let mut is_open = open.get();
    if button(context, "Open alert").clicked {
        is_open = true;
    }
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
}

fn popover_demo(context: &mut Context, open: &Rc<RefCell<OverlayState>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        let trigger = button(context, "Open popover");
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
    column(context).fill().padding(18.0).children(|context| {
        let trigger = button(context, "Hover for tooltip");
        let anchor = trigger
            .bounds
            .unwrap_or_else(|| myrust_ui::Rect::new(18.0, 18.0, 160.0, 30.0));
        let state = tooltip_for(context, "docs-demo-tooltip", anchor, &trigger.id, "Tooltip text");
        if state.visible {
            seen.set(true);
            probe.set(10);
        }
    });
}

fn toast_demo(context: &mut Context, queue: &Rc<RefCell<ToastQueue>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).children(|context| {
        if button(context, "Push toast").clicked {
            queue.borrow_mut().push_message(context, "Saved");
        }
        let mut queue = queue.borrow_mut();
        let response = toasts(context, "docs-demo-toast", &mut queue);
        probe.set(response.visible as i32);
    });
}

fn progress_demo(context: &mut Context, value: &Rc<Cell<f32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        if button(context, "Advance").clicked {
            let next = (value.get() + 25.0).min(100.0);
            value.set(next);
            probe.set(next as i32);
        }
        progress(context, "docs-demo-progress", value.get(), 100.0);
        text(context, &format!("{:.0}%", value.get()))
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn spinner_demo(context: &mut Context, spinning: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        if button(context, "Toggle spinner").clicked {
            spinning.set(!spinning.get());
            probe.set(if spinning.get() { 1 } else { 0 });
        }
        if spinning.get() {
            spinner(context, "docs-demo-spinner");
        }
    });
}

fn meter_demo(context: &mut Context, value: &Rc<Cell<f32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        if button(context, "Change level").clicked {
            let next = if value.get() < 50.0 { 75.0 } else if value.get() < 90.0 { 95.0 } else { 20.0 };
            value.set(next);
            probe.set(next as i32);
        }
        meter(context, "docs-demo-meter", value.get(), 0.0, 100.0, 33.0, 66.0);
    });
}

fn badge_demo(context: &mut Context, count: &Rc<Cell<i32>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        if button(context, "Add badge").clicked {
            count.set(count.get() + 1);
            probe.set(count.get());
        }
        badge(context, &format!("{}", count.get()));
    });
}

fn chart_demo(context: &mut Context, line: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        if button(context, "Toggle chart").clicked {
            line.set(!line.get());
            probe.set(if line.get() { 1 } else { 0 });
        }
        let values = [0.2, 0.8, 0.4, 0.9, 0.6];
        if line.get() {
            line_chart(context, "docs-demo-chart", &values, 120.0, Some(0.0), Some(1.0));
        } else {
            bar_chart(context, "docs-demo-chart", &values, 120.0, Some(0.0), Some(1.0));
        }
    });
}

fn image_demo(context: &mut Context, color: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        if button(context, "Toggle pixels").clicked {
            color.set(!color.get());
            probe.set(if color.get() { 1 } else { 0 });
        }
        let pixels = if color.get() {
            [255u8, 80, 80, 255, 255, 200, 80, 255, 80, 200, 255, 255, 80, 255, 200, 255]
        } else {
            [34u8, 211, 238, 255, 80, 120, 255, 255, 80, 255, 180, 255, 20, 180, 160, 255]
        };
        let data = ImageData::new(&pixels, 2, 2);
        image(context, "docs-demo-image", data, 64.0, 64.0);
        text(context, "ImageData is caller-decoded RGBA8")
            .font_size(12.0)
            .color(Color::rgb(148, 163, 184));
    });
}

fn icon_demo(context: &mut Context, alternate: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        if button(context, "Toggle icon").clicked {
            alternate.set(!alternate.get());
            probe.set(if alternate.get() { 1 } else { 0 });
        }
        let name = if alternate.get() { "check" } else { "arrow_right" };
        icon(context, "docs-demo-icon", name, 24.0, Color::rgb(34, 211, 238));
    });
}

fn avatar_demo(context: &mut Context, alternate: &Rc<Cell<bool>>, probe: &Rc<Cell<i32>>) {
    column(context).fill().padding(18.0).gap(8.0).children(|context| {
        if button(context, "Toggle initials").clicked {
            alternate.set(!alternate.get());
            probe.set(if alternate.get() { 1 } else { 0 });
        }
        let (label, initials) = if alternate.get() {
            ("Ada Lovelace", "AL")
        } else {
            ("Grace Hopper", "GH")
        };
        avatar(context, "docs-demo-avatar", label, initials, 40.0, AvatarShape::Circle);
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
pub extern "C" fn demo_buffer_ptr() -> *const u8 {
    OUTPUT.with(|output| output.borrow().as_ptr())
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
