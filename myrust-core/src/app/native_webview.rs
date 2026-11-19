//! Pure decisions for wry child views embedded in NativeUi regions.
//!
//! The UI crate reports logical and physical rectangles once per frame. This
//! module turns them into the physical view plans the window applies: which
//! views exist, where they sit, whether they are visible, and what changed
//! since the previous frame. Creating or updating the platform view lives in
//! [`super::NativeUiWindow`].

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ui::{
        Rect as UiRect, WebViewNavigationPolicy, WebViewOptions, WebViewPermissionPolicy,
        WebViewRegion,
    },
    webview::{NavigationPolicy, PageSource, PermissionPolicy, RegionPolicy, WebViewConfig},
};

/// Integer physical-pixel rectangle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PhysicalRect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl PhysicalRect {
    pub(crate) const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Converts a UI physical rectangle, rounding to whole device pixels.
    pub(crate) fn from_ui(rect: UiRect) -> Self {
        fn pixel(value: f32) -> i32 {
            if value.is_finite() {
                value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
            } else {
                0
            }
        }
        Self {
            x: pixel(rect.x),
            y: pixel(rect.y),
            width: pixel(rect.width.max(0.0)).max(0) as u32,
            height: pixel(rect.height.max(0.0)).max(0) as u32,
        }
    }

    pub(crate) const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Returns the overlapping rectangle, or an empty one at the origin.
    pub(crate) fn intersection(self, other: Self) -> Self {
        let x = i64::from(self.x).max(i64::from(other.x));
        let y = i64::from(self.y).max(i64::from(other.y));
        let right = (i64::from(self.x) + i64::from(self.width))
            .min(i64::from(other.x) + i64::from(other.width));
        let bottom = (i64::from(self.y) + i64::from(self.height))
            .min(i64::from(other.y) + i64::from(other.height));
        Self {
            x: x as i32,
            y: y as i32,
            width: (right - x).max(0) as u32,
            height: (bottom - y).max(0) as u32,
        }
    }
}

/// One view as it should exist after this frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DesiredWebView {
    pub(crate) id: String,
    pub(crate) options: WebViewOptions,
    pub(crate) bounds: PhysicalRect,
    pub(crate) visible: bool,
    pub(crate) z_order: u32,
}

/// What the previous frame left in the window.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExistingWebView {
    pub(crate) options: WebViewOptions,
    pub(crate) bounds: PhysicalRect,
    pub(crate) visible: bool,
}

/// One platform-view operation for this frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WebViewAction {
    /// Create a native view.
    Create(DesiredWebView),
    /// Move, resize or show/hide an existing view in place.
    Update(DesiredWebView),
    /// Drop a view whose region disappeared or whose configuration changed.
    Destroy { id: String },
}

/// Converts UI regions into physical view plans.
///
/// A region is clamped to its clip and to the window viewport. Partial
/// clipping shrinks the native view because wry exposes no per-view scissor;
/// a fully clipped region is kept alive but hidden, so scrolling back does not
/// reload the page.
pub(crate) fn desired_web_views(
    regions: &[WebViewRegion],
    viewport: PhysicalRect,
) -> Vec<DesiredWebView> {
    let mut desired = regions
        .iter()
        .filter(|region| !region.id.is_empty())
        .map(|region| {
            let bounds = PhysicalRect::from_ui(region.physical_bounds);
            let clip = PhysicalRect::from_ui(region.physical_clip);
            let effective = bounds.intersection(clip).intersection(viewport);
            let visible = region.visible && !effective.is_empty();
            // A hidden view keeps its layout size. Shrinking it to the empty
            // intersection would reflow the page while it is off screen.
            let bounds = if effective.is_empty() {
                bounds
            } else {
                effective
            };
            DesiredWebView {
                id: region.id.clone(),
                options: region.options.clone(),
                bounds,
                visible,
                z_order: region.z_order,
            }
        })
        .collect::<Vec<_>>();
    desired.sort_by_key(|view| view.z_order);
    desired
}

/// Diffs desired views against the previous frame.
pub(crate) fn plan_web_views(
    existing: &BTreeMap<String, ExistingWebView>,
    desired: &[DesiredWebView],
) -> Vec<WebViewAction> {
    let desired_ids = desired
        .iter()
        .map(|view| view.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut actions = Vec::new();
    for (id, view) in existing {
        let changed = desired
            .iter()
            .find(|desired| desired.id == *id)
            .is_some_and(|desired| desired.options != view.options);
        if !desired_ids.contains(id.as_str()) || changed {
            actions.push(WebViewAction::Destroy { id: id.clone() });
        }
    }
    for desired in desired {
        match existing.get(&desired.id) {
            Some(view) if view.options == desired.options => {
                if view.bounds != desired.bounds || view.visible != desired.visible {
                    actions.push(WebViewAction::Update(desired.clone()));
                }
            }
            _ => actions.push(WebViewAction::Create(desired.clone())),
        }
    }
    actions
}

/// Returns the topmost visible WebView region under a logical point.
pub(crate) fn web_view_at(regions: &[WebViewRegion], x: f32, y: f32) -> Option<&str> {
    regions
        .iter()
        .filter(|region| region.contains(x, y))
        .max_by_key(|region| region.z_order)
        .map(|region| region.id.as_str())
}

/// Converts UI options into the core WebView configuration and region policy.
pub(crate) fn web_view_config(options: &WebViewOptions) -> (WebViewConfig, RegionPolicy) {
    let config = WebViewConfig {
        source: match &options.source {
            crate::ui::WebViewSource::Url(url) => PageSource::Url(url.clone()),
            crate::ui::WebViewSource::Html(html) => PageSource::Html(html.clone()),
        },
        ..WebViewConfig::default()
    };
    let policy = RegionPolicy {
        initialization_scripts: options.initialization_scripts.clone(),
        transparent: options.transparent,
        navigation: match &options.navigation {
            WebViewNavigationPolicy::AllowAll => NavigationPolicy::AllowAll,
            WebViewNavigationPolicy::DenyAll => NavigationPolicy::DenyAll,
            WebViewNavigationPolicy::AllowHosts(hosts) => {
                NavigationPolicy::AllowHosts(hosts.clone())
            }
        },
        permissions: match options.permissions {
            WebViewPermissionPolicy::PlatformDefault => PermissionPolicy::PlatformDefault,
            WebViewPermissionPolicy::DenyAll => PermissionPolicy::DenyAll,
            WebViewPermissionPolicy::AllowAll => PermissionPolicy::AllowAll,
        },
    };
    (config, policy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{WebViewOptions, WebViewRegion, WebViewSource};

    fn region(id: &str, bounds: UiRect, clip: UiRect, visible: bool, z: u32) -> WebViewRegion {
        WebViewRegion {
            id: id.to_string(),
            bounds,
            clip,
            physical_bounds: bounds,
            physical_clip: clip,
            visible,
            z_order: z,
            options: WebViewOptions::default(),
        }
    }

    #[test]
    fn physical_bounds_are_intersected_with_clip_and_viewport() {
        let region = region(
            "docs",
            UiRect::new(20.0, 30.0, 300.0, 200.0),
            UiRect::new(0.0, 0.0, 200.0, 100.0),
            true,
            0,
        );
        let desired = desired_web_views(&[region], PhysicalRect::new(0, 0, 160, 90));

        assert_eq!(desired.len(), 1);
        assert_eq!(desired[0].bounds, PhysicalRect::new(20, 30, 140, 60));
        assert!(desired[0].visible);
    }

    #[test]
    fn fully_clipped_region_stays_alive_but_hidden() {
        let region = region(
            "docs",
            UiRect::new(0.0, 200.0, 100.0, 50.0),
            UiRect::new(0.0, 0.0, 100.0, 100.0),
            false,
            0,
        );
        let desired = desired_web_views(&[region], PhysicalRect::new(0, 0, 100, 100));

        assert_eq!(desired.len(), 1);
        assert!(!desired[0].visible);
        assert_eq!(desired[0].bounds.height, 50);
    }

    #[test]
    fn plan_creates_updates_and_destroys_in_stable_order() {
        let mut existing = BTreeMap::new();
        existing.insert(
            "moved".to_string(),
            ExistingWebView {
                options: WebViewOptions::default(),
                bounds: PhysicalRect::new(0, 0, 10, 10),
                visible: true,
            },
        );
        existing.insert(
            "gone".to_string(),
            ExistingWebView {
                options: WebViewOptions::default(),
                bounds: PhysicalRect::new(0, 0, 10, 10),
                visible: true,
            },
        );
        let desired = vec![
            DesiredWebView {
                id: "moved".to_string(),
                options: WebViewOptions::default(),
                bounds: PhysicalRect::new(5, 5, 10, 10),
                visible: true,
                z_order: 0,
            },
            DesiredWebView {
                id: "new".to_string(),
                options: WebViewOptions::default(),
                bounds: PhysicalRect::new(0, 0, 20, 20),
                visible: true,
                z_order: 1,
            },
        ];

        assert_eq!(
            plan_web_views(&existing, &desired),
            vec![
                WebViewAction::Destroy {
                    id: "gone".to_string()
                },
                WebViewAction::Update(desired[0].clone()),
                WebViewAction::Create(desired[1].clone()),
            ]
        );
    }

    #[test]
    fn config_changes_recreate_the_view() {
        let mut existing = BTreeMap::new();
        existing.insert(
            "docs".to_string(),
            ExistingWebView {
                options: WebViewOptions::default(),
                bounds: PhysicalRect::new(0, 0, 10, 10),
                visible: true,
            },
        );
        let desired = vec![DesiredWebView {
            id: "docs".to_string(),
            options: WebViewOptions {
                source: WebViewSource::Html("<p>new</p>".to_string()),
                ..WebViewOptions::default()
            },
            bounds: PhysicalRect::new(0, 0, 10, 10),
            visible: true,
            z_order: 0,
        }];

        assert_eq!(
            plan_web_views(&existing, &desired),
            vec![
                WebViewAction::Destroy {
                    id: "docs".to_string()
                },
                WebViewAction::Create(desired[0].clone()),
            ]
        );
    }

    #[test]
    fn hit_testing_prefers_the_topmost_region() {
        let regions = vec![
            region(
                "below",
                UiRect::new(0.0, 0.0, 100.0, 100.0),
                UiRect::new(0.0, 0.0, 100.0, 100.0),
                true,
                0,
            ),
            region(
                "above",
                UiRect::new(10.0, 10.0, 50.0, 50.0),
                UiRect::new(0.0, 0.0, 100.0, 100.0),
                true,
                1,
            ),
        ];

        assert_eq!(web_view_at(&regions, 20.0, 20.0), Some("above"));
        assert_eq!(web_view_at(&regions, 80.0, 80.0), Some("below"));
        assert_eq!(web_view_at(&regions, 200.0, 200.0), None);
    }

    #[test]
    fn options_map_to_core_security_policies() {
        let options = WebViewOptions {
            source: WebViewSource::Html("<p>hi</p>".to_string()),
            initialization_scripts: vec!["window.x = 1;".to_string()],
            transparent: true,
            navigation: WebViewNavigationPolicy::AllowHosts(vec!["example.com".to_string()]),
            permissions: WebViewPermissionPolicy::DenyAll,
        };
        let (config, policy) = web_view_config(&options);

        assert!(matches!(config.source, PageSource::Html(ref html) if html == "<p>hi</p>"));
        assert_eq!(policy.initialization_scripts, options.initialization_scripts);
        assert!(policy.transparent);
        assert!(policy.navigation.allows("https://docs.example.com/x"));
        assert!(!policy.navigation.allows("https://evil.test/x"));
        assert_eq!(policy.permissions, PermissionPolicy::DenyAll);
    }
}
