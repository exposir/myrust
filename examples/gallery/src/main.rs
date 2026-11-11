//! The widget gallery: base controls plus the Batch 1 choices, Batch 2a
//! anchored surfaces, Batch 2b feedback/dialog/form helpers, Batch 3a
//! layout/navigation controls, Batch 3b data controls, Batch 4a numeric
//! inputs, Batch 4b search/inline-editing controls, Batch 5 routing/formatted
//! text, Batch 6a data-display controls, and Batch 6b icons/transitions, drawn
//! by the immediate-mode runtime.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use myrust_core::app::{App, WindowConfig, WindowContent};
use myrust_core::ui::{
    accordion, alert_dialog, autocomplete, avatar, avatar_image, back_forward_buttons, badge,
    bar_chart, breadcrumbs, button, calendar, checkbox, code_editor, collapsible, color_picker,
    column, combobox, container, context_menu, count_matches, date_input, decimal_input, dialog,
    diff_viewer, drag_source, drop_target, editable_text, find_bar, form_field, format_date,
    format_number, frame_stats_panel, grid, grid_view, icon, image, inspector, line_chart, link,
    list, markdown, menu_button, meter, number_input, outline, popover, progress, radio_group,
    range_slider, rating, reorder, reorder_list, rich_text, router, row, search_field, segmented,
    select, sidebar, slider, spinner, split_view, stepper, switch, table, tabs, text, text_field,
    time_input, toasts, toggle, token_field, toolbar, tooltip_for, transition, tree, AccordionMode,
    AccordionSection, AccordionState, AutocompleteState, AvatarShape, BreadcrumbItem,
    CalendarState, CodeEditorState, Color, ColorPickerState, ComboboxState, Context,
    ContextMenuState, DateInputState, DateStyle, DecimalInputOptions, DialogAction,
    DiffViewerState, Direction, DragState, DrawCommand, EditableTextState, GridViewState,
    ImageData, InspectorItem, InspectorTree, ListReorderState, ListRow, ListState, Locale,
    MenuState, MeterTone, NativeUi, NumberInputOptions, OverlayState, Page, PaintSummary, Point,
    Radius, RangeSliderState, Rect, Role, RouterState, SidebarItem, SidebarSection,
    SplitOrientation, SplitViewState, TableColumn, TableRow, TableState, TextAlignment, Theme,
    Toast, ToastDismissal, ToastQueue, ToggleGroup, ToolbarItem, ToolbarState, Transform,
    TransitionState, TreeNode, TreeState,
};

#[derive(Default)]
struct State {
    checked: bool,
    switch_on: bool,
    volume: f32,
    name: String,
    clicks: u32,
    radio: usize,
    selected: usize,
    tab: usize,
    details_open: bool,
    menu: MenuState,
    menu_selection: Option<usize>,
    context_menu: ContextMenuState,
    context_selection: Option<usize>,
    popover_open: OverlayState,
    email: String,
    toasts: ToastQueue,
    toast_status: String,
    dialog_open: bool,
    dialog_status: String,
    alert_open: bool,
    alert_status: String,
    breadcrumb_status: String,
    split: SplitViewState,
    sidebar_selected: String,
    accordion: AccordionState,
    list: ListState,
    table: TableState,
    tree_state: TreeState,
    tree_expanded: Vec<String>,
    outline_state: TreeState,
    outline_expanded: Vec<String>,
    stepper: i64,
    rating: u32,
    range: RangeSliderState,
    number_text: String,
    decimal_text: String,
    search_text: String,
    search_status: String,
    combo_query: String,
    combo: ComboboxState,
    auto_text: String,
    auto: AutocompleteState,
    tokens: Vec<String>,
    token_input: String,
    rename: EditableTextState,
    rename_text: String,
    router: RouterState,
    find_query: String,
    find_current: usize,
    grid: GridViewState,
    migration_text: String,
    renamed: bool,
    link_status: String,
    toggle_group: ToggleGroup,
    toggle_left: bool,
    toggle_right: bool,
    segmented: usize,
    toolbar: ToolbarState,
    calendar: CalendarState,
    date_text: String,
    date_input: DateInputState,
    time_text: String,
    color: Option<Color>,
    color_picker: ColorPickerState,
    code: CodeEditorState,
    diff: DiffViewerState,
    drag: DragState,
    reorder: ListReorderState,
    reorder_rows: Vec<String>,
    drop_status: String,
    primitive_text: String,
    transition: TransitionState,
    transition_far: bool,
}

fn view(context: &mut Context, state: &mut State) {
    let theme = Theme::system();
    let home = |context: &mut Context| {
        text(context, "Home page route").font_size(14.0);
    };
    let detail = |context: &mut Context| {
        text(context, "Detail page route").font_size(14.0);
    };
    let settings = |context: &mut Context| {
        text(context, "Settings page route").font_size(14.0);
    };
    let pages = [
        Page::new("home", &home),
        Page::new("detail", &detail),
        Page::new("settings", &settings),
    ];
    const IMAGE_PIXELS: [u8; 16] = [
        255, 80, 80, 255, 80, 220, 120, 255, 80, 120, 220, 255, 240, 220, 80, 255,
    ];
    column(context)
        .fill()
        .padding(24.0)
        .gap(12.0)
        .children(|context| {
            text(context, "Widget gallery").font_size(26.0).bold();
            text(
                context,
                "Built-in controls, drawn by the immediate-mode native runtime.",
            )
            .font_size(12.0)
            .color(theme.muted);

            if let Some(clicked) = breadcrumbs(
                context,
                "gallery-breadcrumbs",
                &[
                    BreadcrumbItem::new("gallery", "Gallery"),
                    BreadcrumbItem::new("components", "Components"),
                    BreadcrumbItem::new("batch3", "Batch 3a"),
                ],
            )
            .clicked
            {
                state.breadcrumb_status = format!("Breadcrumb: {clicked}");
            }
            text(context, &state.breadcrumb_status)
                .font_size(12.0)
                .color(theme.muted);

            row(context).gap(8.0).children(|context| {
                if button(context, "Primary").clicked {
                    state.clicks += 1;
                }
                if button(context, "Secondary").clicked {
                    state.clicks += 1;
                }
                if button(context, "Tertiary").clicked {
                    state.clicks += 1;
                }
                text(context, &format!("{} clicks", state.clicks))
                    .font_size(12.0)
                    .color(theme.muted);
            });

            row(context).gap(10.0).children(|context| {
                checkbox(context, "enabled", &mut state.checked);
                text(
                    context,
                    if state.checked {
                        "Checked"
                    } else {
                        "Unchecked"
                    },
                )
                .font_size(13.0)
                .color(theme.foreground);
            });

            row(context).gap(10.0).children(|context| {
                switch(context, "gallery-switch", &mut state.switch_on);
                text(
                    context,
                    if state.switch_on {
                        "Notifications on"
                    } else {
                        "Notifications off"
                    },
                )
                .font_size(13.0)
                .color(theme.foreground);
            });

            row(context).gap(10.0).children(|context| {
                text(context, "Volume").font_size(12.0).color(theme.muted);
                slider(context, "volume", &mut state.volume, 0.0, 100.0);
                text(context, &format!("{:.0}%", state.volume))
                    .font_size(12.0)
                    .color(theme.foreground);
            });

            text(context, "Radio group")
                .font_size(12.0)
                .color(theme.muted);
            container(context).size(300.0, 72.0).children(|context| {
                radio_group(
                    context,
                    "gallery-radio",
                    &mut state.radio,
                    &["Compact", "Comfortable", "Spacious"],
                );
            });

            row(context).gap(10.0).children(|context| {
                text(context, "Select").font_size(12.0).color(theme.muted);
                container(context).size(240.0, 34.0).children(|context| {
                    select(
                        context,
                        "gallery-select",
                        &mut state.selected,
                        &["Small", "Medium", "Large"],
                    );
                });
            });

            tabs(
                context,
                "gallery-tabs",
                &mut state.tab,
                &["Overview", "Activity", "Settings"],
            );
            text(
                context,
                match state.tab {
                    0 => "Overview tab content",
                    1 => "Activity tab content",
                    _ => "Settings tab content",
                },
            )
            .font_size(13.0)
            .color(theme.foreground);

            row(context).gap(12.0).children(|context| {
                text(context, "Grid").font_size(12.0).color(theme.muted);
                container(context).size(240.0, 70.0).children(|context| {
                    grid(context).columns(3).gap(6.0).children(|context| {
                        for label in ["A", "B", "C", "D", "E", "F"] {
                            context.container().height(30.0).children(|context| {
                                text(context, label);
                            });
                        }
                    });
                });
                text(context, "3 equal columns, uniform rows")
                    .font_size(12.0)
                    .color(theme.muted);
            });

            row(context).gap(10.0).children(|context| {
                text(context, "Progress").font_size(12.0).color(theme.muted);
                container(context).size(320.0, 8.0).children(|context| {
                    progress(context, "gallery-progress", state.volume, 100.0);
                });
                badge(context, &format!("{:.0}%", state.volume));
            });

            row(context).gap(10.0).children(|context| {
                spinner(context, "gallery-spinner");
                text(context, "Spinner requests animation frames")
                    .font_size(12.0)
                    .color(theme.muted);
                badge(context, &format!("{} new", state.clicks));
            });

            collapsible(
                context,
                "gallery-details",
                &mut state.details_open,
                "Details",
                |context| {
                    text(
                        context,
                        "Collapsible content: a header owns focus, Enter or a click toggles it.",
                    )
                    .font_size(12.0)
                    .color(theme.muted);
                },
            );

            row(context).gap(10.0).children(|context| {
                let menu = menu_button(
                    context,
                    "gallery-menu",
                    "Actions",
                    &mut state.menu,
                    &["New window", "Preferences", "Clear"],
                );
                if let Some(index) = menu.selected {
                    state.menu_selection = Some(index);
                }
                text(
                    context,
                    match state.menu_selection {
                        Some(0) => "Menu: new window",
                        Some(1) => "Menu: preferences",
                        Some(2) => "Menu: clear",
                        _ => "Menu: click or press Enter",
                    },
                )
                .font_size(12.0)
                .color(theme.muted);
            });

            row(context).gap(10.0).children(|context| {
                let hotspot = context
                    .id("gallery-context-area")
                    .button("Right-click for menu");
                let area = hotspot
                    .bounds
                    .unwrap_or_else(|| Rect::new(24.0, 24.0, 180.0, 30.0));
                let response = context_menu(
                    context,
                    "gallery-context-menu",
                    &mut state.context_menu,
                    area,
                    &["Cut", "Copy", "Paste"],
                );
                if let Some(index) = response.selected {
                    state.context_selection = Some(index);
                }
                let target = context
                    .id("gallery-tooltip-target")
                    .button("Hover or focus for tooltip");
                let anchor = target
                    .bounds
                    .unwrap_or_else(|| Rect::new(24.0, 24.0, 180.0, 30.0));
                tooltip_for(
                    context,
                    "gallery-tooltip",
                    anchor,
                    "gallery-tooltip-target",
                    "Tooltip: delayed, focus-aware, non-interactive.",
                );
                text(
                    context,
                    match state.context_selection {
                        Some(0) => "Context: cut",
                        Some(1) => "Context: copy",
                        Some(2) => "Context: paste",
                        _ => "Context: right-click the button",
                    },
                )
                .font_size(12.0)
                .color(theme.muted);
            });

            let trigger = button(context, "Open popover");
            if trigger.clicked {
                state.popover_open.open = true;
            }
            if state.popover_open.open {
                let anchor = trigger
                    .bounds
                    .unwrap_or_else(|| Rect::new(24.0, 24.0, 140.0, 30.0));
                let mut dismiss = false;
                popover(
                    context,
                    "gallery-popover",
                    &mut state.popover_open,
                    anchor,
                    |context| {
                        text(
                            context,
                            "Popover: anchored panel with overlay focus and Escape.",
                        )
                        .font_size(12.0)
                        .max_width(210.0)
                        .color(theme.foreground);
                        if button(context, "Dismiss").clicked {
                            dismiss = true;
                        }
                    },
                );
                if dismiss {
                    state.popover_open.open = false;
                }
            }

            row(context).gap(12.0).children(|context| {
                let workspace = [
                    SidebarItem::new("inbox", "Inbox"),
                    SidebarItem::new("sent", "Sent"),
                ];
                let settings = [
                    SidebarItem::new("profile", "Profile"),
                    SidebarItem::new("preferences", "Preferences"),
                ];
                let sections = [
                    SidebarSection::new("Workspace", &workspace),
                    SidebarSection::new("Settings", &settings),
                ];
                container(context).size(180.0, 190.0).children(|context| {
                    sidebar(context, "gallery-sidebar", &mut state.sidebar_selected, &sections);
                });
                container(context).size(340.0, 190.0).children(|context| {
                    split_view(
                        context,
                        "gallery-split",
                        &mut state.split,
                        SplitOrientation::Horizontal,
                        |context| {
                            text(context, "First panel").font_size(13.0).color(theme.foreground);
                        },
                        |context| {
                            text(context, "Second panel").font_size(13.0).color(theme.foreground);
                        },
                    );
                });
                column(context).gap(4.0).children(|context| {
                    text(context, &format!("Sidebar: {}", state.sidebar_selected))
                        .font_size(12.0)
                        .color(theme.muted);
                    text(context, &format!("Split: {:.0}%", state.split.ratio * 100.0))
                        .font_size(12.0)
                        .color(theme.muted);
                });
            });

            row(context).gap(12.0).children(|context| {
                let sections = [
                    AccordionSection::new("install", "Install", "Run cargo build to install the gallery."),
                    AccordionSection::new("usage", "Usage", "Panels are rebuilt from caller state every frame."),
                ];
                container(context).size(300.0, 150.0).children(|context| {
                    accordion(context, "gallery-accordion", &mut state.accordion, &sections);
                });
                let nodes = [
                    TreeNode::new("root", "Project", 0, true),
                    TreeNode::new("src", "src", 1, false),
                    TreeNode::new("docs", "docs", 0, false),
                ];
                container(context).size(160.0, 140.0).children(|context| {
                    tree(
                        context,
                        "gallery-tree",
                        &mut state.tree_state,
                        &nodes,
                        &mut state.tree_expanded,
                    );
                });
                container(context).size(160.0, 140.0).children(|context| {
                    outline(
                        context,
                        "gallery-outline",
                        &mut state.outline_state,
                        &nodes,
                        &mut state.outline_expanded,
                    );
                });
            });

            row(context).gap(12.0).children(|context| {
                let rows = [
                    ListRow::new("r0", "Inbox"),
                    ListRow::new("r1", "Sent"),
                    ListRow::new("r2", "Drafts"),
                    ListRow::new("r3", "Archive"),
                    ListRow::new("r4", "Spam"),
                    ListRow::new("r5", "Trash"),
                    ListRow::new("r6", "Templates"),
                    ListRow::new("r7", "Notes"),
                ];
                container(context).size(180.0, 140.0).children(|context| {
                    list(context, "gallery-list", &mut state.list, &rows, 28.0, 120.0);
                });
                let cells0 = ["Design", "Figma"];
                let cells1 = ["Engineering", "Rust"];
                let cells2 = ["Docs", "Markdown"];
                let cells3 = ["QA", "Tests"];
                let columns = [
                    TableColumn::new("Area", 140.0),
                    TableColumn::new("Tool", 100.0),
                ];
                let table_rows = [
                    TableRow::new("design", &cells0),
                    TableRow::new("engineering", &cells1),
                    TableRow::new("docs", &cells2),
                    TableRow::new("qa", &cells3),
                ];
                container(context).size(300.0, 140.0).children(|context| {
                    table(
                        context,
                        "gallery-table",
                        &mut state.table,
                        &columns,
                        &table_rows,
                    );
                });
                text(context, "Virtualized list / sortable table")
                    .font_size(12.0)
                    .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                container(context).size(180.0, 40.0).children(|context| {
                    stepper(context, "gallery-stepper", &mut state.stepper, 0, 10, 1);
                });
                container(context).size(130.0, 32.0).children(|context| {
                    rating(context, "gallery-rating", &mut state.rating, 5);
                });
                container(context).size(220.0, 40.0).children(|context| {
                    range_slider(context, "gallery-range", &mut state.range, 0.0, 100.0);
                });
                text(
                    context,
                    &format!(
                        "S{} R{} {:.0}-{:.0}",
                        state.stepper, state.rating, state.range.low, state.range.high
                    ),
                )
                .font_size(12.0)
                .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                text(context, "Number").font_size(12.0).color(theme.muted);
                container(context).size(150.0, 40.0).children(|context| {
                    number_input(
                        context,
                        "gallery-number",
                        &mut state.number_text,
                        NumberInputOptions::default()
                            .allow_negative(true)
                            .decimals(2)
                            .step(1.0),
                    );
                });
                text(context, "Decimal").font_size(12.0).color(theme.muted);
                container(context).size(150.0, 40.0).children(|context| {
                    decimal_input(
                        context,
                        "gallery-decimal",
                        &mut state.decimal_text,
                        DecimalInputOptions {
                            allow_negative: true,
                            decimals: 2,
                        },
                    );
                });
                text(context, &format!("Decimal text: {}", state.decimal_text))
                    .font_size(12.0)
                    .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                text(context, "Search").font_size(12.0).color(theme.muted);
                let mut submitted = false;
                container(context).size(220.0, 40.0).children(|context| {
                    submitted =
                        search_field(context, "gallery-search", &mut state.search_text).submitted;
                });
                if submitted {
                    state.search_status = format!("Submitted: {}", state.search_text);
                }
                text(context, &state.search_status)
                    .font_size(12.0)
                    .color(theme.muted);
                text(context, "Combobox").font_size(12.0).color(theme.muted);
                container(context).size(160.0, 40.0).children(|context| {
                    combobox(
                        context,
                        "gallery-combo",
                        &mut state.combo_query,
                        &mut state.combo,
                        &["Rust", "Ruby", "Go", "JavaScript"],
                    );
                });
            });

            row(context).gap(12.0).children(|context| {
                text(context, "Autocomplete")
                    .font_size(12.0)
                    .color(theme.muted);
                container(context).size(200.0, 40.0).children(|context| {
                    autocomplete(
                        context,
                        "gallery-auto",
                        &mut state.auto_text,
                        &mut state.auto,
                        &["Rust", "Ruby", "Rails", "Go"],
                    );
                });
                text(context, "Tokens").font_size(12.0).color(theme.muted);
                container(context).size(300.0, 90.0).children(|context| {
                    token_field(
                        context,
                        "gallery-tokens",
                        &mut state.tokens,
                        &mut state.token_input,
                    );
                });
            });

            row(context).gap(12.0).children(|context| {
                text(context, "Rename").font_size(12.0).color(theme.muted);
                container(context).size(220.0, 36.0).children(|context| {
                    editable_text(
                        context,
                        "gallery-editable",
                        &mut state.rename,
                        &mut state.rename_text,
                    );
                });
                text(
                    context,
                    "Click the name to edit; Enter commits, Escape cancels.",
                )
                .font_size(12.0)
                .color(theme.muted);
            });

            row(context).gap(8.0).children(|context| {
                if button(context, "Home").clicked {
                    state.router.push("home");
                }
                if button(context, "Detail").clicked {
                    state.router.push("detail");
                }
                if button(context, "Settings").clicked {
                    state.router.push("settings");
                }
                text(context, "Router: push a page, then use back/forward.")
                    .font_size(12.0)
                    .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                text(context, "Router").font_size(12.0).color(theme.muted);
                container(context).size(70.0, 32.0).children(|context| {
                    back_forward_buttons(context, "gallery-history", &mut state.router);
                });
                container(context).size(300.0, 60.0).children(|context| {
                    router(context, "gallery-router", &mut state.router, &pages);
                });
                text(context, &format!("Page: {}", state.router.current()))
                    .font_size(12.0)
                    .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                text(context, "Find").font_size(12.0).color(theme.muted);
                let matches = count_matches(
                    "Native UI controls, UI themes, and UI pages.",
                    &state.find_query,
                );
                container(context).size(320.0, 40.0).children(|context| {
                    find_bar(
                        context,
                        "gallery-find",
                        &mut state.find_query,
                        &mut state.find_current,
                        matches,
                    );
                });
                text(
                    context,
                    &format!("{}/{} matches", state.find_current + 1, matches),
                )
                .font_size(12.0)
                .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                container(context).size(340.0, 160.0).children(|context| {
                    markdown(
                        context,
                        "gallery-markdown",
                        "# Markdown\n\nA paragraph with `code` and [link](https://example.com).\n\n- one\n- two\n\n1. first",
                    );
                });
                container(context).size(300.0, 100.0).children(|context| {
                    rich_text(
                        context,
                        "gallery-rich",
                        "Rich text: **bold**, *italic*, `code`, [link](https://example.com)",
                    );
                });
            });

            row(context).gap(12.0).children(|context| {
                text(context, "GridView").font_size(12.0).color(theme.muted);
                container(context).size(300.0, 160.0).children(|context| {
                    grid_view(
                        context,
                        "gallery-grid",
                        &mut state.grid,
                        24,
                        myrust_core::ui::Size::new(56.0, 44.0),
                        4,
                        6.0,
                        140.0,
                        |context, index| {
                            text(context, &format!("{}", index + 1));
                        },
                    );
                });
                column(context).gap(6.0).children(|context| {
                    text(context, "Meter").font_size(12.0).color(theme.muted);
                    container(context).size(180.0, 10.0).children(|context| {
                        meter(context, "gallery-meter", state.volume, 0.0, 100.0, 33.0, 66.0);
                    });
                    text(
                        context,
                        match match state.volume {
                            value if value < 33.0 => MeterTone::Low,
                            value if value < 66.0 => MeterTone::Medium,
                            _ => MeterTone::High,
                        } {
                            MeterTone::Low => "Meter: low",
                            MeterTone::Medium => "Meter: medium",
                            MeterTone::High => "Meter: high",
                        },
                    )
                    .font_size(12.0)
                    .color(theme.muted);
                    row(context).gap(8.0).children(|context| {
                        avatar(
                            context,
                            "gallery-avatar",
                            "Maya",
                            "MA",
                            44.0,
                            AvatarShape::Circle,
                        );
                        avatar_image(
                            context,
                            "gallery-avatar-image",
                            "Photo",
                            ImageData::new(&IMAGE_PIXELS, 2, 2),
                            44.0,
                            AvatarShape::Rounded,
                        );
                        container(context).size(32.0, 32.0).children(|context| {
                            image(
                                context,
                                "gallery-image",
                                ImageData::new(&IMAGE_PIXELS, 2, 2),
                                32.0,
                                32.0,
                            );
                        });
                    });
                });
            });

            row(context).gap(12.0).children(|context| {
                text(context, "Charts").font_size(12.0).color(theme.muted);
                column(context).gap(4.0).children(|context| {
                    text(context, "Bars").font_size(12.0).color(theme.muted);
                    container(context).size(240.0, 100.0).children(|context| {
                        bar_chart(
                            context,
                            "gallery-bars",
                            &[0.2, 0.55, 0.35, 0.8, 0.6, 1.0],
                            100.0,
                            None,
                            None,
                        );
                    });
                });
                column(context).gap(4.0).children(|context| {
                    text(context, "Line").font_size(12.0).color(theme.muted);
                    container(context).size(240.0, 100.0).children(|context| {
                        line_chart(
                            context,
                            "gallery-line",
                            &[0.1, 0.4, 0.25, 0.7, 0.5, 0.9],
                            100.0,
                            Some(0.0),
                            Some(1.0),
                        );
                    });
                });
                text(context, "Image: RGBA8")
                    .font_size(12.0)
                    .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                text(context, "Icons").font_size(12.0).color(theme.muted);
                icon(context, "gallery-icon-search", "search", 24.0, theme.foreground);
                icon(context, "gallery-icon-check", "check", 24.0, theme.foreground);
                icon(
                    context,
                    "gallery-icon-arrow",
                    "arrow_right",
                    24.0,
                    theme.foreground,
                );
                text(context, "CPU SVG fill/stroke")
                    .font_size(12.0)
                    .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                if button(context, "Toggle target").clicked {
                    state.transition_far = !state.transition_far;
                }
                let target = if state.transition_far {
                    Rect::new(110.0, 10.0, 60.0, 30.0)
                } else {
                    Rect::new(10.0, 10.0, 60.0, 30.0)
                };
                let frame = transition(context, &mut state.transition, target);
                let bounds = frame.bounds;
                container(context).size(200.0, 60.0).draw(vec![
                    DrawCommand::RoundedRect {
                        x: 0.0,
                        y: 0.0,
                        width: 200.0,
                        height: 60.0,
                        radius: Radius::all(8.0),
                        color: theme.surface,
                    },
                    DrawCommand::RoundedRect {
                        x: bounds.x,
                        y: bounds.y,
                        width: bounds.width,
                        height: bounds.height,
                        radius: Radius::all(8.0),
                        color: theme.accent,
                    },
                ]);
                text(
                    context,
                    &format!("Transition: {:.0}%", frame.progress * 100.0),
                )
                .font_size(12.0)
                .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                column(context).gap(4.0).children(|context| {
                    text(context, "Inspector").font_size(12.0).color(theme.muted);
                    let sample = InspectorTree {
                        roots: vec![InspectorItem {
                            id: "window".to_string(),
                            role: Role::Group,
                            label: "window".to_string(),
                            bounds: Rect::new(0.0, 0.0, 320.0, 120.0),
                            paint: PaintSummary {
                                rounded_rects: 2,
                                text: 2,
                                ..PaintSummary::default()
                            },
                            children: vec![InspectorItem {
                                id: "primary".to_string(),
                                role: Role::Button,
                                label: "Primary".to_string(),
                                bounds: Rect::new(12.0, 12.0, 90.0, 28.0),
                                paint: PaintSummary {
                                    rounded_rects: 1,
                                    text: 1,
                                    ..PaintSummary::default()
                                },
                                children: Vec::new(),
                            }],
                        }],
                        total: PaintSummary {
                            rects: 1,
                            rounded_rects: 3,
                            text: 3,
                            ..PaintSummary::default()
                        },
                    };
                    container(context).size(300.0, 90.0).children(|context| {
                        inspector(context, "gallery-inspector", &sample);
                    });
                });

                column(context).gap(6.0).children(|context| {
                    text(context, "Performance").font_size(12.0).color(theme.muted);
                    let stats = context.frame_stats();
                    frame_stats_panel(context, "gallery-frame-stats", stats);
                    context.transform(
                        Transform {
                            translation: Point::new(4.0, 0.0),
                            scale: Point::new(1.15, 1.15),
                            rotation: 0.05,
                        },
                        |context| {
                            button(context, "Transformed hit");
                        },
                    );
                });
            });

            row(context).gap(12.0).children(|context| {
                text(context, "RTL").font_size(12.0).color(theme.muted);
                context.with_direction(Direction::Rtl, |context| {
                    row(context).gap(8.0).children(|context| {
                        badge(context, "One");
                        badge(context, "Two");
                        badge(context, "Three");
                    });
                });
                text(context, "row order and icon mirroring follow Direction")
                    .font_size(12.0)
                    .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                text(context, "Formats").font_size(12.0).color(theme.muted);
                text(
                    context,
                    &format_number(1_234_567.89, Locale::EnUs, 2).unwrap_or_default(),
                )
                .font_size(13.0)
                .color(theme.foreground);
                text(
                    context,
                    &format_date("2024-02-29", Locale::ZhCn, DateStyle::Short)
                        .unwrap_or_default(),
                )
                .font_size(13.0)
                .color(theme.foreground);
                text(context, "en-US number / zh-CN date")
                    .font_size(12.0)
                    .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                if button(context, "Rename control id").clicked {
                    if state.renamed {
                        context.migrate_state("gallery-new-field", "gallery-old-field");
                    } else {
                        context.migrate_state("gallery-old-field", "gallery-new-field");
                    }
                    state.renamed = !state.renamed;
                }
                let field_id = if state.renamed {
                    "gallery-new-field"
                } else {
                    "gallery-old-field"
                };
                container(context).size(220.0, 40.0).children(|context| {
                    text_field(context, field_id, &mut state.migration_text);
                });
                text(
                    context,
                    "Edit the field, then rename its id; text/cursor state migrates.",
                )
                .font_size(12.0)
                .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                text(context, "Link").font_size(12.0).color(theme.muted);
                let response = link(context, "gallery-link", "Read the docs");
                if response.clicked {
                    state.link_status = "Link clicked".to_string();
                }
                text(context, &state.link_status)
                    .font_size(12.0)
                    .color(theme.muted);

                text(context, "Segmented").font_size(12.0).color(theme.muted);
                container(context).size(220.0, 40.0).children(|context| {
                    segmented(
                        context,
                        "gallery-segmented",
                        &mut state.segmented,
                        &["Small", "Medium", "Large"],
                    );
                });
            });

            row(context).gap(12.0).children(|context| {
                text(context, "Toggle").font_size(12.0).color(theme.muted);
                let mut group = std::mem::take(&mut state.toggle_group);
                toggle(
                    context,
                    "gallery-toggle-left",
                    "Left",
                    &mut state.toggle_left,
                    Some(&mut group),
                );
                toggle(
                    context,
                    "gallery-toggle-right",
                    "Right",
                    &mut state.toggle_right,
                    Some(&mut group),
                );
                let group_active = group.active.clone();
                state.toggle_group = group;
                text(
                    context,
                    &format!("Group: {}", group_active.unwrap_or_else(|| "none".to_string())),
                )
                .font_size(12.0)
                .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                text(context, "Toolbar").font_size(12.0).color(theme.muted);
                let items = [
                    ToolbarItem::new("cut", "Cut", 100.0),
                    ToolbarItem::new("copy", "Copy", 100.0),
                    ToolbarItem::new("paste", "Paste", 100.0),
                ];
                let mut selected = None;
                container(context).size(300.0, 40.0).children(|context| {
                    selected = toolbar(
                        context,
                        "gallery-toolbar",
                        &mut state.toolbar,
                        &items,
                        300.0,
                        |context, index| {
                            button(context, items[index].label);
                        },
                    )
                    .selected;
                });
                if let Some(index) = selected {
                    state.link_status = format!("Toolbar overflow: {}", items[index].label);
                }
                text(context, "Overflow menu holds tail items")
                    .font_size(12.0)
                    .color(theme.muted);
            });

            row(context).gap(12.0).children(|context| {
                column(context).gap(4.0).children(|context| {
                    text(context, "Calendar").font_size(12.0).color(theme.muted);
                    container(context).size(270.0, 260.0).children(|context| {
                        calendar(context, "gallery-calendar", &mut state.calendar);
                    });
                });
                column(context).gap(8.0).children(|context| {
                    text(context, "Date").font_size(12.0).color(theme.muted);
                    container(context).size(240.0, 40.0).children(|context| {
                        date_input(
                            context,
                            "gallery-date",
                            &mut state.date_text,
                            &mut state.date_input,
                        );
                    });
                    text(context, "Time").font_size(12.0).color(theme.muted);
                    container(context).size(240.0, 36.0).children(|context| {
                        time_input(context, "gallery-time", &mut state.time_text);
                    });
                    text(context, "Color").font_size(12.0).color(theme.muted);
                    row(context).gap(8.0).children(|context| {
                        let mut color = state.color.unwrap_or(Color::rgb(34, 211, 238));
                        color_picker(
                            context,
                            "gallery-color",
                            &mut color,
                            &mut state.color_picker,
                        );
                        state.color = Some(color);
                        text(
                            context,
                            &format!("{} / {} / {}", state.date_text, state.time_text, color.to_hex()),
                        )
                        .font_size(12.0)
                        .color(theme.muted);
                    });
                });
            });

            row(context).gap(12.0).children(|context| {
                column(context).gap(4.0).children(|context| {
                    text(context, "Code").font_size(12.0).color(theme.muted);
                    container(context).size(330.0, 140.0).children(|context| {
                        let code = "fn main() {\n    let value = 42;\n    // demo\n    println!(\"hello\");\n}";
                        code_editor(context, "gallery-code", code, &mut state.code);
                    });
                });
                column(context).gap(4.0).children(|context| {
                    text(context, "Diff").font_size(12.0).color(theme.muted);
                    container(context).size(340.0, 140.0).children(|context| {
                        let old = "fn main() {\n    let x = 1;\n    old_call();\n}";
                        let new = "fn main() {\n    let x = 2;\n    new_call();\n}";
                        diff_viewer(
                            context,
                            "gallery-diff",
                            old,
                            new,
                            &mut state.diff,
                        );
                    });
                });
            });

            row(context).gap(12.0).children(|context| {
                column(context).gap(6.0).children(|context| {
                    text(context, "Drag").font_size(12.0).color(theme.muted);
                    row(context).gap(8.0).children(|context| {
                        drag_source(
                            context,
                            "gallery-drag",
                            "Drag card",
                            "card-42",
                            &mut state.drag,
                        );
                        let target = drop_target(
                            context,
                            "gallery-drop",
                            "Drop here",
                            &mut state.drag,
                        );
                        if let Some(drop) = target.dropped {
                            state.drop_status =
                                format!("{} -> {}", drop.payload, drop.target);
                        }
                    });
                    text(context, &state.drop_status)
                        .font_size(12.0)
                        .color(theme.muted);
                });
                column(context).gap(4.0).children(|context| {
                    text(context, "Reorder").font_size(12.0).color(theme.muted);
                    let rows = state
                        .reorder_rows
                        .iter()
                        .map(|label| ListRow::new(label, label))
                        .collect::<Vec<_>>();
                    let result = reorder_list(
                        context,
                        "gallery-reorder",
                        &mut state.reorder,
                        &rows,
                        26.0,
                    );
                    if result.dropped {
                        if let (Some(from), Some(to)) = (result.from, result.to) {
                            reorder(&mut state.reorder_rows, from, to);
                        }
                    }
                });
                column(context).gap(4.0).children(|context| {
                    text(context, "TextInput").font_size(12.0).color(theme.muted);
                    let query = context.text_input_state("gallery-primitives");
                    let detail = query.map_or_else(
                        || "cursor -".to_string(),
                        |state| {
                            format!(
                                "cursor {} selection {:?} @ ({:.0},{:.0})",
                                state.cursor, state.selection, state.cursor_x, state.cursor_y
                            )
                        },
                    );
                    container(context).size(220.0, 40.0).children(|context| {
                        text_field(context, "gallery-primitives", &mut state.primitive_text);
                    });
                    text(context, &detail)
                        .font_size(12.0)
                        .color(theme.muted);
                });
            });

            row(context).gap(8.0).children(|context| {
                if button(context, "Show toast").clicked {
                    state
                        .toasts
                        .push(context, Toast::new("Project saved").action("Undo"));
                }
                if button(context, "Open dialog").clicked {
                    state.dialog_open = true;
                }
                if button(context, "Delete project").clicked {
                    state.alert_open = true;
                }
                text(context, &state.toast_status)
                    .font_size(12.0)
                    .color(theme.muted);
            });

            form_field(
                context,
                "gallery-email",
                "Email",
                Some("Used for notifications only."),
                if state.email.is_empty() || !state.email.contains('@') {
                    Some("Enter a valid email address")
                } else {
                    None
                },
                |context| {
                    text_field(context, "email", &mut state.email);
                },
            );

            text(
                context,
                &format!("{}  |  {}", state.dialog_status, state.alert_status),
            )
            .font_size(12.0)
            .color(theme.muted);

            text(context, "Name").font_size(12.0).color(theme.muted);
            text_field(context, "name", &mut state.name);

            let mut drawing = vec![
                DrawCommand::RoundedRect {
                    x: 0.0,
                    y: 0.0,
                    width: 560.0,
                    height: 120.0,
                    radius: Radius::all(14.0),
                    color: theme.surface,
                },
                DrawCommand::Circle {
                    x: 70.0,
                    y: 60.0,
                    radius: 34.0,
                    color: theme.accent,
                },
                DrawCommand::Line {
                    x1: 130.0,
                    y1: 60.0,
                    x2: 520.0,
                    y2: 60.0,
                    width: 2.0,
                    color: theme.border,
                },
                DrawCommand::Text {
                    x: 130.0,
                    y: 34.0,
                    text: "Display list: rounded rect, circle, line, text".to_string(),
                    font_size: 14.0,
                    weight: 500,
                    color: theme.foreground,
                    max_width: Some(400.0),
                    alignment: TextAlignment::Start,
                },
            ];
            drawing.push(DrawCommand::Text {
                x: 130.0,
                y: 70.0,
                text: format!("{:.0}% accent hue", 210.0 - state.volume * 1.8),
                font_size: 12.0,
                weight: 400,
                color: theme.muted,
                max_width: None,
                alignment: TextAlignment::Start,
            });
            container(context).size(560.0, 120.0).draw(drawing);

            text(
                context,
                "Font sizes: 26 bold heading, 14 body, 12 muted caption.",
            )
            .font_size(12.0)
            .color(theme.muted);

            let toast_response = toasts(context, "gallery-toasts", &mut state.toasts);
            if let Some(event) = toast_response.events.last() {
                state.toast_status = match event.kind {
                    ToastDismissal::Action => format!(
                        "Toast action: {}",
                        event.action.as_deref().unwrap_or_default()
                    ),
                    ToastDismissal::Close => "Toast closed".to_string(),
                    ToastDismissal::Timeout => "Toast expired".to_string(),
                };
            }

            if state.dialog_open {
                let mut open = state.dialog_open;
                let mut action = DialogAction::None;
                let response = dialog(context, "gallery-dialog", &mut open, 420.0, |context| {
                    let theme = context.theme().clone();
                    context.text("Rename project").font_size(18.0).bold();
                    context
                        .text("Dialog content is a caller closure; overlay owns focus, blocking, and Escape.")
                        .font_size(12.0)
                        .color(theme.muted);
                    context.text_field("dialog-name", &mut state.name);
                    context.row().gap(8.0).children(|context| {
                        context.spacer();
                        if context
                            .id("gallery-dialog-cancel")
                            .button("Cancel")
                            .clicked
                        {
                            action = DialogAction::Cancel;
                        }
                        if context
                            .id("gallery-dialog-confirm")
                            .button("Save")
                            .clicked
                        {
                            action = DialogAction::Confirm;
                        }
                    });
                    action
                });
                state.dialog_open = open;
                if response.confirmed {
                    state.dialog_status = "Dialog: saved".to_string();
                } else if response.cancelled {
                    state.dialog_status = "Dialog: cancelled".to_string();
                } else if response.closed {
                    state.dialog_status = "Dialog: closed".to_string();
                }
            }

            if state.alert_open {
                let mut open = state.alert_open;
                let response = alert_dialog(
                    context,
                    "gallery-alert",
                    &mut open,
                    "Delete project?",
                    "This cannot be undone. The alert is modal and centers itself.",
                    "Delete",
                    "Cancel",
                );
                state.alert_open = open;
                if response.confirmed {
                    state.alert_status = "Alert: deleted".to_string();
                } else if response.cancelled {
                    state.alert_status = "Alert: cancelled".to_string();
                }
            }
        });
}

fn main() -> Result<()> {
    let mut app = App::new("myrust-gallery")?;
    let state = Rc::new(RefCell::new(State {
        volume: 40.0,
        switch_on: true,
        toast_status: "Toast: idle".to_string(),
        dialog_status: "Dialog: idle".to_string(),
        alert_status: "Alert: idle".to_string(),
        breadcrumb_status: "Breadcrumb: idle".to_string(),
        sidebar_selected: "inbox".to_string(),
        accordion: AccordionState::new(AccordionMode::Single),
        stepper: 3,
        rating: 3,
        range: RangeSliderState::new(25.0, 75.0),
        number_text: "42.00".to_string(),
        decimal_text: "12.34".to_string(),
        search_text: "native ui".to_string(),
        search_status: "Submitted: native ui".to_string(),
        combo_query: "Rust".to_string(),
        tokens: vec!["rust".to_string(), "ui".to_string()],
        rename_text: "Project name".to_string(),
        migration_text: "migrate me".to_string(),
        date_text: "2024-02-29".to_string(),
        time_text: "09:30".to_string(),
        reorder_rows: vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()],
        primitive_text: "hello".to_string(),
        router: RouterState::new("home"),
        find_query: "ui".to_string(),
        ..State::default()
    }));
    let view_state = Rc::clone(&state);

    let native = NativeUi::new(move |context| {
        let mut state = view_state.borrow_mut();
        view(context, &mut state);
    });

    app.on_ready(move |context| {
        context.create_window(WindowConfig {
            title: "Gallery".to_string(),
            width: 740.0,
            height: 3880.0,
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        Ok(())
    });

    app.run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use myrust_core::ui::{Role, Size};

    #[test]
    fn gallery_builds_all_batch_controls_through_nine() {
        let state = Rc::new(RefCell::new(State {
            volume: 40.0,
            switch_on: true,
            router: RouterState::new("home"),
            find_query: "ui".to_string(),
            ..State::default()
        }));
        let view_state = Rc::clone(&state);
        let ui = NativeUi::new(move |context| {
            let mut state = view_state.borrow_mut();
            view(context, &mut state);
        });
        let frame = ui.render(Size::new(740.0, 3880.0)).unwrap();
        for role in [
            Role::Switch,
            Role::RadioGroup,
            Role::ComboBox,
            Role::TabList,
            Role::ProgressBar,
            Role::Spinner,
            Role::Badge,
            Role::Disclosure,
        ] {
            assert!(
                frame.access_nodes.iter().any(|node| node.role == role),
                "missing {role:?} in gallery"
            );
        }
        for label in [
            "Open popover",
            "Actions",
            "Hover or focus for tooltip",
            "Show toast",
            "Open dialog",
            "Delete project",
            "Grid",
            "First panel",
            "Second panel",
            "Batch 3a",
            "Install",
            "Project",
            "Design",
            "Number",
            "Decimal",
            "Search",
            "Combobox",
            "Autocomplete",
            "Tokens",
            "Rename",
            "Router",
            "Find",
            "Markdown",
            "GridView",
            "Meter",
            "Charts",
            "Icons",
            "Inspector",
            "Performance",
            "Transformed hit",
            "RTL",
            "Formats",
            "Link",
            "Toggle",
            "Segmented",
            "Toolbar",
            "Calendar",
            "Date",
            "Time",
            "Color",
            "Code",
            "Diff",
            "Drag",
            "Reorder",
            "TextInput",
        ] {
            assert!(
                frame.display.commands.iter().any(
                    |command| matches!(command, DrawCommand::Text { text, .. } if text == label)
                ),
                "missing gallery control {label}"
            );
        }
        assert!(frame.access_nodes.iter().any(|node| {
            node.role == Role::Group
                && node.label == "Email"
                && node.value.as_deref() == Some("error")
        }));
        assert!(frame.display.commands.iter().any(
            |command| matches!(command, DrawCommand::Text { text, .. } if text == "Enter a valid email address")
        ));
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Slider && node.label == "split view"));
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Group && node.label == "gallery-breadcrumbs"));
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Group && node.label == "grid"));
        assert!(frame.access_nodes.iter().any(|node| {
            node.role == Role::Option
                && node.label == "Inbox"
                && node.value.as_deref() == Some("selected")
        }));
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Disclosure && node.label == "Install"));
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| node.role == Role::Button && node.label == "Area"));
        assert!(frame
            .access_nodes
            .iter()
            .any(|node| { node.role == Role::Group && node.label == "gallery-outline" }));
        let content_bottom = frame
            .layout
            .iter()
            .map(|result| result.y + result.height)
            .fold(0.0_f32, f32::max);
        assert!(
            content_bottom <= 3880.0,
            "gallery content overflows the window: {content_bottom}"
        );
    }
}
