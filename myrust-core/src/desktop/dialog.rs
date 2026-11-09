//! Native file and message dialogs built on `rfd`.

use std::path::PathBuf;

/// A file type filter shown by native open/save dialogs.
#[derive(Debug, Clone)]
pub struct FileFilter {
    /// Human-readable filter label.
    pub name: String,
    /// File extensions without a leading dot.
    pub extensions: Vec<String>,
}

impl FileFilter {
    /// Creates a file filter.
    pub fn new(
        name: impl Into<String>,
        extensions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            extensions: extensions.into_iter().map(Into::into).collect(),
        }
    }
}

/// Options shared by native file open and save dialogs.
#[derive(Debug, Clone, Default)]
pub struct FileDialogOptions {
    /// Dialog title.
    pub title: Option<String>,
    /// Initial directory.
    pub directory: Option<PathBuf>,
    /// File type filters.
    pub filters: Vec<FileFilter>,
}

/// Severity shown by a native message dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogLevel {
    /// Informational message.
    Info,
    /// Warning message.
    Warning,
    /// Error message.
    Error,
}

/// Button pressed in a native message dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogButton {
    /// OK was pressed.
    Ok,
    /// Cancel was pressed or the dialog was dismissed.
    Cancel,
    /// Yes was pressed.
    Yes,
    /// No was pressed.
    No,
}

/// Options for a native message dialog.
#[derive(Debug, Clone)]
pub struct MessageDialogOptions {
    /// Dialog title.
    pub title: String,
    /// Main message body.
    pub message: String,
    /// Message severity.
    pub level: DialogLevel,
    /// Buttons to show.
    pub buttons: MessageButtons,
}

/// Supported native message dialog button sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageButtons {
    /// A single OK button.
    Ok,
    /// OK and Cancel.
    OkCancel,
    /// Yes and No.
    YesNo,
    /// Yes, No, and Cancel.
    YesNoCancel,
}

impl MessageDialogOptions {
    /// Creates an informational OK dialog.
    pub fn info(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            level: DialogLevel::Info,
            buttons: MessageButtons::Ok,
        }
    }
}

/// Native dialog entry points.
pub struct Dialog;

impl Dialog {
    /// Opens a native file picker and returns the selected path.
    pub fn open_file(options: FileDialogOptions) -> Option<PathBuf> {
        configure_file_dialog(options).pick_file()
    }

    /// Opens a native multi-file picker.
    pub fn open_files(options: FileDialogOptions) -> Option<Vec<PathBuf>> {
        configure_file_dialog(options).pick_files()
    }

    /// Opens a native save dialog and returns the selected path.
    pub fn save_file(
        options: FileDialogOptions,
        suggested_name: Option<String>,
    ) -> Option<PathBuf> {
        let mut dialog = configure_file_dialog(options);
        if let Some(name) = suggested_name {
            dialog = dialog.set_file_name(name);
        }
        dialog.save_file()
    }

    /// Shows a native message dialog and maps the result to [`DialogButton`].
    pub fn message(options: MessageDialogOptions) -> DialogButton {
        use rfd::MessageDialog;

        let level = match options.level {
            DialogLevel::Info => rfd::MessageLevel::Info,
            DialogLevel::Warning => rfd::MessageLevel::Warning,
            DialogLevel::Error => rfd::MessageLevel::Error,
        };
        let buttons = match options.buttons {
            MessageButtons::Ok => rfd::MessageButtons::Ok,
            MessageButtons::OkCancel => rfd::MessageButtons::OkCancel,
            MessageButtons::YesNo => rfd::MessageButtons::YesNo,
            MessageButtons::YesNoCancel => rfd::MessageButtons::YesNoCancel,
        };

        match MessageDialog::new()
            .set_title(options.title)
            .set_description(options.message)
            .set_level(level)
            .set_buttons(buttons)
            .show()
        {
            rfd::MessageDialogResult::Ok => DialogButton::Ok,
            rfd::MessageDialogResult::Cancel => DialogButton::Cancel,
            rfd::MessageDialogResult::Yes => DialogButton::Yes,
            rfd::MessageDialogResult::No => DialogButton::No,
            rfd::MessageDialogResult::Custom(_) => DialogButton::Cancel,
        }
    }
}

fn configure_file_dialog(options: FileDialogOptions) -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new();
    if let Some(title) = options.title {
        dialog = dialog.set_title(title);
    }
    if let Some(directory) = options.directory {
        dialog = dialog.set_directory(directory);
    }
    for filter in options.filters {
        dialog = dialog.add_filter(filter.name, &filter.extensions);
    }
    dialog
}
