//! The indicators a tab's `{flags}` placeholder shows: one per state of the
//! tab worth noticing from the bar, each drawn from a format option of its
//! own.

use crate::{
    CONFIG_BELL_FORMAT, CONFIG_FLOATING_FORMAT, CONFIG_FULLSCREEN_FORMAT, CONFIG_SYNC_FORMAT,
    DEFAULT_BELL_FORMAT, DEFAULT_FLOATING_FORMAT, DEFAULT_FULLSCREEN_FORMAT, DEFAULT_SYNC_FORMAT,
};

/// One indicator `{flags}` can show.
pub(crate) struct Flag {
    /// The format option the indicator is drawn from.
    pub option: &'static str,
    /// What the indicator shows when its option is unset.
    pub default: &'static str,
}

/// Every indicator, in the order `{flags}` draws them.
pub(crate) const FLAGS: [Flag; 4] = [
    // A pane of the tab fills the whole tab.
    Flag {
        option: CONFIG_FULLSCREEN_FORMAT,
        default: DEFAULT_FULLSCREEN_FORMAT,
    },
    // What is typed goes to every pane of the tab.
    Flag {
        option: CONFIG_SYNC_FORMAT,
        default: DEFAULT_SYNC_FORMAT,
    },
    // The tab's floating panes are shown over its tiled ones.
    Flag {
        option: CONFIG_FLOATING_FORMAT,
        default: DEFAULT_FLOATING_FORMAT,
    },
    // A pane of the tab rang the bell, or has something that needs the user.
    Flag {
        option: CONFIG_BELL_FORMAT,
        default: DEFAULT_BELL_FORMAT,
    },
];
