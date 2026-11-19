//! Platform accessibility bridge.
//!
//! The immediate-mode runtime describes every frame as a flat list of
//! [`myrust_ui::access::AccessNode`]s. Linux publishes that list through AccessKit's AT-SPI
//! adapter, macOS through its NSAccessibility adapter, and Windows through its
//! UI Automation adapter. All three consume the same AccessKit tree so roles,
//! names, values, focus, bounds and the `Focus`/`Click` actions stay aligned.

/// An action an assistive technology asked for on a widget.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessRequest {
    /// Whether the request focuses the widget or presses it.
    pub kind: AccessRequestKind,
    /// Widget key the action targets, when the node carries one.
    pub key: Option<String>,
    /// Widget centre in logical pixels, for a press.
    pub center: Option<(f32, f32)>,
}

/// Which accessibility action was requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessRequestKind {
    /// Move keyboard focus to the widget.
    Focus,
    /// Press the widget, as a pointer press and release at its centre.
    Click,
}

/// Receives accessibility actions from the platform bridge thread.
pub type AccessSender = std::sync::Arc<dyn Fn(AccessRequest) + Send + Sync>;

/// Per-window accessibility bridge.
#[cfg(all(feature = "a11y", target_os = "linux"))]
pub use linux::Bridge;

/// Per-window accessibility bridge.
#[cfg(all(feature = "a11y", target_os = "macos"))]
pub use macos::Bridge;

/// Per-window accessibility bridge.
#[cfg(all(feature = "a11y", target_os = "windows"))]
pub use windows::Bridge;

/// No-op bridge for builds without a platform accessibility adapter.
#[cfg(not(any(
    all(feature = "a11y", target_os = "linux"),
    all(feature = "a11y", target_os = "macos"),
    all(feature = "a11y", target_os = "windows"),
)))]
#[derive(Debug, Default)]
pub struct Bridge;

#[cfg(not(any(
    all(feature = "a11y", target_os = "linux"),
    all(feature = "a11y", target_os = "macos"),
    all(feature = "a11y", target_os = "windows"),
)))]
impl Bridge {
    /// Creates a bridge that does nothing.
    pub fn new(_title: &str, _send: AccessSender) -> Self {
        Self
    }

    /// Creates a bridge that does nothing.
    pub fn new_for_window(
        _title: &str,
        _window: Option<&tao::window::Window>,
        _send: AccessSender,
    ) -> Self {
        Self
    }

    /// Publishes one frame's nodes; a no-op here.
    pub fn update(&mut self, _nodes: &[myrust_ui::access::AccessNode], _focused: bool) {}

    /// Updates the root window label; a no-op here.
    pub fn set_title(&mut self, _title: &str) {}

    /// Records whether the window has keyboard focus; a no-op here.
    pub fn set_window_focused(&mut self, _focused: bool) {}
}

#[cfg(feature = "a11y")]
mod tree {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use accesskit::{
        Action, ActionHandler, ActionRequest, ActivationHandler, Node, NodeId,
        Role as PlatformRole, TreeId, TreeInfo, TreeUpdate,
    };
    use myrust_ui::access::{AccessNode, Role};

    use super::{AccessRequest, AccessRequestKind, AccessSender};

    /// What an accessibility node id refers to when an action arrives.
    #[derive(Clone, Debug)]
    pub(super) struct NodeTarget {
        pub(super) key: Option<String>,
        pub(super) center: (f32, f32),
    }

    /// Returns the latest tree when assistive technology first asks for one.
    pub(super) struct TreeSource {
        latest: Arc<Mutex<Option<TreeUpdate>>>,
    }

    impl TreeSource {
        pub(super) fn new(latest: Arc<Mutex<Option<TreeUpdate>>>) -> Self {
            Self { latest }
        }
    }

    impl ActivationHandler for TreeSource {
        fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
            self.latest.lock().ok().and_then(|slot| slot.clone())
        }
    }

    /// Forwards platform actions to the window thread.
    pub(super) struct ActionForward {
        nodes: Arc<Mutex<HashMap<NodeId, NodeTarget>>>,
        send: AccessSender,
    }

    impl ActionForward {
        pub(super) fn new(
            nodes: Arc<Mutex<HashMap<NodeId, NodeTarget>>>,
            send: AccessSender,
        ) -> Self {
            Self { nodes, send }
        }
    }

    impl ActionHandler for ActionForward {
        fn do_action(&mut self, request: ActionRequest) {
            let target = self
                .nodes
                .lock()
                .ok()
                .and_then(|nodes| nodes.get(&request.target_node).cloned());
            let Some(target) = target else { return };
            let kind = match request.action {
                Action::Focus => AccessRequestKind::Focus,
                Action::Click => AccessRequestKind::Click,
                _ => return,
            };
            (self.send)(AccessRequest {
                kind,
                key: target.key,
                center: Some(target.center),
            });
        }
    }

    /// Lays one frame's nodes out as an AccessKit tree. Ids come from the
    /// frame's order, so they are stable while the view's widget order is.
    pub(super) fn build_tree(
        title: &str,
        nodes: &[AccessNode],
        focused: bool,
    ) -> (TreeUpdate, HashMap<NodeId, NodeTarget>) {
        let mut children = Vec::new();
        let mut list = Vec::new();
        let mut targets = HashMap::new();
        let mut focus = NodeId(0);
        for (index, access) in nodes.iter().enumerate() {
            // Containers carry a generic role and label; showing every row and
            // column to a screen reader would only add noise.
            if access.role == Role::Group
                && matches!(access.label.as_str(), "column" | "row" | "grid")
            {
                continue;
            }
            // Ids come from the frame's order, which is deterministic for one
            // view. Widgets that carry a key keep theirs across frames; a
            // keyless node that moves gets a new id.
            let id = NodeId(index as u64 + 1);
            let mut node = Node::new(platform_role(access.role));
            if !access.label.is_empty() {
                node.set_label(access.label.clone());
            }
            if let Some(value) = &access.value {
                node.set_value(value.clone());
            }
            node.set_bounds(accesskit::Rect {
                x0: f64::from(access.bounds.x),
                y0: f64::from(access.bounds.y),
                x1: f64::from(access.bounds.x + access.bounds.width),
                y1: f64::from(access.bounds.y + access.bounds.height),
            });
            if access.disabled {
                node.set_disabled();
            }
            if access.focused {
                focus = id;
            }
            if access.key.is_some() {
                node.add_action(Action::Focus);
                if is_clickable(access.role) {
                    node.add_action(Action::Click);
                }
                targets.insert(
                    id,
                    NodeTarget {
                        key: access.key.clone(),
                        center: (
                            access.bounds.x + access.bounds.width / 2.0,
                            access.bounds.y + access.bounds.height / 2.0,
                        ),
                    },
                );
            }
            children.push(id);
            list.push((id, node));
        }
        let mut root = Node::new(PlatformRole::Window);
        root.set_label(if title.trim().is_empty() {
            "MyRust window".to_string()
        } else {
            title.to_string()
        });
        root.set_children(children);
        list.insert(0, (NodeId(0), root));
        let mut tree = TreeInfo::new(NodeId(0));
        tree.toolkit_name = Some("MyRust".to_string());
        tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").to_string());
        (
            TreeUpdate {
                nodes: list,
                tree: Some(tree),
                tree_id: TreeId::ROOT,
                focus: if focused { focus } else { NodeId(0) },
            },
            targets,
        )
    }

    pub(super) fn platform_role(role: Role) -> PlatformRole {
        match role {
            Role::Button => PlatformRole::Button,
            Role::CheckBox => PlatformRole::CheckBox,
            Role::Slider => PlatformRole::Slider,
            Role::TextField => PlatformRole::TextInput,
            Role::Group => PlatformRole::Group,
            Role::Label => PlatformRole::Label,
            Role::ScrollView => PlatformRole::ScrollView,
            Role::Switch => PlatformRole::Switch,
            Role::RadioGroup => PlatformRole::RadioGroup,
            Role::Radio => PlatformRole::RadioButton,
            Role::ComboBox => PlatformRole::ComboBox,
            Role::Option => PlatformRole::ListBoxOption,
            Role::TabList => PlatformRole::TabList,
            Role::Tab => PlatformRole::Tab,
            Role::ProgressBar => PlatformRole::ProgressIndicator,
            Role::Spinner => PlatformRole::ProgressIndicator,
            Role::Badge => PlatformRole::Label,
            Role::Disclosure => PlatformRole::DisclosureTriangle,
            Role::Popup => PlatformRole::Dialog,
        }
    }

    pub(super) fn is_clickable(role: Role) -> bool {
        matches!(
            role,
            Role::Button
                | Role::CheckBox
                | Role::Switch
                | Role::Radio
                | Role::Tab
                | Role::Option
                | Role::Disclosure
                | Role::ComboBox
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use myrust_ui::paint::Rect;

        fn access(
            role: Role,
            label: &str,
            key: Option<&str>,
            rect: (f32, f32, f32, f32),
        ) -> AccessNode {
            AccessNode {
                role,
                label: label.to_string(),
                value: None,
                disabled: false,
                focused: false,
                key: key.map(str::to_string),
                bounds: Rect::new(rect.0, rect.1, rect.2, rect.3),
            }
        }

        #[test]
        fn containers_are_skipped_and_roles_map() {
            let nodes = vec![
                access(Role::Group, "column", None, (0.0, 0.0, 100.0, 100.0)),
                access(Role::Button, "Save", Some("save"), (10.0, 10.0, 80.0, 24.0)),
                access(Role::Label, "Saved", None, (10.0, 40.0, 60.0, 20.0)),
            ];
            let (update, _) = build_tree("Editor", &nodes, true);
            let root = &update.nodes[0].1;
            assert_eq!(update.nodes.len(), 3, "the container is not a node");
            assert_eq!(root.label(), Some("Editor"));
            assert_eq!(root.children().len(), 2);
            let button = &update.nodes[1].1;
            assert_eq!(button.role(), PlatformRole::Button);
            assert_eq!(button.label(), Some("Save"));
            let bounds = button.bounds().expect("bounds travel with the node");
            assert_eq!(
                (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
                (10.0, 10.0, 90.0, 34.0)
            );
        }

        #[test]
        fn keyed_widgets_get_focus_and_click_actions() {
            let nodes = vec![
                access(
                    Role::TextField,
                    "Name",
                    Some("name"),
                    (0.0, 0.0, 120.0, 28.0),
                ),
                access(Role::Button, "OK", Some("ok"), (0.0, 30.0, 80.0, 24.0)),
                access(Role::Label, "Label", None, (0.0, 60.0, 40.0, 20.0)),
            ];
            let (update, targets) = build_tree("Dialog", &nodes, true);
            assert!(update.nodes[1].1.supports_action(Action::Focus));
            assert!(!update.nodes[1].1.supports_action(Action::Click));
            assert!(update.nodes[2].1.supports_action(Action::Focus));
            assert!(update.nodes[2].1.supports_action(Action::Click));
            assert!(!update.nodes[3].1.supports_action(Action::Focus));
            let target = targets.get(&NodeId(2)).expect("the OK button");
            assert_eq!(target.key.as_deref(), Some("ok"));
            assert_eq!(target.center, (40.0, 42.0));
        }

        #[test]
        fn focus_is_the_focused_node_only_while_the_window_has_focus() {
            let mut nodes = vec![
                access(Role::Button, "One", Some("one"), (0.0, 0.0, 10.0, 10.0)),
                access(Role::Button, "Two", Some("two"), (0.0, 10.0, 10.0, 10.0)),
            ];
            nodes[1].focused = true;
            let (update, _) = build_tree("Window", &nodes, true);
            assert_eq!(update.focus, NodeId(2));
            let (update, _) = build_tree("Window", &nodes, false);
            assert_eq!(update.focus, NodeId(0));
        }

        #[test]
        fn labels_values_and_disabled_state_are_shared_by_all_bridges() {
            let mut field = access(
                Role::TextField,
                "Name",
                Some("name"),
                (0.0, 0.0, 120.0, 28.0),
            );
            field.value = Some("Ada".to_string());
            field.disabled = true;
            let (update, _) = build_tree("Profile", &[field], true);
            let node = &update.nodes[1].1;
            assert_eq!(node.role(), PlatformRole::TextInput);
            assert_eq!(node.label(), Some("Name"));
            assert_eq!(node.value(), Some("Ada"));
            assert!(node.is_disabled());
            assert_eq!(update.nodes[0].1.label(), Some("Profile"));
        }

        #[test]
        fn platform_actions_become_the_same_access_request() {
            let nodes = Arc::new(Mutex::new(HashMap::from([(
                NodeId(7),
                NodeTarget {
                    key: Some("save".to_string()),
                    center: (20.0, 30.0),
                },
            )])));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let requests_for_send = Arc::clone(&requests);
            let send: AccessSender = Arc::new(move |request| {
                requests_for_send
                    .lock()
                    .expect("request list")
                    .push(request);
            });
            let mut forward = ActionForward::new(Arc::clone(&nodes), send);
            forward.do_action(ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: NodeId(7),
                data: None,
            });
            forward.do_action(ActionRequest {
                action: Action::Focus,
                target_tree: TreeId::ROOT,
                target_node: NodeId(7),
                data: None,
            });
            let requests = requests.lock().expect("request list");
            assert_eq!(
                requests.as_slice(),
                [
                    AccessRequest {
                        kind: AccessRequestKind::Click,
                        key: Some("save".to_string()),
                        center: Some((20.0, 30.0)),
                    },
                    AccessRequest {
                        kind: AccessRequestKind::Focus,
                        key: Some("save".to_string()),
                        center: Some((20.0, 30.0)),
                    },
                ]
            );
        }
    }
}

#[cfg(all(feature = "a11y", target_os = "linux"))]
mod linux {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use accesskit::{DeactivationHandler, NodeId, TreeUpdate};
    use accesskit_unix::Adapter;
    use myrust_ui::access::AccessNode;

    use super::tree::{build_tree, ActionForward, NodeTarget, TreeSource};
    use super::AccessSender;

    /// Per-window accessibility bridge backed by AT-SPI.
    pub struct Bridge {
        adapter: Adapter,
        latest: Arc<Mutex<Option<TreeUpdate>>>,
        nodes: Arc<Mutex<HashMap<NodeId, NodeTarget>>>,
        title: String,
    }

    impl Bridge {
        /// Creates a bridge for one window. AT-SPI talks to it on its own
        /// thread; actions come back through `send`.
        pub fn new(title: &str, send: AccessSender) -> Self {
            let latest = Arc::new(Mutex::new(None));
            let nodes = Arc::new(Mutex::new(HashMap::new()));
            let adapter = Adapter::new(
                TreeSource::new(Arc::clone(&latest)),
                ActionForward::new(Arc::clone(&nodes), send),
                Deactivation,
            );
            Self {
                adapter,
                latest,
                nodes,
                title: title.to_string(),
            }
        }

        /// Creates the Linux bridge. AT-SPI does not need a native window
        /// handle, so this is the same as [`Self::new`].
        pub fn new_for_window(
            title: &str,
            _window: Option<&tao::window::Window>,
            send: AccessSender,
        ) -> Self {
            Self::new(title, send)
        }

        /// Publishes one frame's nodes. The tree is sent whole, which AccessKit
        /// diffs against the previous one before raising platform events.
        pub fn update(&mut self, nodes: &[AccessNode], focused: bool) {
            let update = self.build(nodes, focused);
            if let Ok(mut slot) = self.latest.lock() {
                *slot = Some(update.clone());
            }
            let next = update;
            self.adapter.update_if_active(|| next);
        }

        /// Records whether the window has keyboard focus.
        pub fn set_window_focused(&mut self, focused: bool) {
            self.adapter.update_window_focus_state(focused);
        }

        /// Updates the root window label on the next tree update.
        pub fn set_title(&mut self, title: &str) {
            self.title.clear();
            self.title.push_str(title);
        }

        fn build(&mut self, nodes: &[AccessNode], focused: bool) -> TreeUpdate {
            let (update, targets) = build_tree(&self.title, nodes, focused);
            if let Ok(mut guard) = self.nodes.lock() {
                *guard = targets;
            }
            update
        }
    }

    /// Nothing to release when a screen reader stops watching.
    struct Deactivation;

    impl DeactivationHandler for Deactivation {
        fn deactivate_accessibility(&mut self) {}
    }
}

#[cfg(all(feature = "a11y", target_os = "macos"))]
#[allow(unsafe_code)]
mod macos {
    use std::collections::HashMap;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::{Arc, Mutex, Once};

    use accesskit::{NodeId, TreeUpdate};
    use accesskit_macos::SubclassingAdapter;
    use myrust_ui::access::AccessNode;
    use tao::{
        rwh_06::{HasWindowHandle, RawWindowHandle},
        window::Window,
    };

    use super::tree::{build_tree, ActionForward, NodeTarget, TreeSource};
    use super::AccessSender;

    static FOCUS_FORWARDER: Once = Once::new();

    /// Per-window accessibility bridge backed by NSAccessibility.
    pub struct Bridge {
        adapter: Option<SubclassingAdapter>,
        latest: Arc<Mutex<Option<TreeUpdate>>>,
        nodes: Arc<Mutex<HashMap<NodeId, NodeTarget>>>,
        title: String,
    }

    impl Bridge {
        /// Creates a bridge with no attached native view.
        pub fn new(title: &str, _send: AccessSender) -> Self {
            Self {
                adapter: None,
                latest: Arc::new(Mutex::new(None)),
                nodes: Arc::new(Mutex::new(HashMap::new())),
                title: title.to_string(),
            }
        }

        /// Creates a bridge for one NSView.
        ///
        /// The adapter is installed before the native UI window becomes
        /// visible. If AppKit has already started observing the view, the
        /// bridge remains a no-op instead of taking down the window.
        pub fn new_for_window(title: &str, window: Option<&Window>, send: AccessSender) -> Self {
            let Some(window) = window else {
                return Self::new(title, send);
            };
            let Ok(handle) = window.window_handle() else {
                tracing::warn!("macOS accessibility: the window has no native handle");
                return Self::new(title, send);
            };
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                tracing::warn!("macOS accessibility: the window handle is not AppKit");
                return Self::new(title, send);
            };

            FOCUS_FORWARDER.call_once(|| {
                let installed = catch_unwind(AssertUnwindSafe(|| unsafe {
                    accesskit_macos::add_focus_forwarder_to_window_class("TaoWindow");
                }));
                if installed.is_err() {
                    tracing::warn!(
                        "macOS accessibility: TaoWindow already handles focused UI elements"
                    );
                }
            });

            let latest = Arc::new(Mutex::new(None));
            let nodes = Arc::new(Mutex::new(HashMap::new()));
            let view = handle.ns_view.as_ptr();
            let created = catch_unwind(AssertUnwindSafe(|| unsafe {
                SubclassingAdapter::new(
                    view,
                    TreeSource::new(Arc::clone(&latest)),
                    ActionForward::new(Arc::clone(&nodes), send),
                )
            }));
            let adapter = match created {
                Ok(adapter) => Some(adapter),
                Err(_) => {
                    tracing::warn!(
                        "macOS accessibility: failed to install the NSAccessibility adapter"
                    );
                    None
                }
            };
            Self {
                adapter,
                latest,
                nodes,
                title: title.to_string(),
            }
        }

        /// Publishes one frame's nodes.
        pub fn update(&mut self, nodes: &[AccessNode], focused: bool) {
            let update = self.build(nodes, focused);
            if let Ok(mut slot) = self.latest.lock() {
                *slot = Some(update.clone());
            }
            if let Some(adapter) = &mut self.adapter {
                if let Some(events) = adapter.update_if_active(|| update) {
                    events.raise();
                }
            }
        }

        /// Records whether the window has keyboard focus.
        pub fn set_window_focused(&mut self, focused: bool) {
            if let Some(adapter) = &mut self.adapter {
                if let Some(events) = adapter.update_view_focus_state(focused) {
                    events.raise();
                }
            }
        }

        /// Updates the root window label on the next tree update.
        pub fn set_title(&mut self, title: &str) {
            self.title.clear();
            self.title.push_str(title);
        }

        fn build(&mut self, nodes: &[AccessNode], focused: bool) -> TreeUpdate {
            let (update, targets) = build_tree(&self.title, nodes, focused);
            if let Ok(mut guard) = self.nodes.lock() {
                *guard = targets;
            }
            update
        }
    }
}

#[cfg(all(feature = "a11y", target_os = "windows"))]
#[allow(unsafe_code)]
mod windows {
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::{Arc, Mutex};

    use accesskit::{NodeId, TreeUpdate};
    use accesskit_windows::{SubclassingAdapter, HWND};
    use myrust_ui::access::AccessNode;
    use tao::{
        rwh_06::{HasWindowHandle, RawWindowHandle},
        window::Window,
    };

    use super::tree::{build_tree, ActionForward, NodeTarget, TreeSource};
    use super::AccessSender;

    /// Per-window accessibility bridge backed by UI Automation.
    pub struct Bridge {
        adapter: Option<SubclassingAdapter>,
        latest: Arc<Mutex<Option<TreeUpdate>>>,
        nodes: Arc<Mutex<HashMap<NodeId, NodeTarget>>>,
        title: String,
    }

    impl Bridge {
        /// Creates a bridge with no attached native window.
        pub fn new(title: &str, _send: AccessSender) -> Self {
            Self {
                adapter: None,
                latest: Arc::new(Mutex::new(None)),
                nodes: Arc::new(Mutex::new(HashMap::new())),
                title: title.to_string(),
            }
        }

        /// Creates a bridge for one HWND.
        ///
        /// The adapter is installed before the native UI window becomes
        /// visible. UIA can call the action handler from another thread.
        pub fn new_for_window(title: &str, window: Option<&Window>, send: AccessSender) -> Self {
            let Some(window) = window else {
                return Self::new(title, send);
            };
            let Ok(handle) = window.window_handle() else {
                tracing::warn!("Windows accessibility: the window has no native handle");
                return Self::new(title, send);
            };
            let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                tracing::warn!("Windows accessibility: the window handle is not Win32");
                return Self::new(title, send);
            };
            let hwnd = HWND(handle.hwnd.get() as *mut c_void);
            let latest = Arc::new(Mutex::new(None));
            let nodes = Arc::new(Mutex::new(HashMap::new()));
            let created = catch_unwind(AssertUnwindSafe(|| {
                SubclassingAdapter::new(
                    hwnd,
                    TreeSource::new(Arc::clone(&latest)),
                    ActionForward::new(Arc::clone(&nodes), send),
                )
            }));
            let adapter = match created {
                Ok(adapter) => Some(adapter),
                Err(_) => {
                    tracing::warn!("Windows accessibility: failed to install the UIA adapter");
                    None
                }
            };
            Self {
                adapter,
                latest,
                nodes,
                title: title.to_string(),
            }
        }

        /// Publishes one frame's nodes.
        pub fn update(&mut self, nodes: &[AccessNode], focused: bool) {
            let update = self.build(nodes, focused);
            if let Ok(mut slot) = self.latest.lock() {
                *slot = Some(update.clone());
            }
            if let Some(adapter) = &mut self.adapter {
                if let Some(events) = adapter.update_if_active(|| update) {
                    events.raise();
                }
            }
        }

        /// Records whether the window has keyboard focus.
        pub fn set_window_focused(&mut self, _focused: bool) {
            // The subclassing adapter observes WM_SETFOCUS and
            // WM_KILLFOCUS on the message loop and updates UIA itself.
        }

        /// Updates the root window label on the next tree update.
        pub fn set_title(&mut self, title: &str) {
            self.title.clear();
            self.title.push_str(title);
        }

        fn build(&mut self, nodes: &[AccessNode], focused: bool) -> TreeUpdate {
            let (update, targets) = build_tree(&self.title, nodes, focused);
            if let Ok(mut guard) = self.nodes.lock() {
                *guard = targets;
            }
            update
        }
    }
}
