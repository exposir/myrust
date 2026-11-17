//! Showcase application — sidebar navigation, an overview dashboard with a
//! live chart, a filterable and sortable project table, and a settings form.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use myrust_core::app::{App, WindowConfig, WindowContent};
use myrust_core::ui::{
    avatar, button_primary, column, container, line_chart, row, segmented, sidebar, slider,
    spacer, switch, table, text, text_field, text_field_placeholder, toasts, AlignItems,
    AvatarShape, Context, NativeUi, SidebarItem, SidebarSection, SortDirection, TableColumn,
    TableRow, TableState, Theme, ToastQueue,
};

struct State {
    page: String,
    search: String,
    table: TableState,
    name: String,
    notifications: bool,
    autosave: bool,
    density: usize,
    scale: f32,
    saves: u32,
    toasts: ToastQueue,
    tick: u64,
}

impl State {
    fn new() -> Self {
        Self {
            page: "Overview".to_string(),
            search: String::new(),
            table: TableState::default(),
            name: "Ada Lovelace".to_string(),
            notifications: true,
            autosave: false,
            density: 1,
            scale: 100.0,
            saves: 0,
            toasts: ToastQueue::new(),
            tick: 0,
        }
    }
}

/// The rows the project table lists, in creation order (the order used until
/// the user sorts a column).
const PROJECTS: [(&str, &str, &str, &str); 6] = [
    ("Atlas", "Linus", "Today", "Active"),
    ("Beacon", "Grace", "2d ago", "Paused"),
    ("Canvas", "Alan", "1w ago", "Active"),
    ("Delta", "Margaret", "3d ago", "Draft"),
    ("Echo", "Katherine", "2w ago", "Archived"),
    ("Flux", "Ada", "5d ago", "Active"),
];

const ACTIVITY: [(&str, &str); 4] = [
    ("Atlas deployed to production", "2m ago"),
    ("Grace invited 3 teammates", "1h ago"),
    ("Canvas weekly report generated", "3h ago"),
    ("Nightly backup completed", "Yesterday"),
];

const NARROW_BREAKPOINT: f32 = 640.0;
const SIDEBAR_WIDTH: f32 = 180.0;

fn view(context: &mut Context, state: &mut State) {
    // The overview chart advances every frame, so the window keeps
    // repainting while it is open.
    context.request_animation_frame();
    state.tick = state.tick.wrapping_add(1);
    let theme = context.theme().clone();
    let size = context.available_size();
    let is_narrow = size.width < NARROW_BREAKPOINT;

    let pages = [
        SidebarItem::new("Overview", "Overview").icon("home"),
        SidebarItem::new("Projects", "Projects").icon("grid").badge("6"),
        SidebarItem::new("Settings", "Settings").icon("settings"),
    ];
    let sections = [SidebarSection::new("Workspace", &pages)];

    if is_narrow {
        let content_width = (size.width - 24.0).max(1.0);
        container(context)
            .size(size.width, size.height)
            .padding_each(12.0, 12.0, 12.0, 12.0)
            .children(|context| {
                column(context).gap(14.0).children(|context| {
                    let mut nav_index = match state.page.as_str() {
                        "Projects" => 1,
                        "Settings" => 2,
                        _ => 0,
                    };
                    let nav_resp = segmented(
                        context,
                        "showcase-nav-narrow",
                        &mut nav_index,
                        &["Overview", "Projects", "Settings"],
                    );
                    if nav_resp.changed {
                        state.page = match nav_index {
                            1 => "Projects".to_string(),
                            2 => "Settings".to_string(),
                            _ => "Overview".to_string(),
                        };
                    }

                    row(context).gap(12.0).align(AlignItems::Center).children(|context| {
                        text(context, state.page.as_str()).font_size(20.0).bold();
                        spacer(context);
                        if state.page != "Settings"
                            && button_primary(context, "New project").clicked
                        {
                            state.toasts.push_message(context, "New project created");
                        }
                    });

                    match state.page.as_str() {
                        "Projects" => projects_view(context, state, content_width, true),
                        "Settings" => settings_view(context, state, content_width, true),
                        _ => overview(context, &theme, state.tick, content_width, true),
                    }
                });
            });
    } else {
        row(context)
            .size(size.width, size.height)
            .align(AlignItems::Stretch)
            .children(|context| {
                container(context)
                    .width(SIDEBAR_WIDTH)
                    .padding_each(16.0, 14.0, 16.0, 14.0)
                    .background(theme.surface, 0.0)
                    .children(|context| {
                        column(context).gap(20.0).children(|context| {
                            text(context, "Acme Studio").font_size(16.0).bold();
                            sidebar(context, "showcase-nav", &mut state.page, &sections);
                            spacer(context);
                            row(context).gap(8.0).align(AlignItems::Center).children(|context| {
                                avatar(
                                    context,
                                    "showcase-user",
                                    "Ada Lovelace",
                                    None,
                                    None,
                                    0.0,
                                    AvatarShape::Circle,
                                );
                                column(context).gap(1.0).children(|context| {
                                    text(context, "Ada Lovelace").font_size(12.0).bold();
                                    text(context, "Admin").font_size(11.0).color(theme.muted);
                                });
                            });
                        });
                    });

                let content_width = (size.width - SIDEBAR_WIDTH - 48.0).max(200.0);
                container(context)
                    .width((size.width - SIDEBAR_WIDTH).max(1.0))
                    .padding_each(20.0, 24.0, 16.0, 24.0)
                    .children(|context| {
                        column(context).gap(18.0).children(|context| {
                            row(context).gap(12.0).align(AlignItems::Center).children(|context| {
                                text(context, state.page.as_str()).font_size(24.0).bold();
                                spacer(context);
                                if state.page != "Settings"
                                    && button_primary(context, "New project").clicked
                                {
                                    state.toasts.push_message(context, "New project created");
                                }
                            });

                            match state.page.as_str() {
                                "Projects" => projects_view(context, state, content_width, false),
                                "Settings" => settings_view(context, state, content_width, false),
                                _ => overview(context, &theme, state.tick, content_width, false),
                            }
                        });
                    });
            });
    }

    toasts(context, "showcase-toasts", &mut state.toasts);
}

fn overview(context: &mut Context, theme: &Theme, tick: u64, content_width: f32, is_narrow: bool) {
    let stats = [
        ("24", "Active projects", "+3 this week"),
        ("8", "Due this week", "-1 vs last"),
        ("73%", "Sprint complete", "+12%"),
    ];

    if is_narrow {
        column(context).gap(8.0).children(|context| {
            for (value, label, trend) in stats {
                container(context)
                    .width(content_width)
                    .height(56.0)
                    .padding_each(8.0, 12.0, 8.0, 12.0)
                    .background(theme.surface, 8.0)
                    .border(theme.border, 8.0)
                    .children(|context| {
                        row(context).gap(12.0).align(AlignItems::Center).children(|context| {
                            text(context, value).font_size(20.0).bold().color(theme.accent);
                            column(context).gap(1.0).children(|context| {
                                text(context, label).font_size(11.0).color(theme.muted);
                                text(context, trend).font_size(10.0).color(theme.muted);
                            });
                        });
                    });
            }
        });
    } else {
        row(context).gap(12.0).children(|context| {
            for (value, label, trend) in stats {
                container(context)
                    .fill_parent()
                    .height(78.0)
                    .padding(12.0)
                    .background(theme.surface, 10.0)
                    .border(theme.border, 10.0)
                    .children(|context| {
                        column(context).gap(1.0).children(|context| {
                            text(context, value).font_size(23.0).bold().color(theme.accent);
                            text(context, label).font_size(12.0).color(theme.muted);
                            text(context, trend).font_size(11.0).color(theme.muted);
                        });
                    });
            }
        });
    }

    let values: Vec<f32> = (0..30)
        .map(|i| {
            let phase = (tick as f32 * 0.05 + i as f32 * 0.24).sin();
            (0.5 + 0.34 * phase).clamp(0.0, 1.0)
        })
        .collect();
    container(context)
        .width(content_width)
        .height(if is_narrow { 100.0 } else { 120.0 })
        .padding(10.0)
        .background(theme.surface, 10.0)
        .border(theme.border, 10.0)
        .children(|context| {
            line_chart(
                context,
                "showcase-chart",
                &values,
                if is_narrow { 80.0 } else { 100.0 },
                Some(0.0),
                Some(1.0),
            );
        });

    text(context, "Recent activity").font_size(if is_narrow { 13.0 } else { 14.0 }).bold();
    column(context).gap(8.0).children(|context| {
        for (event, when) in ACTIVITY {
            row(context).gap(8.0).align(AlignItems::Center).children(|context| {
                text(context, event).font_size(if is_narrow { 12.0 } else { 13.0 });
                spacer(context);
                text(context, when).font_size(if is_narrow { 11.0 } else { 12.0 }).color(theme.muted);
            });
        }
    });
}

fn projects_view(context: &mut Context, state: &mut State, content_width: f32, is_narrow: bool) {
    let theme = context.theme().clone();
    let query = state.search.to_lowercase();
    let mut filtered: Vec<&(&str, &str, &str, &str)> = PROJECTS
        .iter()
        .filter(|(project, owner, _, status)| {
            query.is_empty()
                || project.to_lowercase().contains(&query)
                || owner.to_lowercase().contains(&query)
                || status.to_lowercase().contains(&query)
        })
        .collect();
    if let Some(column) = state.table.sort_column {
        let descending = state.table.sort_direction == SortDirection::Descending;
        filtered.sort_by(|a, b| {
            let (left, right) = match column {
                0 => (a.0, b.0),
                1 => if is_narrow { (a.3, b.3) } else { (a.1, b.1) },
                2 => (a.2, b.2),
                _ => (a.3, b.3),
            };
            let order = left.cmp(right);
            if descending { order.reverse() } else { order }
        });
    }

    row(context).gap(10.0).align(AlignItems::Center).children(|context| {
        container(context).width(if is_narrow { (content_width - 120.0).max(140.0) } else { 240.0 }).children(|context| {
            text_field_placeholder(
                context,
                "showcase-search",
                &mut state.search,
                "Filter projects…",
            );
        });
        spacer(context);
        text(
            context,
            &format!("{} of {} projects", filtered.len(), PROJECTS.len()),
        )
        .font_size(12.0)
        .color(theme.muted);
    });

    if is_narrow {
        let project_width = (content_width * 0.62).max(120.0);
        let status_width = (content_width - project_width).max(70.0);
        let columns = [
            TableColumn::new("Project", project_width),
            TableColumn::new("Status", status_width),
        ];
        let cells: Vec<[&str; 2]> = filtered
            .iter()
            .map(|(project, _, _, status)| [*project, *status])
            .collect();
        let rows: Vec<TableRow<'_>> = filtered
            .iter()
            .zip(cells.iter())
            .map(|((project, _, _, _), row_cells)| TableRow::new(*project, row_cells))
            .collect();
        table(context, "showcase-table", &mut state.table, &columns, &rows);
    } else {
        let project_width = (content_width * 0.34).max(80.0);
        let owner_width = (content_width * 0.22).max(64.0);
        let updated_width = (content_width * 0.22).max(76.0);
        let status_width = (content_width - project_width - owner_width - updated_width).max(70.0);
        let columns = [
            TableColumn::new("Project", project_width),
            TableColumn::new("Owner", owner_width),
            TableColumn::new("Updated", updated_width),
            TableColumn::new("Status", status_width),
        ];
        let cells: Vec<[&str; 4]> = filtered
            .iter()
            .map(|(project, owner, updated, status)| [*project, *owner, *updated, *status])
            .collect();
        let rows: Vec<TableRow<'_>> = filtered
            .iter()
            .zip(cells.iter())
            .map(|((project, _, _, _), row_cells)| TableRow::new(*project, row_cells))
            .collect();
        table(context, "showcase-table", &mut state.table, &columns, &rows);
    }
}

fn settings_view(context: &mut Context, state: &mut State, content_width: f32, is_narrow: bool) {
    let theme = context.theme().clone();
    if is_narrow {
        column(context).gap(14.0).children(|context| {
            column(context).gap(4.0).children(|context| {
                text(context, "Display name").font_size(12.0).color(theme.muted);
                container(context).width(content_width).children(|context| {
                    text_field(context, "showcase-name", &mut state.name);
                });
            });
            row(context).gap(12.0).align(AlignItems::Center).children(|context| {
                text(context, "Notifications").font_size(12.0).color(theme.muted);
                spacer(context);
                switch(context, "showcase-notifications", &mut state.notifications);
            });
            row(context).gap(12.0).align(AlignItems::Center).children(|context| {
                text(context, "Auto-save").font_size(12.0).color(theme.muted);
                spacer(context);
                switch(context, "showcase-autosave", &mut state.autosave);
            });
            column(context).gap(4.0).children(|context| {
                text(context, "Density").font_size(12.0).color(theme.muted);
                container(context).width(content_width).children(|context| {
                    segmented(
                        context,
                        "showcase-density",
                        &mut state.density,
                        &["Compact", "Comfortable"],
                    );
                });
            });
            column(context).gap(4.0).children(|context| {
                row(context).gap(8.0).align(AlignItems::Center).children(|context| {
                    text(context, "Interface scale").font_size(12.0).color(theme.muted);
                    spacer(context);
                    text(context, &format!("{:.0}%", state.scale))
                        .font_size(12.0)
                        .color(theme.muted);
                });
                container(context).width(content_width).children(|context| {
                    slider(context, "showcase-scale", &mut state.scale, 80.0, 140.0);
                });
            });
            row(context).gap(10.0).align(AlignItems::Center).children(|context| {
                if button_primary(context, "Save changes").clicked {
                    state.saves += 1;
                    state.toasts.push_message(context, "Settings saved");
                }
                text(context, &format!("Saved {} time(s)", state.saves))
                    .font_size(12.0)
                    .color(theme.muted);
            });
        });
    } else {
        column(context).gap(16.0).children(|context| {
            row(context).gap(12.0).align(AlignItems::Center).children(|context| {
                container(context).width(140.0).children(|context| {
                    text(context, "Display name").font_size(13.0).color(theme.muted);
                });
                container(context).width(260.0).children(|context| {
                    text_field(context, "showcase-name", &mut state.name);
                });
            });
            row(context).gap(12.0).align(AlignItems::Center).children(|context| {
                container(context).width(140.0).children(|context| {
                    text(context, "Notifications").font_size(13.0).color(theme.muted);
                });
                switch(context, "showcase-notifications", &mut state.notifications);
            });
            row(context).gap(12.0).align(AlignItems::Center).children(|context| {
                container(context).width(140.0).children(|context| {
                    text(context, "Auto-save").font_size(13.0).color(theme.muted);
                });
                switch(context, "showcase-autosave", &mut state.autosave);
            });
            row(context).gap(12.0).align(AlignItems::Center).children(|context| {
                container(context).width(140.0).children(|context| {
                    text(context, "Density").font_size(13.0).color(theme.muted);
                });
                container(context).width(260.0).children(|context| {
                    segmented(
                        context,
                        "showcase-density",
                        &mut state.density,
                        &["Compact", "Comfortable"],
                    );
                });
            });
            row(context).gap(12.0).align(AlignItems::Center).children(|context| {
                container(context).width(140.0).children(|context| {
                    text(context, "Interface scale").font_size(13.0).color(theme.muted);
                });
                container(context).width(220.0).children(|context| {
                    slider(context, "showcase-scale", &mut state.scale, 80.0, 140.0);
                });
                text(context, &format!("{:.0}%", state.scale))
                    .font_size(12.0)
                    .color(theme.muted);
            });
            row(context).gap(10.0).align(AlignItems::Center).children(|context| {
                if button_primary(context, "Save changes").clicked {
                    state.saves += 1;
                    state.toasts.push_message(context, "Settings saved");
                }
                text(context, &format!("Saved {} time(s)", state.saves))
                    .font_size(12.0)
                    .color(theme.muted);
            });
        });
    }
}

fn main() -> Result<()> {
    let mut app = App::new("myrust-showcase")?;

    let state = Rc::new(RefCell::new(State::new()));
    let view_state = Rc::clone(&state);

    let native = NativeUi::new(move |context| {
        let mut state = view_state.borrow_mut();
        view(context, &mut state);
    });

    app.on_ready(move |context| {
        context.create_window(WindowConfig {
            title: "Showcase".to_string(),
            width: 920.0,
            height: 720.0,
            content: Some(WindowContent::NativeUi(native.clone())),
            ..WindowConfig::default()
        })?;
        Ok(())
    });

    app.run()
}
