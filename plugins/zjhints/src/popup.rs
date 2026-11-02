//! The names zjhints' bars and hover-preview popups know each other by. The
//! popup itself is `zjkit::popup`.

pub use zjkit::popup::PreviewPopup;

/// Messages `zjhints::hover_*`, configuration keys `zjhints_internal_preview*`.
pub static NAMES: zjkit::popup::Names = zjkit::popup_names!("zjhints");

/// The configuration keys only a popup is opened with, which the settings
/// screen refuses to add.
pub const INTERNAL_OPTIONS: &[&str] = &NAMES.internal_options();

#[cfg(test)]
mod tests {
    use super::*;

    /// A popup and its bar can be different builds of zjhints across an
    /// upgrade, so these strings never change.
    #[test]
    fn names_are_unchanged() {
        assert_eq!(NAMES.opened, "zjhints::hover_opened");
        assert_eq!(NAMES.closed, "zjhints::hover_closed");
        assert_eq!(NAMES.close, "zjhints::hover_close");
        assert_eq!(NAMES.picked, "zjhints::hover_picked");
        assert_eq!(
            INTERNAL_OPTIONS,
            [
                "zjhints_internal_preview",
                "zjhints_internal_preview_tag",
                "zjhints_internal_preview_entries",
            ]
        );
    }
}
