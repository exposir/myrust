//! System WebView regions inside an immediate-mode native UI.
//!
//! A [`WebViewRegion`] is layout space that a host may cover with a native
//! child WebView. The UI crate owns only the per-frame geometry and
//! configuration; it never creates a platform view and never emits a
//! [`DrawCommand`](crate::DrawCommand) for the region.

use serde::{Deserialize, Serialize};

use super::paint::Rect;

/// Content loaded by a region WebView.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebViewSource {
    /// Navigate to a URL.
    Url(String),
    /// Render an HTML document directly from a string.
    Html(String),
}

impl Default for WebViewSource {
    fn default() -> Self {
        Self::Url("about:blank".to_string())
    }
}

/// Navigation policy applied by the host WebView.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebViewNavigationPolicy {
    /// Allow every navigation.
    #[default]
    AllowAll,
    /// Block every navigation, including the initial document.
    DenyAll,
    /// Allow `about:blank` and hosts in this list.
    ///
    /// A listed host also matches its subdomains.
    AllowHosts(Vec<String>),
}

/// Permission policy applied by the host WebView.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebViewPermissionPolicy {
    /// Use the platform WebView's own prompt and persistence behavior.
    #[default]
    PlatformDefault,
    /// Deny every permission request.
    DenyAll,
    /// Allow every permission request.
    AllowAll,
}

/// Configuration for one embedded WebView region.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebViewOptions {
    /// Initial URL or inline HTML.
    pub source: WebViewSource,
    /// Scripts installed before the page's own scripts run.
    pub initialization_scripts: Vec<String>,
    /// Requests a transparent page background where the platform supports it.
    pub transparent: bool,
    /// Navigation policy for the region.
    pub navigation: WebViewNavigationPolicy,
    /// Permission policy for the region.
    pub permissions: WebViewPermissionPolicy,
}

/// One WebView region in the frame that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct WebViewRegion {
    /// Stable application-defined view id.
    pub id: String,
    /// Logical rectangle occupied by the region, with ancestor scroll offsets
    /// applied.
    pub bounds: Rect,
    /// Logical rectangle the native view may paint into.
    ///
    /// This is the window viewport intersected with every scroll viewport and
    /// clipping container above the region.
    pub clip: Rect,
    /// Physical-pixel region bounds, rounded to the nearest device pixel.
    pub physical_bounds: Rect,
    /// Physical-pixel clip rectangle, rounded to the nearest device pixel.
    pub physical_clip: Rect,
    /// Whether the region has a non-empty visible intersection.
    pub visible: bool,
    /// Stacking order among this frame's WebView regions.
    ///
    /// Higher values sit above lower ones. Native child views always sit
    /// above the GPU/CPU native UI pixels regardless of this value.
    pub z_order: u32,
    /// Region configuration.
    pub options: WebViewOptions,
}

impl WebViewRegion {
    /// Returns whether a logical point is inside the region's visible area.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        if !self.visible {
            return false;
        }
        let visible = intersect(self.bounds, self.clip);
        x >= visible.x && y >= visible.y && x <= visible.right() && y <= visible.bottom()
    }
}

/// Returns the intersection of two rectangles, or a zero-size rectangle when
/// they do not overlap.
pub(crate) fn intersect(left: Rect, right: Rect) -> Rect {
    let x = left.x.max(right.x);
    let y = left.y.max(right.y);
    let right_edge = left.right().min(right.right());
    let bottom_edge = left.bottom().min(right.bottom());
    Rect::new(x, y, (right_edge - x).max(0.0), (bottom_edge - y).max(0.0))
}

/// Converts a logical rectangle to physical pixels.
pub(crate) fn physical_rect(rect: Rect, scale: f32) -> Rect {
    let scale = if scale.is_finite() {
        scale.clamp(1.0, 4.0)
    } else {
        1.0
    };
    let snap = |value: f32| (value * scale).round();
    Rect::new(
        snap(rect.x),
        snap(rect.y),
        snap(rect.width),
        snap(rect.height),
    )
}

/// Builder returned by [`Context::web_view`](crate::Context::web_view).
pub struct WebViewBuilder<'a> {
    ctx: &'a mut super::context::Context,
    node_id: u64,
    index: usize,
}

impl<'a> WebViewBuilder<'a> {
    pub(crate) fn new(ctx: &'a mut super::context::Context, node_id: u64, index: usize) -> Self {
        Self {
            ctx,
            node_id,
            index,
        }
    }

    /// Sets the initial URL.
    pub fn url(self, url: impl Into<String>) -> Self {
        self.ctx.web_view_options_mut(self.index).source = WebViewSource::Url(url.into());
        self
    }

    /// Sets the initial inline HTML document.
    pub fn html(self, html: impl Into<String>) -> Self {
        self.ctx.web_view_options_mut(self.index).source = WebViewSource::Html(html.into());
        self
    }

    /// Appends a script that runs before the page's own scripts.
    pub fn initialization_script(self, script: impl Into<String>) -> Self {
        self.ctx
            .web_view_options_mut(self.index)
            .initialization_scripts
            .push(script.into());
        self
    }

    /// Requests a transparent page background.
    pub fn transparent(self, transparent: bool) -> Self {
        self.ctx.web_view_options_mut(self.index).transparent = transparent;
        self
    }

    /// Sets the navigation policy for the region.
    pub fn navigation(self, policy: WebViewNavigationPolicy) -> Self {
        self.ctx.web_view_options_mut(self.index).navigation = policy;
        self
    }

    /// Sets the permission policy for the region.
    pub fn permissions(self, policy: WebViewPermissionPolicy) -> Self {
        self.ctx.web_view_options_mut(self.index).permissions = policy;
        self
    }

    /// Sets the logical size used by the layout engine.
    pub fn size(self, width: f32, height: f32) -> Self {
        self.ctx
            .set_web_view_size(self.node_id, Some(width.max(0.0)), Some(height.max(0.0)));
        self
    }

    /// Sets the logical width used by the layout engine.
    pub fn width(self, width: f32) -> Self {
        self.ctx
            .set_web_view_size(self.node_id, Some(width.max(0.0)), None);
        self
    }

    /// Sets the logical height used by the layout engine.
    pub fn height(self, height: f32) -> Self {
        self.ctx
            .set_web_view_size(self.node_id, None, Some(height.max(0.0)));
        self
    }

    /// Makes the region grow along its parent's main axis.
    pub fn flex_grow(self, grow: f32) -> Self {
        self.ctx.set_web_view_flex_grow(self.node_id, grow.max(0.0));
        self
    }

    /// Shows or hides the native view without removing its layout space.
    pub fn visible(self, visible: bool) -> Self {
        self.ctx.set_web_view_visible(self.index, visible);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NativeUi, ScrollStyle, Size};

    #[test]
    fn web_view_occupies_layout_without_a_draw_command() {
        let ui = NativeUi::new(|context| {
            context.column().children(|context| {
                context.web_view("docs", 240.0, 120.0).url("about:blank");
            });
        });
        let frame = ui.render(Size::new(400.0, 300.0)).expect("frame");

        assert_eq!(frame.web_views.len(), 1);
        assert!(frame.display.commands.is_empty());
        assert_eq!(frame.stats.total_commands, 0);
        let region = &frame.web_views[0];
        assert_eq!(region.id, "docs");
        assert_eq!(region.bounds.width, 240.0);
        assert_eq!(region.bounds.height, 120.0);
        assert_eq!(region.clip, Rect::new(0.0, 0.0, 400.0, 300.0));
        assert!(region.visible);
        assert_eq!(region.z_order, 0);
    }

    #[test]
    fn web_view_bounds_include_layout_and_physical_scale() {
        let ui = NativeUi::new(|context| {
            context.row().gap(10.0).children(|context| {
                context.text("label");
                context.web_view("docs", 100.0, 50.0).url("about:blank");
            });
        });
        ui.set_pixel_scale(2.0);
        let frame = ui.render(Size::new(400.0, 300.0)).expect("frame");
        let region = &frame.web_views[0];

        assert!(region.bounds.x > 0.0, "region follows the row layout");
        assert_eq!(region.physical_bounds, physical_rect(region.bounds, 2.0));
        assert_eq!(region.physical_clip, Rect::new(0.0, 0.0, 800.0, 600.0));
    }

    #[test]
    fn hidden_web_view_keeps_its_layout_space() {
        let ui = NativeUi::new(|context| {
            context.column().children(|context| {
                context
                    .web_view("docs", 100.0, 50.0)
                    .url("about:blank")
                    .visible(false);
                context.text("below");
            });
        });
        let frame = ui.render(Size::new(400.0, 300.0)).expect("frame");
        let region = &frame.web_views[0];

        assert!(!region.visible);
        assert_eq!(region.bounds.height, 50.0);
        let below = frame
            .layout
            .iter()
            .find(|result| result.height > 0.0 && result.y >= 50.0)
            .expect("text below the hidden region");
        assert!(below.y >= 50.0);
    }

    #[test]
    fn scroll_view_offsets_bounds_and_clips_the_region() {
        let ui = NativeUi::new(|context| {
            context.scroll_view_styled(
                "list",
                100.0,
                ScrollStyle {
                    initial_offset: Some(60.0),
                    ..ScrollStyle::default()
                },
                |context| {
                    context.web_view("docs", 200.0, 80.0).url("about:blank");
                    context.container().height(200.0).children(|_| {});
                },
            );
        });
        let frame = ui.render(Size::new(400.0, 300.0)).expect("frame");
        let region = &frame.web_views[0];

        assert_eq!(region.bounds.y, -60.0);
        assert_eq!(region.clip.y, 0.0);
        assert_eq!(region.clip.height, 100.0);
        assert!(region.visible, "20 pixels remain inside the viewport");
        assert!(region.contains(200.0, 10.0));
        assert!(!region.contains(200.0, 30.0), "the clipped part is not hit");
    }

    #[test]
    fn a_region_scrolled_out_of_view_is_hidden() {
        let ui = NativeUi::new(|context| {
            context.scroll_view_styled(
                "list",
                100.0,
                ScrollStyle {
                    initial_offset: Some(f32::MAX),
                    ..ScrollStyle::default()
                },
                |context| {
                    context.web_view("docs", 200.0, 80.0).url("about:blank");
                    context.container().height(200.0).children(|_| {});
                },
            );
        });
        let frame = ui.render(Size::new(400.0, 300.0)).expect("frame");
        let region = &frame.web_views[0];

        assert!(!region.visible);
        assert!(!region.contains(200.0, 10.0));
    }

    #[test]
    fn nested_scroll_moves_the_outer_clip() {
        let ui = NativeUi::new(|context| {
            context.scroll_view_styled(
                "outer",
                120.0,
                ScrollStyle {
                    initial_offset: Some(20.0),
                    ..ScrollStyle::default()
                },
                |context| {
                    context.scroll_view_styled(
                        "inner",
                        100.0,
                        ScrollStyle {
                            initial_offset: Some(10.0),
                            ..ScrollStyle::default()
                        },
                        |context| {
                            context.web_view("docs", 200.0, 80.0).url("about:blank");
                            context.container().height(200.0).children(|_| {});
                        },
                    );
                    context.container().height(200.0).children(|_| {});
                },
            );
        });
        let frame = ui.render(Size::new(400.0, 300.0)).expect("frame");
        let region = &frame.web_views[0];

        assert_eq!(region.bounds.y, -30.0);
        // The inner viewport starts at -20 in window space, so the window's
        // top edge cuts 20 pixels off it: 100 - 20 = 80 visible pixels.
        assert_eq!(region.clip.y, 0.0);
        assert_eq!(region.clip.height, 80.0);
        assert!(region.contains(100.0, 10.0));
        assert!(!region.contains(100.0, 90.0));
    }

    #[test]
    fn region_options_are_recorded_per_frame() {
        let ui = NativeUi::new(|context| {
            context
                .web_view("docs", 200.0, 100.0)
                .html("<p>hello</p>")
                .initialization_script("window.injected = true;")
                .transparent(true)
                .navigation(WebViewNavigationPolicy::AllowHosts(vec![
                    "example.com".to_string()
                ]))
                .permissions(WebViewPermissionPolicy::DenyAll);
        });
        let frame = ui.render(Size::new(400.0, 300.0)).expect("frame");
        let options = &frame.web_views[0].options;

        assert_eq!(options.source, WebViewSource::Html("<p>hello</p>".into()));
        assert_eq!(
            options.initialization_scripts,
            vec!["window.injected = true;".to_string()]
        );
        assert!(options.transparent);
        assert_eq!(
            options.navigation,
            WebViewNavigationPolicy::AllowHosts(vec!["example.com".to_string()])
        );
        assert_eq!(options.permissions, WebViewPermissionPolicy::DenyAll);
    }

    #[test]
    fn duplicate_ids_keep_the_last_registration() {
        let ui = NativeUi::new(|context| {
            context.column().children(|context| {
                context.web_view("docs", 100.0, 50.0).url("about:blank");
                context.web_view("docs", 120.0, 60.0).html("<p>last</p>");
            });
        });
        let frame = ui.render(Size::new(400.0, 300.0)).expect("frame");

        assert_eq!(frame.web_views.len(), 1);
        assert_eq!(frame.web_views[0].bounds.width, 120.0);
        assert_eq!(
            frame.web_views[0].options.source,
            WebViewSource::Html("<p>last</p>".into())
        );
        assert_eq!(frame.web_views[0].z_order, 0);
    }
}
