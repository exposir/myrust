//! Desktop notifications built on `notify-rust`.

use std::{sync::Arc, thread};

use anyhow::{Context, Result};

type ClickHandler = Arc<dyn Fn() + Send + Sync + 'static>;

#[derive(Clone)]
struct NotificationConfig {
    title: String,
    body: String,
    icon: Option<String>,
    on_click: Option<ClickHandler>,
}

/// Builder for a native desktop notification.
pub struct DesktopNotification {
    config: NotificationConfig,
}

impl DesktopNotification {
    /// Creates a notification with a title and body.
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            config: NotificationConfig {
                title: title.into(),
                body: body.into(),
                icon: None,
                on_click: None,
            },
        }
    }

    /// Sets an icon theme name or platform-supported icon path.
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.config.icon = Some(icon.into());
        self
    }

    /// Registers a callback for the notification's default click action.
    ///
    /// With a click handler, the notification is shown on a background thread
    /// so the native event loop is not blocked while awaiting the action.
    pub fn on_click<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.config.on_click = Some(Arc::new(callback));
        self
    }

    /// Shows the notification.
    pub fn send(self) -> Result<()> {
        if self.config.on_click.is_some() {
            let config = self.config;
            thread::spawn(move || {
                if let Err(error) = show_notification(config) {
                    tracing::error!(%error, "failed to show notification");
                }
            });
            Ok(())
        } else {
            show_notification(self.config)
        }
    }
}

fn show_notification(config: NotificationConfig) -> Result<()> {
    let mut notification = notify_rust::Notification::new();
    notification.summary(&config.title).body(&config.body);

    if let Some(icon) = &config.icon {
        notification.icon(icon);
    }

    if let Some(callback) = config.on_click {
        notification.action("default", "Open");
        let handle = notification
            .show()
            .context("failed to show desktop notification")?;
        handle.wait_for_action(move |action| {
            if action == "default" {
                callback();
            }
        });
    } else {
        notification
            .show()
            .context("failed to show desktop notification")?;
    }

    Ok(())
}
