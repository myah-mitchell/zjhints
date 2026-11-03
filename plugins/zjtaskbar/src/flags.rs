//! The indicators an entry's `{flags}` placeholder shows: one per state of
//! the pane worth noticing from the bar, each drawn from a format option of
//! its own.

use crate::{
    CONFIG_EXITED_FORMAT, CONFIG_MINIMIZED_FORMAT, CONFIG_OTHER_TAB_FORMAT, CONFIG_PLUGIN_FORMAT,
    DEFAULT_EXITED_FORMAT, DEFAULT_MINIMIZED_FORMAT, DEFAULT_OTHER_TAB_FORMAT,
    DEFAULT_PLUGIN_FORMAT,
};

/// One indicator `{flags}` can show.
pub(crate) struct Flag {
    /// The format option the indicator is drawn from.
    pub option: &'static str,
    /// What the indicator shows when its option is unset.
    pub default: &'static str,
}

/// Every indicator, in the order `{flags}` draws them. The order of
/// `Entry::flags` follows it.
pub(crate) const FLAGS: [Flag; 4] = [
    // The pane is hidden from the screen and kept on the bar, so it can be
    // brought back from there.
    Flag {
        option: CONFIG_MINIMIZED_FORMAT,
        default: DEFAULT_MINIMIZED_FORMAT,
    },
    // The pane's command has finished, and Zellij holds the pane open so its
    // output can still be read.
    Flag {
        option: CONFIG_EXITED_FORMAT,
        default: DEFAULT_EXITED_FORMAT,
    },
    // The pane runs a plugin rather than a terminal.
    Flag {
        option: CONFIG_PLUGIN_FORMAT,
        default: DEFAULT_PLUGIN_FORMAT,
    },
    // The pane is in a tab other than the active one. Its format may hold
    // `{tab}`, for the name of that tab.
    Flag {
        option: CONFIG_OTHER_TAB_FORMAT,
        default: DEFAULT_OTHER_TAB_FORMAT,
    },
];
